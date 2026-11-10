# Image & SVG Processing Reference

Covers: **image**, **resvg**, **usvg**, **tiny-skia**, **cairo-rs**, **fast_image_resize**, **imgref**, **rgb**, **gif**, **linicon**, **base64**, **sixel-rs**, **icy_sixel**

---

## 1. image

**Version:** `0.25.10`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/image |
| GitHub Repository | https://github.com/image-rs/image |

### Architecture in Dock

The `image` crate provides raster image loading and saving. Dock uses it for:
- Loading application icons (PNG, JPEG, ICO)
- Image format detection and conversion
- Screenshot capture and processing

#### Key Concepts

- **DynamicImage**: Format-agnostic image type (RGBA8, RGB8, etc.)
- **ImageFormat**: Detected or specified image format
- **Codec**: Format-specific encoder/decoder

#### Supported Formats

| Format | Load | Save |
|--------|------|------|
| PNG | ✓ | ✓ |
| JPEG | ✓ | ✓ |
| GIF | ✓ | ✓ |
| ICO | ✓ | ✓ |
| WebP | ✓ | ✓ |
| TIFF | ✓ | ✓ |
| BMP | ✓ | ✓ |

---

## 2. resvg & usvg

**Version:** resvg `0.47.0`, usvg `0.48.1`

### Official Documentation

| Resource | URL |
|----------|-----|
| resvg API | https://docs.rs/resvg |
| usvg API | https://docs.rs/usvg |
| GitHub Repository | https://github.com/RazrFalcon/resvg |

### Architecture in Dock

resvg provides SVG-to-pixel conversion. usvg handles SVG parsing and layout. Dock uses them for:
- Rendering SVG application icons to raster images
- Converting SVG paths to pixel data for GPU textures

#### Pipeline

```
SVG file → usvg (parse + layout) → resvg (rasterize) → pixel buffer → GPU texture
```

#### Integration Notes

- usvg produces a tree of SVG elements; resvg renders that tree to pixels.
- The output is typically RGBA8 pixel data ready for GPU upload.
- SVG rendering is CPU-bound; cache rendered results for frequently used icons.

---

## 3. tiny-skia

**Version:** `0.12.0`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/tiny-skia |
| GitHub Repository | https://github.com/nical/lyon/tree/main/tiny-skia |

### Architecture in Dock

tiny-skia provides a 2D Canvas API (subset of Skia). Dock uses it for:
- Raster operations on images
- Drawing primitives (lines, rectangles, paths)
- Image compositing and transformations

#### Integration Notes

- tiny-skia is CPU-based, not GPU-accelerated.
- Use for preprocessing images before GPU upload.
- Integrates with `image` crate for format conversion.

---

## 4. cairo-rs

**Version:** `0.19`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/cairo-rs |
| Cairo Documentation | https://www.cairographics.org/documentation/ |

### Architecture in Dock

cairo-rs provides Rust bindings for the Cairo 2D graphics library. Dock may use it for:
- Advanced 2D drawing operations
- Text rendering with fontconfig integration
- Image surface manipulation

#### Integration Notes

- Cairo is a C library; cairo-rs provides safe Rust bindings.
- Use for operations not covered by tiny-skia or wgpu-api.
- Cairo surfaces can be converted to `image` crate types.

---

## 5. fast_image_resize

**Version:** `5.0`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/fast_image_resize |
| GitHub Repository | https://github.com/aldanor/fast_image_resize |

### Architecture in Dock

fast_image_resize provides SIMD-optimized image resizing. Dock uses it to:
- Resize application icons to dock icon size
- Scale video frames for preview thumbnails
- Replace custom Lanczos-3 implementation

#### Key Features

- SIMD acceleration (AVX2, NEON)
- Multiple resize algorithms (nearest, bilinear, bicubic, lanczos)
- In-place resizing to avoid allocations

---

## 6. Supporting Libraries

### imgref (`1.10`)
Immutable image reference type. Used for passing image data without ownership transfers.

### rgb (`0.8`)
RGB pixel type. Used for pixel-level image manipulation.

### gif (`0.13`)
GIF decoding for animated images. Used in wgpu-api for animated icon support.

### linicon (`2.3`)
Linux icon theme lookup. Used to find application icons according to the XDG icon theme specification.

### base64 (`0.22`)
Base64 encoding for image bytes. Used for terminal output and data transfer.

### sixel-rs (`0.5.0`) & icy_sixel (`0.5.0`)
Sixel image format encoding and terminal output. Used for displaying images in terminal environments.

---

## Dependency Compatibility

| Library | Version | Notes |
|---------|---------|-------|
| image | `0.25.10` | Raster image I/O |
| resvg | `0.47.0` | SVG rasterization |
| usvg | `0.48.1` | SVG parsing |
| tiny-skia | `0.12.0` | 2D Canvas API |
| cairo-rs | `0.19` | Cairo bindings |
| fast_image_resize | `5.0` | SIMD resize |

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| SVG render fails | Invalid SVG syntax | Validate SVG before rendering |
| Icon not found | Theme path incorrect | Check linicon theme configuration |
| Resize artifacts | Wrong algorithm | Use lanczos for quality, bilinear for speed |
| Memory usage high | Too many cached images | Implement LRU cache with size limit |
| GIF not animating | Frame timing wrong | Check GIF frame delay values |

## Testing

- **Image changes**: Visual inspection of rendered icons
- **SVG changes**: Render test SVGs and compare output
- **Resize changes**: Compare quality at different scales
