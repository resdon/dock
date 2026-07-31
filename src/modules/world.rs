use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use fontdue::{Font, FontSettings};
use memmap2::Mmap;

pub struct CachedGlyph {
    pub bitmap: Vec<u8>,
    pub metrics: fontdue::Metrics,
}

pub struct FontManager {
    primary_font: Font,
    _primary_mmap: Option<Mmap>,
    cache_dir: PathBuf,
    cache: HashMap<(char, u32), CachedGlyph>,
    fallback_font_cache: HashMap<PathBuf, Vec<Font>>,
}

impl FontManager {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(&path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let primary_font = Font::from_bytes(&mmap[..], FontSettings::default())
            .map_err(|e| format!("Failed to parse font: {e}"))?;

        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("dock/glyphs");
        let _ = fs::create_dir_all(&cache_dir);

        eprintln!("[FontManager] Initialized with cache dir: {:?}", cache_dir);

        Ok(Self {
            primary_font,
            _primary_mmap: Some(mmap),
            cache_dir,
            cache: HashMap::new(),
            fallback_font_cache: HashMap::new(),
        })
    }

    pub fn new(font_data: &[u8]) -> Self {
        let font = Font::from_bytes(font_data, FontSettings::default()).expect("Invalid font data");
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("dock/glyphs");
        let _ = fs::create_dir_all(&cache_dir);

        eprintln!("[FontManager] Initialized (raw bytes) with cache dir: {:?}", cache_dir);

        Self {
            primary_font: font,
            _primary_mmap: None,
            cache_dir,
            cache: HashMap::new(),
            fallback_font_cache: HashMap::new(),
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
        file.write_all(&(metrics.xmin as i32).to_le_bytes())?;
        file.write_all(&(metrics.ymin as i32).to_le_bytes())?;
        file.write_all(&(metrics.advance_width as f32).to_le_bytes())?;
        file.write_all(&(metrics.advance_height as f32).to_le_bytes())?;
        file.write_all(bitmap)?;

        eprintln!(
            "[GlyphCache] Saved binary cache to {:?} (dims: {}x{}, bitmap len: {})",
            path, metrics.width, metrics.height, bitmap.len()
        );

        Ok(())
    }

    fn load_glyph_bin(&self, path: &Path, size: f32) -> Option<CachedGlyph> {
        let mut file = File::open(path).ok()?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).ok()?;

        if buf.len() < 24 {
            eprintln!("[GlyphCache] File {:?} is too small ({} bytes)", path, buf.len());
            return None;
        }

        let width = u32::from_le_bytes(buf[0..4].try_into().ok()?) as usize;
        let height = u32::from_le_bytes(buf[4..8].try_into().ok()?) as usize;
        let xmin = i32::from_le_bytes(buf[8..12].try_into().ok()?);
        let ymin = i32::from_le_bytes(buf[12..16].try_into().ok()?);
        let advance_width = f32::from_le_bytes(buf[16..20].try_into().ok()?);
        let advance_height = f32::from_le_bytes(buf[20..24].try_into().ok()?);
        let bitmap = buf[24..].to_vec();

        eprintln!(
            "[GlyphCache] Loaded cached bin {:?} (dims: {}x{}, bitmap len: {})",
            path, width, height, bitmap.len()
        );

        let (mut metrics, _) = self.primary_font.rasterize(' ', size);
        metrics.width = width;
        metrics.height = height;
        metrics.xmin = xmin;
        metrics.ymin = ymin;
        metrics.advance_width = advance_width;
        metrics.advance_height = advance_height;

        Some(CachedGlyph { bitmap, metrics })
    }

    fn find_system_font_path_for_char(character: char) -> Option<PathBuf> {
        let lang = Self::char_to_lang(character);
        let pattern = format!(":lang={}", lang);

        let output = Command::new("fc-match")
            .arg(&pattern)
            .arg("-f")
            .arg("%{file}")
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let path_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !path_str.is_empty() {
                    let path = PathBuf::from(&path_str);
                    if path.exists() {
                        eprintln!(
                            "[Fontconfig] fc-match for char '{}' (U+{:04X}, lang={}) -> {:?}",
                            character, character as u32, lang, path
                        );
                        return Some(path);
                    }
                }
            }
            Ok(out) => {
                eprintln!("[Fontconfig] fc-match failed with code: {:?}", out.status.code());
            }
            Err(e) => {
                eprintln!("[Fontconfig] Failed to execute fc-match: {}", e);
            }
        }
        None
    }

    fn char_to_lang(c: char) -> &'static str {
        match c as u32 {
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => "zh",
            0x3040..=0x309F | 0x30A0..=0x30FF => "ja",
            0xAC00..=0xD7AF | 0x1100..=0x11FF => "ko",
            _ => "en",
        }
    }

    fn load_fonts_from_path(path: &Path) -> Vec<Font> {
        let mut fonts = Vec::new();
        if let Ok(file) = File::open(path) {
            if let Ok(mmap) = unsafe { Mmap::map(&file) } {
                for collection_index in 0..8 {
                    let settings = FontSettings {
                        collection_index,
                        ..FontSettings::default()
                    };
                    if let Ok(font) = Font::from_bytes(&mmap[..], settings) {
                        fonts.push(font);
                    } else if collection_index == 0 {
                        break;
                    }
                }
            }
        }
        eprintln!("[FontLoader] Loaded {} font face(s) from {:?}", fonts.len(), path);
        fonts
    }

    fn extract_single_glyph(&mut self, character: char, size: f32) -> Option<CachedGlyph> {
        let bin_path = self.cache_dir.join(format!("{:x}_{}.bin", character as u32, size as u32));

        if bin_path.exists() {
            if let Some(glyph) = self.load_glyph_bin(&bin_path, size) {
                return Some(glyph);
            }
        }

        eprintln!(
            "[GlyphExtract] Resolving missing glyph for char '{}' (U+{:04X}, size={})",
            character, character as u32, size
        );

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

            if !self.fallback_font_cache.contains_key(&path) {
                let parsed_fonts = Self::load_fonts_from_path(&path);
                self.fallback_font_cache.insert(path.clone(), parsed_fonts);
            }

            if let Some(fonts) = self.fallback_font_cache.get(&path) {
                for (idx, font) in fonts.iter().enumerate() {
                    let glyph_idx = font.lookup_glyph_index(character);
                    if glyph_idx > 0 {
                        let (metrics, bitmap) = font.rasterize(character, size);

                        if metrics.width == 0 || metrics.height == 0 || bitmap.is_empty() {
                            continue;
                        }

                        eprintln!(
                            "[GlyphExtract] Found valid glyph '{}' in {:?} (face #{}, dims: {}x{})",
                            character, path, idx, metrics.width, metrics.height
                        );

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
            }
        }

        eprintln!(
            "[GlyphExtract] FAILED: No system fallback font contained character '{}' (U+{:04X})",
            character, character as u32
        );

        None
    }

    pub fn get_glyph(&mut self, character: char, size: f32) -> &CachedGlyph {
        let key = (character, size as u32);

        if !self.cache.contains_key(&key) {
            let primary_glyph_idx = self.primary_font.lookup_glyph_index(character);

            let mut valid_primary = false;
            let mut primary_res = None;

            if primary_glyph_idx > 0 {
                let (metrics, bitmap) = self.primary_font.rasterize(character, size);
                if metrics.width > 0 && metrics.height > 0 && !bitmap.is_empty() {
                    valid_primary = true;
                    primary_res = Some(CachedGlyph { bitmap, metrics });
                }
            }

            let cached_glyph = if valid_primary {
                primary_res.unwrap()
            } else if let Some(glyph) = self.extract_single_glyph(character, size) {
                glyph
            } else {
                eprintln!(
                    "[GetGlyph] Falling back to primary font default for missing char '{}' (U+{:04X})",
                    character, character as u32
                );
                let (metrics, bitmap) = self.primary_font.rasterize(character, size);
                CachedGlyph { bitmap, metrics }
            };

            self.cache.insert(key, cached_glyph);
        }

        self.cache.get(&key).unwrap()
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.cache.shrink_to_fit();
    }
}

pub struct World {
    pub font_manager: FontManager,
    pub width: usize,
    pub height: usize,
}

impl World {
    pub fn from_font_path<P: AsRef<Path>>(
        width: usize,
        height: usize,
        font_path: P,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let font_manager = FontManager::from_file(font_path)?;
        Ok(Self {
            font_manager,
            width,
            height,
        })
    }

    pub fn new(width: usize, height: usize, font_data: &[u8]) -> Self {
        Self {
            font_manager: FontManager::new(font_data),
            width,
            height,
        }
    }

    pub fn draw_blue_box(
        &self,
        frame: &mut [u8],
        x_start: usize,
        x_end: usize,
        y_start: usize,
        y_end: usize,
    ) {
        let blue_pixel: [u8; 4] = [255, 100, 0, 255]; 

        for y in y_start..=y_end {
            for x in x_start..=x_end {
                let is_edge = x == x_start || x == x_end || y == y_start || y == y_end;
                
                if is_edge {
                    let pixel_index = (y * self.width + x) * 4;

                    if pixel_index + 3 < frame.len() {
                        frame[pixel_index..pixel_index + 4].copy_from_slice(&blue_pixel);
                    }
                }
            }
        }
    }

    pub fn draw_text(
        &mut self,
        frame: &mut [u8],
        text: &str,
        start_x: usize,
        baseline_y: usize,
        size: f32,
        color: [u8; 3],
        frame_width: usize,
        frame_height: usize,
    ) -> Result<(), String> {
        let mut cursor_x = start_x;

        eprintln!("[DrawText] Rendering string: \"{}\"", text);

        for c in text.chars() {
            // Skip zero-width spaces and non-printable control characters
            if c == '\u{200b}' || c.is_control() {
                continue;
            }

            let glyph = self.font_manager.get_glyph(c, size);
            let advance = glyph.metrics.advance_width.round() as usize;

            if !glyph.bitmap.is_empty() {
                Self::blit_glyph(
                    frame,
                    frame_width,
                    frame_height,
                    glyph,
                    cursor_x,
                    baseline_y,
                    color,
                );
            }

            cursor_x += advance;
            if cursor_x >= frame_width {
                break;
            }
        }

        Ok(())
    }

    fn blit_glyph(
        frame: &mut [u8],
        width: usize,
        height: usize,
        glyph: &CachedGlyph,
        x: usize,
        y: usize,
        color: [u8; 3],
    ) {
        let g_width = glyph.metrics.width;
        let g_height = glyph.metrics.height;

        for row in 0..g_height {
            for col in 0..g_width {
                let bitmap_idx = row * g_width + col;
                let opacity_u8 = glyph.bitmap[bitmap_idx];

                if opacity_u8 == 0 {
                    continue; 
                }

                let target_x = (x as i32 + glyph.metrics.xmin + col as i32) as isize;
                let target_y = (y as i32 - glyph.metrics.ymin - g_height as i32 + row as i32) as isize;

                if target_x < 0
                    || target_x >= width as isize
                    || target_y < 0
                    || target_y >= height as isize
                {
                    continue;
                }

                let pixel_index = (target_y as usize * width + target_x as usize) * 4;

                if pixel_index + 3 >= frame.len() {
                    continue;
                }

                let alpha = opacity_u8 as f32 / 255.0;
                
                for i in 0..3 {
                    let dst = frame[pixel_index + i] as f32;
                    let src = color[i] as f32;
                    let blended = (src * alpha) + (dst * (1.0 - alpha));
                    frame[pixel_index + i] = blended as u8;
                }
            }
        }
    }

    pub fn clear_font_cache(&mut self) {
        self.font_manager.clear_cache();
    }
}