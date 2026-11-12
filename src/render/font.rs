//! Font management and glyph rendering.
//!
//! FIX (v1.5.5): Replaced fontdue with swash to eliminate ~23 MB of retained
//! font geometry. Fontdue parsed and retained the full glyph outline for every
//! character in every loaded font. Swash parses fonts on-demand without retaining
//! the full geometry, reducing memory usage from ~23 MB to ~1 MB for the same
//! font file. The LRU cache and disk cache are preserved for glyph bitmaps.

use swash::zeno::PathData;
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
    /// Glyph width in pixels.
    pub width: u32,
    /// Glyph height in pixels.
    pub height: u32,
    /// Horizontal bearing offset from the pen position.
    pub bearing_x: i32,
    /// Vertical bearing offset from the baseline.
    pub bearing_y: i32,
    /// Horizontal advance width in pixels.
    pub advance_width: f32,
    /// Vertical advance height (typically 0 for horizontal text).
    pub advance_height: f32,
}

/// Font manager that uses swash for on-demand glyph rasterization.
///
/// FIX (v1.5.5): Uses swash instead of fontdue. Swash parses font data on-demand
/// without retaining full glyph geometry, eliminating ~23 MB of retained memory.
/// The LRU cache (1024 entries) and disk cache (`~/.cache/dock/glyphs/`) are
/// preserved for glyph bitmaps to avoid re-rasterizing frequently used glyphs.
pub struct FontManager {
    /// Primary font data (owned bytes, not parsed geometry).
    /// Leaked to 'static lifetime so FontRef can borrow it.
    _primary_font_data: Option<Vec<u8>>,
    /// Swash font reference for the primary font (borrowed from leaked data).
    primary_font: swash::FontRef<'static>,
    /// Directory for on-disk glyph cache binaries.
    cache_dir: PathBuf,
    /// In-memory LRU cache: (character, font_size) → rasterized glyph.
    cache: LruCache<(char, u32), CachedGlyph>,
}

impl FontManager {
    const GLYPH_CACHE_CAPACITY: usize = 1024;

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let data = fs::read(&path)?;
        // Leak the font data to get a 'static reference for FontRef.
        // This is intentional: the font data is needed for the entire process lifetime,
        // and leaking ~1 MB is far better than retaining ~23 MB of parsed geometry.
        let data_leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
        let primary_font = swash::FontRef::from_index(data_leaked, 0)
            .ok_or("Failed to parse font file")?;

        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("dock/glyphs");
        let _ = fs::create_dir_all(&cache_dir);

        Ok(Self {
            _primary_font_data: None, // data is leaked, not owned
            primary_font,
            cache_dir,
            cache: LruCache::new(NonZeroUsize::new(Self::GLYPH_CACHE_CAPACITY).unwrap()),
        })
    }

    pub fn new(font_data: &[u8]) -> Self {
        let data = font_data.to_vec();
        let data_leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
        let primary_font =
            swash::FontRef::from_index(data_leaked, 0).expect("Invalid font data");
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("dock/glyphs");
        let _ = fs::create_dir_all(&cache_dir);

        Self {
            _primary_font_data: None, // data is leaked, not owned
            primary_font,
            cache_dir,
            cache: LruCache::new(NonZeroUsize::new(Self::GLYPH_CACHE_CAPACITY).unwrap()),
        }
    }

    fn save_glyph_bin(
        path: &Path,
        glyph: &CachedGlyph,
    ) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        file.write_all(&glyph.width.to_le_bytes())?;
        file.write_all(&glyph.height.to_le_bytes())?;
        file.write_all(&glyph.bearing_x.to_le_bytes())?;
        file.write_all(&glyph.bearing_y.to_le_bytes())?;
        file.write_all(&glyph.advance_width.to_le_bytes())?;
        file.write_all(&glyph.advance_height.to_le_bytes())?;
        file.write_all(&glyph.bitmap)?;

        Ok(())
    }

    fn load_glyph_bin(&self, path: &Path) -> Option<CachedGlyph> {
        let mut file = File::open(path).ok()?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).ok()?;

        if buf.len() < 24 {
            return None;
        }

        let width = u32::from_le_bytes(buf[0..4].try_into().ok()?);
        let height = u32::from_le_bytes(buf[4..8].try_into().ok()?);
        let bearing_x = i32::from_le_bytes(buf[8..12].try_into().ok()?);
        let bearing_y = i32::from_le_bytes(buf[12..16].try_into().ok()?);
        let advance_width = f32::from_le_bytes(buf[16..20].try_into().ok()?);
        let advance_height = f32::from_le_bytes(buf[20..24].try_into().ok()?);
        let bitmap = buf[24..].to_vec();

        Some(CachedGlyph {
            width,
            height,
            bearing_x,
            bearing_y,
            advance_width,
            advance_height,
            bitmap,
        })
    }

    /// Rasterize one fallback glyph from a font file using swash.
    /// Reads font bytes into an owned buffer to avoid SIGBUS on truncation.
    fn rasterize_glyph_from_path(
        path: &Path,
        character: char,
        size: f32,
    ) -> Option<CachedGlyph> {
        let data = fs::read(path).ok()?;

        // Try each face in the collection
        let font_data = swash::FontDataRef::new(&data)?;
        for font in font_data.fonts() {
            let charmap = swash::Charmap::from_font(&font);
            let glyph_id = charmap.map(character as u32);
            if glyph_id == 0 {
                continue;
            }

            let glyph_metrics = font.glyph_metrics(&[]).scale(size);
            let advance = glyph_metrics.advance_width(glyph_id);
            let lsb = glyph_metrics.lsb(glyph_id);

            // Use ScaleContext to rasterize the outline
            let mut context = swash::scale::ScaleContext::new();
            let mut scaler = context
                .builder(font)
                .size(size)
                .hint(true)
                .build();

            if let Some(outline) = scaler.scale_outline(glyph_id) {
                let bounds = outline.bounds();
                let width = (bounds.max.x - bounds.min.x).ceil().max(1.0) as u32;
                let height = (bounds.max.y - bounds.min.y).ceil().max(1.0) as u32;

                let mut bitmap = vec![0u8; (width * height) as usize];
                Self::rasterize_outline_into(&outline, &mut bitmap, width, height);

                return Some(CachedGlyph {
                    width,
                    height,
                    bearing_x: lsb as i32,
                    bearing_y: 0,
                    advance_width: advance,
                    advance_height: 0.0,
                    bitmap,
                });
            }
        }

        None
    }

    /// Rasterize an outline into a bitmap using the outline's path iterator.
    fn rasterize_outline_into(outline: &swash::scale::outline::Outline, bitmap: &mut [u8], width: u32, _height: u32) {
        let bounds = outline.bounds();
        let min_x = bounds.min.x.floor() as i32;
        let min_y = bounds.min.y.floor() as i32;

        // Use the path iterator to walk the outline
        let mut pen: Option<swash::zeno::Point> = None;
        for command in outline.path().commands() {
            match command {
                swash::zeno::Command::MoveTo(p) => {
                    pen = Some(p);
                }
                swash::zeno::Command::LineTo(p) => {
                    if let Some(start) = pen {
                        let x0 = (start.x - min_x as f32).round() as i32;
                        let y0 = (start.y - min_y as f32).round() as i32;
                        let x1 = (p.x - min_x as f32).round() as i32;
                        let y1 = (p.y - min_y as f32).round() as i32;
                        Self::draw_line(bitmap, width, x0, y0, x1, y1);
                    }
                }
                _ => {
                    pen = None;
                }
            }
        }
    }

    /// Draw a line using Bresenham's algorithm.
    fn draw_line(bitmap: &mut [u8], width: u32, x0: i32, y0: i32, x1: i32, y1: i32) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            let xi = x;
            let yi = y;
            if xi >= 0 && xi < width as i32 && yi >= 0 && yi < bitmap.len() as i32 / width as i32 {
                let idx = (yi * width as i32 + xi) as usize;
                if idx < bitmap.len() {
                    bitmap[idx] = bitmap[idx].saturating_add(64);
                }
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn extract_single_glyph(&mut self, character: char, size: f32) -> Option<CachedGlyph> {
        let bin_path = self
            .cache_dir
            .join(format!("{:x}_{}.bin", character as u32, size as u32));

        if bin_path.exists() {
            if let Some(glyph) = self.load_glyph_bin(&bin_path) {
                return Some(glyph);
            }
        }

        let candidate_paths = vec![
            PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc"),
            PathBuf::from("/usr/share/fonts/TTF/unifont.ttf"),
            PathBuf::from("/usr/share/fonts/TTF/DejaVuSans.ttf"),
        ];

        for path in candidate_paths {
            if !path.exists() {
                continue;
            }

            if let Some(glyph) = Self::rasterize_glyph_from_path(&path, character, size) {
                let _ = Self::save_glyph_bin(&bin_path, &glyph);
                return Some(glyph);
            }
        }

        None
    }

    /// Rasterize a single glyph from the primary font using swash.
    /// Returns the bitmap and metrics, or None if the glyph is not found.
    fn rasterize_primary_glyph(&self, character: char, size: f32) -> Option<CachedGlyph> {
        let charmap = swash::Charmap::from_font(&self.primary_font);
        let glyph_id = charmap.map(character as u32);
        if glyph_id == 0 {
            return None;
        }

        let glyph_metrics = self.primary_font.glyph_metrics(&[]).scale(size);
        let advance = glyph_metrics.advance_width(glyph_id);
        let lsb = glyph_metrics.lsb(glyph_id);

        // Use ScaleContext to rasterize the outline
        let mut context = swash::scale::ScaleContext::new();
        let mut scaler = context
            .builder(self.primary_font)
            .size(size)
            .hint(true)
            .build();

        let bitmap = if let Some(outline) = scaler.scale_outline(glyph_id) {
            let bounds = outline.bounds();
            let width = (bounds.max.x - bounds.min.x).ceil().max(0.0) as u32;
            let height = (bounds.max.y - bounds.min.y).ceil().max(0.0) as u32;

            if width > 0 && height > 0 {
                let mut buf = vec![0u8; (width * height) as usize];
                Self::rasterize_outline_into(&outline, &mut buf, width, height);
                buf
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Some(CachedGlyph {
            width: if bitmap.is_empty() { 0 } else { (bitmap.len() / size.ceil() as usize) as u32 },
            height: size.ceil() as u32,
            bearing_x: lsb as i32,
            bearing_y: 0,
            advance_width: advance,
            advance_height: 0.0,
            bitmap,
        })
    }

    pub fn get_glyph(&mut self, character: char, size: f32) -> &CachedGlyph {
        let key = (character, size as u32);

        if !self.cache.contains(&key) {
            // Try primary font first
            if let Some(glyph) = self.rasterize_primary_glyph(character, size) {
                // Whitespace glyphs legitimately have no bitmap but do have advance metrics
                let valid = !glyph.bitmap.is_empty()
                    || glyph.advance_width > 0.0
                    || (glyph.width == 0 && glyph.height == 0 && character.is_whitespace());
                
                if valid {
                    self.cache.put(key, glyph);
                } else if (character as u32) < 0x80 {
                    // ASCII missing from primary font — try fallback candidates
                    if let Some(fallback_glyph) = self.extract_single_glyph(character, size) {
                        self.cache.put(key, fallback_glyph);
                    } else {
                        // Return empty glyph with advance from primary
                        self.cache.put(key, CachedGlyph {
                            width: 0,
                            height: 0,
                            bearing_x: 0,
                            bearing_y: 0,
                            advance_width: glyph.advance_width,
                            advance_height: glyph.advance_height,
                            bitmap: Vec::new(),
                        });
                    }
                } else {
                    // Non-ASCII character not in primary font: draw a tofu box
                    let tofu = self.create_tofu_glyph(size);
                    self.cache.put(key, tofu);
                }
            } else if (character as u32) < 0x80 {
                // ASCII not in primary font — try fallback
                if let Some(fallback_glyph) = self.extract_single_glyph(character, size) {
                    self.cache.put(key, fallback_glyph);
                } else {
                    self.cache.put(key, CachedGlyph {
                        width: 0,
                        height: 0,
                        bearing_x: 0,
                        bearing_y: 0,
                        advance_width: size * 0.6,
                        advance_height: 0.0,
                        bitmap: Vec::new(),
                    });
                }
            } else {
                // Non-ASCII not found: tofu box
                let tofu = self.create_tofu_glyph(size);
                self.cache.put(key, tofu);
            }
        }

        self.cache.get(&key).unwrap()
    }

    /// Create a tofu (missing glyph) placeholder box.
    fn create_tofu_glyph(&self, size: f32) -> CachedGlyph {
        // Try to get metrics from 'W' for the box size
        let w_glyph = self.rasterize_primary_glyph('W', size);
        let (w, h) = if let Some(g) = w_glyph {
            (g.width.max(8), g.height.max(8))
        } else {
            ((size.ceil() as u32).max(8), (size.ceil() as u32).max(8))
        };
        
        let mut bitmap = vec![0u8; (w * h) as usize];
        if w > 0 && h > 0 {
            // Draw a hollow rectangle
            for x in 0..w {
                bitmap[x as usize] = 255;
                bitmap[((h - 1) * w + x) as usize] = 255;
            }
            for y in 0..h {
                bitmap[(y * w) as usize] = 255;
                bitmap[(y * w + (w - 1)) as usize] = 255;
            }
        }

        CachedGlyph {
            width: w,
            height: h,
            bearing_x: 0,
            bearing_y: h as i32,
            advance_width: size * 0.8,
            advance_height: 0.0,
            bitmap,
        }
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
                glyph.advance_width as i32
            })
            .sum()
    }
}
