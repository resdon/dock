# Text & Fonts Reference

Covers: **cosmic-text**, **fontdb**, **swash**, **fontdue**

---

## 1. cosmic-text

**Version:** `0.18`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/cosmic-text |
| GitHub Repository | https://github.com/pop-os/cosmic-text |
| Smithay Ecosystem | https://github.com/Smithay |

### Architecture in Dock

cosmic-text provides text shaping and layout. Dock uses it for:
- Text rendering in dock labels and tooltips
- Multi-line text layout
- Font feature handling (ligatures, kerning)
- International text support (Unicode, bidirectional text)

#### Key Concepts

- **FontContext**: Shaping context for a single font.
- **SwapBuffer**: Double-buffered text buffer for efficient updates.
- **Shaping**: Converting text strings to glyph positions and advances.
- **Layout**: Determining line breaks and text flow.
- **Rasterizer**: Converting glyph outlines to pixel data (cosmic-text uses swash).

#### Pipeline

```
Text string → cosmic-text (shape + layout) → swash (rasterize glyphs) → pixel buffer → GPU texture
```

#### Integration Notes

- cosmic-text is CPU-based. Cache shaped text for frequently displayed strings.
- Font discovery uses `fontdb` internally.
- Supports OpenType features (kerning, ligatures, small caps).
- Integrates with the Smithay ecosystem for Wayland applications.

---

## 2. fontdb

**Version:** `0.16`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/fontdb |
| GitHub Repository | https://github.com/RazrFalcon/fontdb |

### Architecture in Dock

fontdb provides font discovery and database management. Dock uses it for:
- Discovering system fonts
- Loading fonts by family name
- Font face selection and matching

#### Key Concepts

- **Database**: Collection of available fonts.
- **Query**: Font selection criteria (family, weight, style).
- **FaceInfo**: Information about a matched font (file path, index).

#### Integration Notes

- fontdb scans standard font directories (`/usr/share/fonts`, `~/.local/share/fonts`).
- Use `query()` to find fonts by family name and style.
- Font data can be loaded from files or memory buffers.

---

## 3. swash

**Version:** `0.1.18`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/swash |
| GitHub Repository | https://github.com/linebender/swash |

### Architecture in Dock

swash provides font outline extraction and glyph rasterization. Dock uses it for:
- Extracting glyph outlines from OpenType/TrueType fonts
- Rasterizing glyphs to pixel data
- Subpixel antialiasing for crisp text rendering

#### Key Concepts

- **Scale**: Glyph scaler that produces outlines or rasterized images.
- **Outline**: Vector representation of a glyph.
- **Raster**: Pixel representation of a glyph.
- **Transform**: Glyph transformation (scale, rotation, position).

#### Integration Notes

- swash is part of the Linebender ecosystem.
- Supports subpixel antialiasing for better text quality on LCD displays.
- Can produce both outlines (for vector rendering) and rasters (for bitmap rendering).

---

## 4. fontdue

**Version:** `0.9`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/fontdue |
| GitHub Repository | https://github.com/typst/fontdue |

### Architecture in Dock

fontdue provides simple font glyph rasterization. Dock may use it as a fallback for:
- Simple text rendering without full shaping
- Monospace text (code, system indicators)
- Lightweight glyph extraction

#### Integration Notes

- fontdue is simpler than cosmic-text + swash but lacks advanced shaping.
- Use for simple text where full Unicode shaping is not needed.
- Good for monospace fonts and system UI text.

---

## 5. Unicode and Text Standards

These are essential references for correct international text, glyph shaping, font fallback, emoji, and right-to-left text rendering.

### Unicode Standard

| Resource | URL |
|----------|-----|
| Unicode Home Page | https://home.unicode.org |
| Unicode Standard Documentation | https://home.unicode.org/standard/overview |
| Unicode Character Database | https://home.unicode.org/unicode-character-database |
| Unicode Technical Reports | https://www.unicode.org/reports |
| Unicode Emoji | https://unicode.org/emoji |

**Relevance to Dock:** cosmic-text and swash rely on the Unicode standard for character encoding, script detection, and glyph indexing. Understanding Unicode is essential for:
- Correct character-to-glyph mapping across scripts
- Emoji rendering and variation selectors
- Font fallback chains for missing glyphs
- Normalization (NFC, NFD) for text comparison

### Unicode Bidirectional Algorithm (UAX #9)

| Resource | URL |
|----------|-----|
| UAX #9: Bidirectional Algorithm | https://www.unicode.org/reports/tr9 |
| Unicode Bidi Algorithm Overview | https://www.unicode.org/reports/tr9/#BD |
| Bidi Character Embedding | https://www.unicode.org/reports/tr9/#Character.Types |

**Relevance to Dock:** Required for correct rendering of mixed-direction text (e.g., English with Arabic or Hebrew). cosmic-text implements the bidirectional algorithm for layout. Key concepts:
- Base direction detection (left-to-right vs right-to-left)
- Character-level directionality (strong, weak, neutral)
- Embedding and overriding levels
- Visual vs logical ordering

### OpenType Specification

| Resource | URL |
|----------|-----|
| OpenType Specification | https://learn.microsoft.com/en-us/typography/opentype/spec/ |
| OpenType Font Files | https://learn.microsoft.com/en-us/typography/opentype/spec/otff |
| OpenType Layout | https://learn.microsoft.com/en-us/typography/opentype/spec/chapter2 |
| OpenType Feature Tags | https://learn.microsoft.com/en-us/typography/opentype/spec/featuretags |
| W3C CSS Fonts Module (OpenType features) | https://www.w3.org/TR/css-fonts-4 |

**Relevance to Dock:** OpenType features control glyph substitution and positioning. cosmic-text and swash support OpenType features for:
- **Kerning** (`kern`): Adjust spacing between specific glyph pairs
- **Ligatures** (`liga`, `dlig`): Combine characters into single glyphs
- **Small Caps** (`smcp`): Uppercase-like forms at lowercase size
- **Numerics** (`lnum`, `onum`): Lining vs oldstyle figures
- **Alternates** (`salt`, `ss01`–`ss20`): Stylistic alternates
- **Glyph positioning** (`mark`, `mkmk`): Mark-to-base and mark-to-mark

---

## Dependency Compatibility

| Library | Version | Notes |
|---------|---------|-------|
| cosmic-text | `0.18` | Text shaping and layout |
| fontdb | `0.16` | Font discovery |
| swash | `0.1.18` | Glyph rasterization |
| fontdue | `0.9` | Simple glyph rasterization |

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Text not rendering | Font not found | Check fontdb database and font paths |
| Text blurry | DPI mismatch | Set correct device pixel ratio |
| Shaping slow | Long text strings | Cache shaped text results |
| Missing glyphs | Font doesn't cover script | Add fallback fonts for the script |
| Kerning wrong | Font feature not enabled | Enable kerning in cosmic-text |

## Testing

- **Text changes**: Visual inspection of rendered text
- **Font changes**: Verify font discovery and loading
- **Shaping changes**: Test with various Unicode scripts
