//! Shader source code for the 2D rendering pipeline.
//!
//! All shaders are written in WGSL (WebGPU Shading Language) and compiled
//! at runtime by wgpu. The shaders support textured quads with per-vertex
//! colors and a 2D transformation matrix.

/// Vertex shader for 2D textured rendering.
///
/// Each vertex has:
/// - Position (x, y) in clip space [-1, +1]
/// - Texture coordinates (u, v)
/// - Per-vertex color (r, g, b, a)
///
/// The uniform buffer provides a 3x3 transformation matrix (packed as 3 vec2s)
/// and the surface size for coordinate conversion.
pub const VERTEX_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct Uniforms {
    transform: mat2x2<f32>,
    offset: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pos = uniforms.transform * input.position + uniforms.offset;
    output.clip_position = vec4<f32>(pos, 0.0, 1.0);
    output.tex_coords = input.tex_coords;
    output.color = input.color;
    return output;
}
"#;

/// Fragment shader for textured rendering with alpha blending.
///
/// Samples the texture and multiplies by the per-vertex color.
/// Supports alpha blending for transparency.
pub const FRAGMENT_SHADER: &str = r#"
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var texture_view: texture_2d<f32>;

struct FragmentInput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@fragment
fn main(input: FragmentInput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(texture_view, texture_sampler, input.tex_coords);
    return tex_color * input.color;
}
"#;

/// Vertex shader for solid-color (untextured) rendering.
///
/// Used when rendering shapes without textures, such as filled paths
/// or solid rectangles.
pub const SOLID_VERTEX_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

struct Uniforms {
    transform: mat2x2<f32>,
    offset: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pos = uniforms.transform * input.position + uniforms.offset;
    output.clip_position = vec4<f32>(pos, 0.0, 1.0);
    output.color = input.color;
    return output;
}
"#;

/// Fragment shader for solid-color rendering.
///
/// Simply passes through the interpolated vertex color.
pub const SOLID_FRAGMENT_SHADER: &str = r#"
struct FragmentInput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@fragment
fn main(input: FragmentInput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

/// Vertex shader for post-processing effects (full-screen quad).
///
/// Generates a full-screen quad in clip space and outputs texture
/// coordinates for the fragment shader.
pub const POST_VERTEX_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Generate a full-screen quad from vertex index
    let x = i32(vertex_index & 1u) * 2 - 1;
    let y = i32((vertex_index >> 1) & 1u) * 2 - 1;
    var output: VertexOutput;
    output.clip_position = vec4<f32>(f32(x), f32(y), 0.0, 1.0);
    output.tex_coords = vec2<f32>(f32(x) * 0.5 + 0.5, f32(y) * 0.5 + 0.5);
    return output;
}
"#;

/// Fragment shader for blur post-processing effect.
///
/// Applies a Gaussian-like blur by sampling neighboring pixels.
pub const BLUR_FRAGMENT_SHADER: &str = r#"
struct Uniforms {
    radius: f32,
    _padding: vec3<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var texture_view: texture_2d<f32>;

struct FragmentInput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@fragment
fn main(input: FragmentInput) -> @location(0) vec4<f32> {
    let texel_size = vec2<f32>(1.0) / vec2<f32>(textureDimensions(texture_view));
    let radius = uniforms.radius;
    var color = vec4<f32>(0.0);
    var weight = 0.0;

    for (var y = -i32(radius); y <= i32(radius); y = y + 1) {
        for (var x = -i32(radius); x <= i32(radius); x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * texel_size;
            let dist = length(vec2<f32>(f32(x), f32(y)));
            let w = exp(-dist * dist / (2.0 * radius * radius));
            color += textureSample(texture_view, texture_sampler, input.tex_coords + offset) * w;
            weight += w;
        }
    }
    return color / weight;
}
"#;
