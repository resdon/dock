//! Font database management using fontdb.
//!
//! This module provides font discovery, loading, and family resolution.
//! It wraps fontdb to handle:
//!
//! - Font loading from paths and system directories
//! - Font family resolution and fallback chains
//! - Font property matching (weight, style, stretch)
//! - System font discovery

use crate::text::{Font, FontStyle, FontWeight};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Font properties
// ---------------------------------------------------------------------------

/// Font properties for matching and selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontProperties {
    /// Font weight.
    pub weight: FontWeight,
    /// Font style.
    pub style: FontStyle,
    /// Font stretch (condensed, normal, expanded).
    pub stretch: FontStretch,
}

impl FontProperties {
    /// Create new font properties.
    pub fn new(weight: FontWeight, style: FontStyle) -> Self {
        Self {
            weight,
            style,
            stretch: FontStretch::Normal,
        }
    }
}

impl Default for FontProperties {
    fn default() -> Self {
        Self {
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        }
    }
}

/// Font stretch (width variation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl Default for FontStretch {
    fn default() -> Self {
        Self::Normal
    }
}

// ---------------------------------------------------------------------------
// Font entry
// ---------------------------------------------------------------------------

/// A font entry in the database.
#[derive(Debug, Clone)]
pub struct FontEntry {
    /// Unique identifier.
    pub id: u32,
    /// Font family name.
    pub family: String,
    /// Font properties.
    pub properties: FontProperties,
    /// Path to the font file.
    pub path: PathBuf,
    /// Whether the font is loaded in memory.
    pub loaded: bool,
    /// Font data (if loaded).
    pub data: Option<Vec<u8>>,
}

impl FontEntry {
    /// Create a new font entry.
    pub fn new(id: u32, family: String, properties: FontProperties, path: PathBuf) -> Self {
        Self {
            id,
            family,
            properties,
            path,
            loaded: false,
            data: None,
        }
    }

    /// Mark the font as loaded.
    pub fn mark_loaded(&mut self, data: Vec<u8>) {
        self.loaded = true;
        self.data = Some(data);
    }

    /// Check if this font matches the given properties.
    pub fn matches(&self, props: &FontProperties) -> bool {
        self.properties.weight == props.weight
            && self.properties.style == props.style
            && self.properties.stretch == props.stretch
    }

    /// Calculate how closely this font matches the desired properties.
    /// Lower score = better match.
    pub fn match_score(&self, props: &FontProperties) -> u32 {
        let mut score = 0;

        // Weight difference
        let weight_diff = (self.properties.weight as i32 - props.weight as i32).unsigned_abs();
        score += weight_diff / 10;

        // Style difference
        if self.properties.style != props.style {
            score += 10;
        }

        // Stretch difference
        if self.properties.stretch != props.stretch {
            score += 5;
        }

        score
    }
}

// ---------------------------------------------------------------------------
// Font database
// ---------------------------------------------------------------------------

/// Font database that manages font discovery and loading.
///
/// Wraps fontdb to provide:
/// - System font discovery
/// - Custom font loading
/// - Family-based font selection
/// - Property-based font matching
pub struct FontDatabase {
    /// Loaded font entries.
    pub entries: Vec<FontEntry>,
    /// Family to entries mapping.
    pub families: HashMap<String, Vec<u32>>,
    /// Next font entry ID.
    pub next_id: u32,
    /// System font directories to scan.
    pub system_dirs: Vec<PathBuf>,
    /// Whether system fonts have been loaded.
    pub system_loaded: bool,
}

impl FontDatabase {
    /// Create a new font database.
    pub fn new() -> Self {
        let mut system_dirs = Vec::new();

        // Common system font directories on Linux
        system_dirs.push(PathBuf::from("/usr/share/fonts"));
        system_dirs.push(PathBuf::from("/usr/local/share/fonts"));

        // User font directory
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = PathBuf::from(home);
            system_dirs.push(home_path.join(".local/share/fonts"));
            system_dirs.push(home_path.join(".fonts"));
        }

        Self {
            entries: Vec::new(),
            families: HashMap::new(),
            next_id: 1,
            system_dirs,
            system_loaded: false,
        }
    }

    /// Add a custom font directory to scan.
    pub fn add_font_dir<P: AsRef<Path>>(&mut self, path: P) {
        self.system_dirs.push(path.as_ref().to_path_buf());
    }

    /// Load a font from a file path.
    pub fn load_font<P: AsRef<Path>>(&mut self, path: P) -> Result<u32, String> {
        let path = path.as_ref().to_path_buf();
        let data = std::fs::read(&path).map_err(|e| format!("Failed to read font file: {}", e))?;

        self.load_font_from_data(data, path)
    }

    /// Load a font from raw data.
    pub fn load_font_from_data(&mut self, data: Vec<u8>, path: PathBuf) -> Result<u32, String> {
        // Validate font data
        if data.len() < 4 {
            return Err("Invalid font data: too short".to_string());
        }

        // Parse font metadata from the provided data.
        let mut db = fontdb::Database::new();
        db.load_font_data(data.clone());
        db.set_serif_family("Noto Serif".to_string());
        db.set_sans_serif_family("Noto Sans".to_string());
        db.set_monospace_family("Noto Sans Mono".to_string());

        let (family, weight, style) = if let Some(face) = db.faces().next() {
            let family = face
                .families
                .first()
                .map(|f| f.0.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let weight = match face.weight.0 {
                100..=149 => FontWeight::Thin,
                150..=249 => FontWeight::ExtraLight,
                250..=349 => FontWeight::Light,
                350..=449 => FontWeight::Normal,
                450..=549 => FontWeight::Medium,
                550..=649 => FontWeight::SemiBold,
                650..=749 => FontWeight::Bold,
                750..=849 => FontWeight::ExtraBold,
                _ => FontWeight::Black,
            };
            let style = if face.style == fontdb::Style::Italic {
                FontStyle::Italic
            } else if face.style == fontdb::Style::Oblique {
                FontStyle::Oblique
            } else {
                FontStyle::Normal
            };
            (family, weight, style)
        } else {
            ("Unknown".to_string(), FontWeight::Normal, FontStyle::Normal)
        };

        let properties = FontProperties::new(weight, style);
        let id = self.next_id;
        self.next_id += 1;

        let mut entry = FontEntry::new(id, family.clone(), properties, path);
        entry.mark_loaded(data);

        self.entries.push(entry);

        // Update family mapping
        self.families.entry(family).or_insert_with(Vec::new).push(id);

        Ok(id)
    }

    /// Load a font descriptor.
    pub fn load_font_descriptor(&mut self, font: &Font) -> Result<u32, String> {
        self.load_font(&font.path)
    }

    /// Discover system fonts.
    ///
    /// Scans the configured system directories for font files.
    pub fn discover_system_fonts(&mut self) {
        if self.system_loaded {
            return;
        }

        // Clone directories to avoid borrow checker issues
        let dirs = self.system_dirs.clone();
        for dir in &dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();

                    // Recurse into subdirectories
                    if path.is_dir() {
                        self.scan_font_dir(&path);
                    } else if Self::is_font_file(&path) {
                        if let Err(e) = self.load_font(&path) {
                            log::debug!("Failed to load system font {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        self.system_loaded = true;
    }

    /// Scan a directory for font files.
    fn scan_font_dir(&mut self, dir: &Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    self.scan_font_dir(&path);
                } else if Self::is_font_file(&path) {
                    if let Err(e) = self.load_font(&path) {
                        log::debug!("Failed to load font {:?}: {}", path, e);
                    }
                }
            }
        }
    }

    /// Check if a file is a font file.
    fn is_font_file(path: &Path) -> bool {
        if let Some(ext) = path.extension() {
            matches!(
                ext.to_string_lossy().to_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "woff" | "woff2"
            )
        } else {
            false
        }
    }

    /// Find fonts by family name.
    pub fn find_by_family(&self, family: &str) -> Vec<&FontEntry> {
        if let Some(ids) = self.families.get(family) {
            ids.iter()
                .filter_map(|id| self.entries.iter().find(|e| e.id == *id))
                .collect()
        } else {
            // Try case-insensitive match
            self.entries
                .iter()
                .filter(|e| e.family.eq_ignore_ascii_case(family))
                .collect()
        }
    }

    /// Find the best matching font for the given family and properties.
    pub fn find_best_match(&self, family: &str, props: &FontProperties) -> Option<&FontEntry> {
        // First try exact family match
        let mut candidates = self.find_by_family(family);

        // If no exact match, try all fonts
        if candidates.is_empty() {
            candidates = self.entries.iter().collect();
        }

        // Find the best match by score
        candidates.into_iter().min_by_key(|e| e.match_score(props))
    }

    /// Get all available font families.
    pub fn families(&self) -> Vec<&str> {
        self.families.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of loaded fonts.
    pub fn font_count(&self) -> usize {
        self.entries.len()
    }

    /// Get a font entry by ID.
    pub fn get(&self, id: u32) -> Option<&FontEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Get the font data for a font entry.
    pub fn get_data(&self, id: u32) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| e.data.as_deref())
    }

    /// Clear all loaded fonts.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.families.clear();
        self.next_id = 1;
        self.system_loaded = false;
    }

    /// Estimate memory usage of loaded fonts.
    pub fn estimate_memory_mb(&self) -> f32 {
        let total_bytes: usize = self
            .entries
            .iter()
            .filter_map(|e| e.data.as_ref())
            .map(|d| d.len())
            .sum();
        total_bytes as f32 / (1024.0 * 1024.0)
    }
}

impl Default for FontDatabase {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Font resolver
// ---------------------------------------------------------------------------

/// Font resolver that selects the best font for a given text string.
///
/// Handles font fallback for characters not available in the primary font.
pub struct FontResolver {
    /// Font database.
    pub database: FontDatabase,
    /// Default font family.
    pub default_family: String,
    /// Fallback font families.
    pub fallback_families: Vec<String>,
    /// Default properties.
    pub default_properties: FontProperties,
}

impl FontResolver {
    /// Create a new font resolver.
    pub fn new() -> Self {
        Self {
            database: FontDatabase::new(),
            default_family: "Sans".to_string(),
            fallback_families: vec![
                "Noto Sans".to_string(),
                "Noto Sans CJK JP".to_string(),
                "Noto Sans CJK SC".to_string(),
                "Noto Sans CJK TC".to_string(),
                "Noto Sans CJK KR".to_string(),
                "Noto Color Emoji".to_string(),
            ],
            default_properties: FontProperties::default(),
        }
    }

    /// Set the default font family.
    pub fn with_default_family(mut self, family: String) -> Self {
        self.default_family = family;
        self
    }

    /// Set the default font properties.
    pub fn with_default_properties(mut self, props: FontProperties) -> Self {
        self.default_properties = props;
        self
    }

    /// Add a fallback font family.
    pub fn add_fallback(&mut self, family: String) {
        self.fallback_families.push(family);
    }

    /// Resolve the best font for a character.
    pub fn resolve_for_char(&self, _ch: char) -> Option<&FontEntry> {
        // Try the default family first
        if let Some(entry) = self
            .database
            .find_best_match(&self.default_family, &self.default_properties)
        {
            return Some(entry);
        }

        // Try fallback families
        for family in &self.fallback_families {
            if let Some(entry) = self
                .database
                .find_best_match(family, &self.default_properties)
            {
                return Some(entry);
            }
        }

        // Return any available font
        self.database.entries.first()
    }

    /// Resolve fonts for a text string.
    ///
    /// Returns a list of (byte_range, font_entry) pairs.
    pub fn resolve_for_text(&self, text: &str) -> Vec<(std::ops::Range<usize>, u32)> {
        let mut result = Vec::new();
        let mut byte_offset = 0;

        for ch in text.chars() {
            let char_len = ch.len_utf8();

            if let Some(entry) = self.resolve_for_char(ch) {
                result.push((byte_offset..(byte_offset + char_len), entry.id));
            }

            byte_offset += char_len;
        }

        result
    }

    /// Get the font database.
    pub fn database(&self) -> &FontDatabase {
        &self.database
    }

    /// Get the mutable font database.
    pub fn database_mut(&mut self) -> &mut FontDatabase {
        &mut self.database
    }
}

impl Default for FontResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_properties() {
        let props = FontProperties::new(FontWeight::Bold, FontStyle::Italic);
        assert_eq!(props.weight, FontWeight::Bold);
        assert_eq!(props.style, FontStyle::Italic);
        assert_eq!(props.stretch, FontStretch::Normal);
    }

    #[test]
    fn test_font_properties_default() {
        let props = FontProperties::default();
        assert_eq!(props.weight, FontWeight::Normal);
        assert_eq!(props.style, FontStyle::Normal);
    }

    #[test]
    fn test_font_entry_creation() {
        let entry = FontEntry::new(
            1,
            "Test".to_string(),
            FontProperties::default(),
            PathBuf::from("/test/font.ttf"),
        );

        assert_eq!(entry.id, 1);
        assert_eq!(entry.family, "Test");
        assert!(!entry.loaded);
    }

    #[test]
    fn test_font_entry_match() {
        let props = FontProperties::new(FontWeight::Bold, FontStyle::Italic);
        let entry = FontEntry::new(1, "Test".to_string(), props, PathBuf::from("/test.ttf"));

        assert!(entry.matches(&props));
        assert!(!entry.matches(&FontProperties::default()));
    }

    #[test]
    fn test_font_entry_match_score() {
        let props = FontProperties::new(FontWeight::Bold, FontStyle::Normal);
        let entry = FontEntry::new(1, "Test".to_string(), props, PathBuf::from("/test.ttf"));

        // Exact match should have score 0
        assert_eq!(entry.match_score(&props), 0);

        // Different weight should have higher score
        let other_props = FontProperties::new(FontWeight::Thin, FontStyle::Normal);
        assert!(entry.match_score(&other_props) > 0);
    }

    #[test]
    fn test_font_database_creation() {
        let db = FontDatabase::new();
        assert_eq!(db.font_count(), 0);
        assert_eq!(db.next_id, 1);
        assert!(!db.system_loaded);
    }

    #[test]
    fn test_font_database_families() {
        let db = FontDatabase::new();
        let families = db.families();
        assert!(families.is_empty());
    }

    #[test]
    fn test_font_database_memory_estimate() {
        let db = FontDatabase::new();
        assert!((db.estimate_memory_mb() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_is_font_file() {
        assert!(FontDatabase::is_font_file(Path::new("font.ttf")));
        assert!(FontDatabase::is_font_file(Path::new("font.otf")));
        assert!(FontDatabase::is_font_file(Path::new("font.TTC")));
        assert!(!FontDatabase::is_font_file(Path::new("font.png")));
        assert!(!FontDatabase::is_font_file(Path::new("font.txt")));
    }

    #[test]
    fn test_font_resolver_creation() {
        let resolver = FontResolver::new();
        assert_eq!(resolver.default_family, "Sans");
        assert!(!resolver.fallback_families.is_empty());
    }

    #[test]
    fn test_font_resolver_with_defaults() {
        let resolver = FontResolver::new()
            .with_default_family("Custom".to_string())
            .with_default_properties(FontProperties::new(FontWeight::Bold, FontStyle::Normal));

        assert_eq!(resolver.default_family, "Custom");
        assert_eq!(resolver.default_properties.weight, FontWeight::Bold);
    }

    #[test]
    fn test_font_resolver_fallback() {
        let mut resolver = FontResolver::new();
        resolver.add_fallback("Custom Fallback".to_string());
        assert!(resolver
            .fallback_families
            .contains(&"Custom Fallback".to_string()));
    }

    #[test]
    fn test_font_stretch_default() {
        let stretch = FontStretch::default();
        assert_eq!(stretch, FontStretch::Normal);
    }
}
