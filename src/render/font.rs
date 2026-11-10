//! Font management and glyph rendering.
//!
//! Memory-maps TTF files and rasterizes individual glyphs to pixel buffers.

use fontdue::{Font, FontSettings};
use lru::LruCache;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

/// A cached rasterized glyph bitmap with its rendering metrics.
///
/// Stored in both the in-memory LRU cache and on-disk binary cache
/// to avoid re-rasterizing frequently used glyphs.
pub struct CachedGlyph {
    /// Raw bitmap pixels (grayscale alpha, one byte per pixel).
    pub bitmap: Vec<u8>,
    /// Glyph metrics: dimensions, bearing, and advance.
    pub metrics: fontdue::Metrics,
}

/// Font manager that memory-maps a TTF file and caches rasterized glyphs.
///
/// Uses an LRU cache (1024 entries) for in-memory glyph caching and a
/// disk cache (`~/.cache/dock/glyphs/`) for persistence across restarts.
/// Fallback fonts are rasterized on-demand without retaining the full
/// font collection in memory.
pub struct FontManager {
    /// Primary font used for most text rendering.
    primary_font: Font,
    /// Owned font data to keep the font valid (avoids mmap SIGBUS on truncation).
    _primary_font_data: Option<Vec<u8>>,
    /// Directory for on-disk glyph cache binaries.
    cache_dir: PathBuf,
    /// In-memory LRU cache: (character, font_size) → rasterized glyph.
    cache: LruCache<(char, u32), CachedGlyph>,
}

impl FontManager {
    const GLYPH_CACHE_CAPACITY: usize = 1024;

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let data = fs::read(&path)?;
        let primary_font = Font::from_bytes(data.as_slice(), FontSettings::default())
            .map_err(|e| format!("Failed to parse font: {e}"))?;

        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("dock/glyphs");
        let _ = fs::create_dir_all(&cache_dir);

        Ok(Self {
            primary_font,
            _primary_font_data: Some(data),
            cache_dir,
            cache: LruCache::new(NonZeroUsize::new(Self::GLYPH_CACHE_CAPACITY).unwrap()),
        })
    }

    pub fn new(font_data: &[u8]) -> Self {
        let data = font_data.to_vec();
        let font =
            Font::from_bytes(data.as_slice(), FontSettings::default()).expect("Invalid font data");
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("dock/glyphs");
        let _ = fs::create_dir_all(&cache_dir);

        Self {
            primary_font: font,
            _primary_font_data: Some(data),
            cache_dir,
            cache: LruCache::new(NonZeroUsize::new(Self::GLYPH_CACHE_CAPACITY).unwrap()),
        }
    }

    pub fn get_font(&self, _character: char) -> &Font {
        &self.primary_font
    }

    fn save_glyph_bin(
        path: &Path,
        metrics: &fontdue::Metrics,
        bitmap: &[u8],
    ) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(&(metrics.width as u32).to_le_bytes())?;
        file.write_all(&(metrics.height as u32).to_le_bytes())?;
        file.write_all(&metrics.xmin.to_le_bytes())?;
        file.write_all(&metrics.ymin.to_le_bytes())?;
        file.write_all(&metrics.advance_width.to_le_bytes())?;
        file.write_all(&metrics.advance_height.to_le_bytes())?;
        file.write_all(bitmap)?;

        Ok(())
    }

    fn load_glyph_bin(&self, path: &Path, size: f32) -> Option<CachedGlyph> {
        let mut file = File::open(path).ok()?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).ok()?;

        if buf.len() < 24 {
            return None;
        }

        let width = u32::from_le_bytes(buf[0..4].try_into().ok()?) as usize;
        let height = u32::from_le_bytes(buf[4..8].try_into().ok()?) as usize;
        let xmin = i32::from_le_bytes(buf[8..12].try_into().ok()?);
        let ymin = i32::from_le_bytes(buf[12..16].try_into().ok()?);
        let advance_width = f32::from_le_bytes(buf[16..20].try_into().ok()?);
        let advance_height = f32::from_le_bytes(buf[20..24].try_into().ok()?);
        let bitmap = buf[24..].to_vec();

        let (mut metrics, _) = self.primary_font.rasterize(' ', size);
        metrics.width = width;
        metrics.height = height;
        metrics.xmin = xmin;
        metrics.ymin = ymin;
        metrics.advance_width = advance_width;
        metrics.advance_height = advance_height;

        Some(CachedGlyph { bitmap, metrics })
    }

    fn find_system_font_path_for_char(_character: char) -> Option<PathBuf> {
        // Avoid spawning subprocesses (fc-match) on the render thread.
        // Fallback glyph discovery relies on static candidate paths below.
        None
    }

    /// Rasterize one fallback glyph without retaining entire fallback font
    /// collections. Read font bytes into an owned buffer instead of mmap to
    /// avoid UB if files are modified/truncated while mapped.
    fn rasterize_glyph_from_path(
        path: &Path,
        character: char,
        size: f32,
    ) -> Option<(fontdue::Metrics, Vec<u8>)> {
        let data = fs::read(path).ok()?;

        for collection_index in 0..8 {
            let settings = FontSettings {
                collection_index,
                ..FontSettings::default()
            };
            let font = match Font::from_bytes(data.as_slice(), settings) {
                Ok(font) => font,
                Err(_) => break,
            };

            if font.lookup_glyph_index(character) == 0 {
                continue;
            }

            let (metrics, bitmap) = font.rasterize(character, size);
            if metrics.width > 0 && metrics.height > 0 && !bitmap.is_empty() {
                return Some((metrics, bitmap));
            }
        }

        None
    }

    fn extract_single_glyph(&mut self, character: char, size: f32) -> Option<CachedGlyph> {
        let bin_path = self
            .cache_dir
            .join(format!("{:x}_{}.bin", character as u32, size as u32));

        if bin_path.exists() {
            if let Some(glyph) = self.load_glyph_bin(&bin_path, size) {
                return Some(glyph);
            }
        }

        let mut candidate_paths = Vec::new();

        if let Some(fc_path) = Self::find_system_font_path_for_char(character) {
            candidate_paths.push(fc_path);
        }

        candidate_paths.extend(vec![
            PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc"),
            PathBuf::from("/usr/share/fonts/TTF/unifont.ttf"),
            PathBuf::from("/usr/share/fonts/TTF/DejaVuSans.ttf"),
        ]);

        for path in candidate_paths {
            if !path.exists() {
                continue;
            }

            if let Some((metrics, bitmap)) = Self::rasterize_glyph_from_path(&path, character, size)
            {
                let _ = Self::save_glyph_bin(&bin_path, &metrics, &bitmap);

                let (mut base_metrics, _) = self.primary_font.rasterize(' ', size);
                base_metrics.width = metrics.width;
                base_metrics.height = metrics.height;
                base_metrics.xmin = metrics.xmin;
                base_metrics.ymin = metrics.ymin;
                base_metrics.advance_width = metrics.advance_width;
                base_metrics.advance_height = metrics.advance_height;

                return Some(CachedGlyph {
                    bitmap,
                    metrics: base_metrics,
                });
            }
        }

        None
    }

    pub fn get_glyph(&mut self, character: char, size: f32) -> &CachedGlyph {
        let key = (character, size as u32);

        if !self.cache.contains(&key) {
            let primary_glyph_idx = self.primary_font.lookup_glyph_index(character);

            let mut valid_primary = false;
            let mut primary_res = None;

            if primary_glyph_idx > 0 {
                let (metrics, bitmap) = self.primary_font.rasterize(character, size);

                // Whitespace glyphs (especially U+0020) legitimately have no
                // bitmap but do have advance metrics. Treat them as valid so
                // fallback font discovery is not triggered for spaces.
                if !bitmap.is_empty()
                    || metrics.advance_width > 0.0
                    || (metrics.width == 0 && metrics.height == 0 && character.is_whitespace())
                {
                    valid_primary = true;
                    primary_res = Some(CachedGlyph { bitmap, metrics });
                }
            }

            // Only use fallback font discovery for ASCII/printable characters.
            // For CJK, emoji, and other complex scripts, draw a tofu placeholder
            // instead of blocking render-time font discovery/parsing.
            let cached_glyph = if valid_primary {
                primary_res.unwrap()
            } else if (character as u32) < 0x80 {
                // ASCII missing from primary font — try fallback candidates.
                if let Some(glyph) = self.extract_single_glyph(character, size) {
                    glyph
                } else {
                    let (metrics, bitmap) = self.primary_font.rasterize(character, size);
                    CachedGlyph { bitmap, metrics }
                }
            } else {
                // Non-ASCII character not in primary font: draw a tofu box.
                let (metrics, _) = self.primary_font.rasterize('W', size);
                let w = metrics.width.max(8);
                let h = metrics.height.max(8);
                let mut bitmap = vec![0u8; (w * h) as usize];
                if w > 0 && h > 0 {
                    for x in 0..w {
                        bitmap[x] = 255;
                        bitmap[(h - 1) * w + x] = 255;
                    }
                    for y in 0..h {
                        bitmap[y * w] = 255;
                        bitmap[y * w + (w - 1)] = 255;
                    }
                }
                CachedGlyph { bitmap, metrics }
            };

            self.cache.put(key, cached_glyph);
        }

        self.cache.get(&key).unwrap()
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Measure the pixel width of a string at the given font size.
    pub fn measure(&mut self, text: &str, size: i32) -> i32 {
        let size_f = size.max(7) as f32;
        text.chars()
            .map(|c| {
                let glyph = self.get_glyph(c, size_f);
                glyph.metrics.advance_width as i32
            })
            .sum()
    }
}
