//! Draw command batching and sorting.
//!
//! This module takes the raw draw commands produced by the rendering pipeline
//! and organizes them into efficient batches for GPU rendering.
//!
//! ## Batching Strategy
//!
//! Sort compatible draw commands by:
//! 1. Render target
//! 2. Pipeline type (Solid2D, TexturedQuad, Text, Composite)
//! 3. Texture or texture atlas
//! 4. Blend mode
//!
//! Preserve the required visual stacking order (z-index) within each batch.
//!
//! ## Goal
//!
//! Minimize draw calls and state changes without sacrificing correctness.
//! For a typical dock UI with ~15 icons and text labels, the entire interface
//! should render in just a handful of draw calls.

use std::collections::HashMap;

use crate::geometry::Rect;
use crate::gpu::{BlendMode, PipelineType, QuadInstance};
use crate::render::DrawCommand;
use crate::texture::TextureId;

// ---------------------------------------------------------------------------
// Batch key
// ---------------------------------------------------------------------------

/// Key for grouping draw commands into batches.
///
/// Commands with the same key can be rendered in a single draw call
/// using instanced rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BatchKey {
    /// Pipeline type.
    pub pipeline_type: PipelineType,
    /// Texture ID (None for solid-color rendering).
    pub texture_id: Option<TextureId>,
    /// Blend mode.
    pub blend_mode: BlendMode,
    /// Clip region index (0 = no clip).
    pub clip_index: u32,
}

impl BatchKey {
    /// Create a new batch key.
    pub fn new(
        pipeline_type: PipelineType,
        texture_id: Option<TextureId>,
        blend_mode: BlendMode,
        clip_index: u32,
    ) -> Self {
        Self {
            pipeline_type,
            texture_id,
            blend_mode,
            clip_index,
        }
    }

    /// Create a solid-color batch key.
    pub fn solid(blend_mode: BlendMode, clip_index: u32) -> Self {
        Self::new(PipelineType::Solid2D, None, blend_mode, clip_index)
    }

    /// Create a textured batch key.
    pub fn textured(texture_id: TextureId, blend_mode: BlendMode, clip_index: u32) -> Self {
        Self::new(
            PipelineType::TexturedQuad,
            Some(texture_id),
            blend_mode,
            clip_index,
        )
    }
}

// ---------------------------------------------------------------------------
// Batched draw command
// ---------------------------------------------------------------------------

/// A draw command that has been assigned to a batch.
#[derive(Debug, Clone)]
pub struct BatchedCommand {
    /// Batch key for grouping.
    pub key: BatchKey,
    /// Z-order within the batch (preserves visual stacking).
    pub z_order: u32,
    /// Quad instance data.
    pub instance: QuadInstance,
    /// Original draw command (for debugging).
    #[allow(dead_code)]
    pub original: DrawCommandType,
}

/// Type of the original draw command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawCommandType {
    Rectangle,
    Image,
    Text,
    Video,
}

// ---------------------------------------------------------------------------
// Batch
// ---------------------------------------------------------------------------

/// A batch of draw commands that can be rendered together.
#[derive(Debug, Clone)]
pub struct Batch {
    /// Batch key.
    pub key: BatchKey,
    /// Commands in this batch, sorted by z-order.
    pub commands: Vec<BatchedCommand>,
    /// Bounding rectangle of all commands in this batch.
    pub bounding_rect: Rect,
}

impl Batch {
    /// Create a new empty batch.
    pub fn new(key: BatchKey) -> Self {
        Self {
            key,
            commands: Vec::with_capacity(64),
            bounding_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Add a command to the batch.
    pub fn add(&mut self, command: BatchedCommand) {
        // Update bounding rect
        let inst_rect = command.instance.bounding_rect();
        if self.commands.is_empty() {
            self.bounding_rect = inst_rect;
        } else {
            self.bounding_rect = self.bounding_rect.union(&inst_rect);
        }
        self.commands.push(command);
    }

    /// Sort commands by z-order.
    pub fn sort_by_z_order(&mut self) {
        self.commands.sort_by_key(|c| c.z_order);
    }

    /// Get the number of commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Get the instance data for all commands.
    pub fn instances(&self) -> Vec<QuadInstance> {
        self.commands.iter().map(|c| c.instance).collect()
    }

    /// Collect instance data into a pre-allocated buffer.
    pub fn collect_instances(&self, buffer: &mut Vec<QuadInstance>) {
        for cmd in &self.commands {
            buffer.push(cmd.instance);
        }
    }
}

// ---------------------------------------------------------------------------
// Batch sorter
// ---------------------------------------------------------------------------

/// Sorts draw commands into batches for efficient rendering.
///
/// Groups commands by pipeline type, texture, and blend mode to minimize
/// state changes and draw calls.
#[derive(Debug, Clone)]
pub struct BatchSorter {
    /// Collected batches by key.
    batches: HashMap<BatchKey, Batch>,
    /// Current z-order counter.
    z_order: u32,
    /// Current blend mode.
    blend_mode: BlendMode,
    /// Current clip index.
    clip_index: u32,
}

impl BatchSorter {
    /// Create a new batch sorter.
    pub fn new() -> Self {
        Self {
            batches: HashMap::new(),
            z_order: 0,
            blend_mode: BlendMode::Alpha,
            clip_index: 0,
        }
    }

    /// Add a draw command to the sorter.
    pub fn add_command(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::Rectangle {
                rect,
                color,
                stroke,
                stroke_width,
                ..
            } => {
                // Main fill
                let key = BatchKey::solid(self.blend_mode, self.clip_index);
                let instance = QuadInstance::solid(
                    [rect.x, rect.y],
                    [rect.width, rect.height],
                    [color.r, color.g, color.b, color.a],
                );
                self.add_batched(
                    key,
                    BatchedCommand {
                        key,
                        z_order: self.z_order,
                        instance,
                        original: DrawCommandType::Rectangle,
                    },
                );
                self.z_order += 1;

                // Stroke as a separate command if present
                if let Some(stroke_color) = stroke {
                    if *stroke_width > 0.0 {
                        // Stroke is rendered as a slightly larger rectangle
                        // with the fill color masked out (simplified here)
                        let stroke_rect = Rect::new(
                            rect.x - stroke_width / 2.0,
                            rect.y - stroke_width / 2.0,
                            rect.width + stroke_width,
                            rect.height + stroke_width,
                        );
                        let key = BatchKey::solid(self.blend_mode, self.clip_index);
                        let instance = QuadInstance::solid(
                            [stroke_rect.x, stroke_rect.y],
                            [stroke_rect.width, stroke_rect.height],
                            [
                                stroke_color.r,
                                stroke_color.g,
                                stroke_color.b,
                                stroke_color.a,
                            ],
                        );
                        self.add_batched(
                            key,
                            BatchedCommand {
                                key,
                                z_order: self.z_order,
                                instance,
                                original: DrawCommandType::Rectangle,
                            },
                        );
                        self.z_order += 1;
                    }
                }
            }
            DrawCommand::Image {
                dest_rect,
                texture,
                source_uv,
                tint,
            } => {
                let key = BatchKey::textured(*texture, self.blend_mode, self.clip_index);
                let instance = QuadInstance::textured(
                    [dest_rect.x, dest_rect.y],
                    [dest_rect.width, dest_rect.height],
                    [source_uv.x, source_uv.y],
                    [
                        source_uv.x + source_uv.width,
                        source_uv.y + source_uv.height,
                    ],
                    [tint.r, tint.g, tint.b, tint.a],
                );
                self.add_batched(
                    key,
                    BatchedCommand {
                        key,
                        z_order: self.z_order,
                        instance,
                        original: DrawCommandType::Image,
                    },
                );
                self.z_order += 1;
            }
            DrawCommand::Text {
                position,
                font_size,
                color,
                ..
            } => {
                // Text is rendered as a placeholder quad for now.
                // In the full implementation, text shaping produces multiple
                // glyph quads that are batched by atlas texture.
                let key = BatchKey::new(
                    PipelineType::Text,
                    None, // Glyph atlas texture
                    self.blend_mode,
                    self.clip_index,
                );
                let instance = QuadInstance::solid(
                    [position.x, position.y],
                    [*font_size, *font_size],
                    [color.r, color.g, color.b, color.a],
                );
                self.add_batched(
                    key,
                    BatchedCommand {
                        key,
                        z_order: self.z_order,
                        instance,
                        original: DrawCommandType::Text,
                    },
                );
                self.z_order += 1;
            }
            DrawCommand::Video {
                dest_rect,
                texture,
                source_uv,
                tint,
            } => {
                let key = BatchKey::textured(*texture, self.blend_mode, self.clip_index);
                let instance = QuadInstance::textured(
                    [dest_rect.x, dest_rect.y],
                    [dest_rect.width, dest_rect.height],
                    [source_uv.x, source_uv.y],
                    [
                        source_uv.x + source_uv.width,
                        source_uv.y + source_uv.height,
                    ],
                    [tint.r, tint.g, tint.b, tint.a],
                );
                self.add_batched(
                    key,
                    BatchedCommand {
                        key,
                        z_order: self.z_order,
                        instance,
                        original: DrawCommandType::Video,
                    },
                );
                self.z_order += 1;
            }
            DrawCommand::PushClip { clip_rect: _ } => {
                self.clip_index += 1;
            }
            DrawCommand::PopClip => {
                if self.clip_index > 0 {
                    self.clip_index -= 1;
                }
            }
            DrawCommand::SetOpacity { opacity } => {
                // Opacity is applied to the color of subsequent commands
                // rather than changing the blend mode
                if *opacity < 1.0 {
                    self.blend_mode = BlendMode::Alpha;
                } else {
                    self.blend_mode = BlendMode::Opaque;
                }
            }
            DrawCommand::SetTransform { .. } => {
                // Transform is resolved on the CPU before batching
                // so it doesn't affect batch keys
            }
        }
    }

    /// Add a batched command to the appropriate batch.
    fn add_batched(&mut self, key: BatchKey, command: BatchedCommand) {
        let batch = self.batches.entry(key).or_insert_with(|| Batch::new(key));
        batch.add(command);
    }

    /// Finalize sorting and return the batches.
    ///
    /// Returns batches sorted by pipeline type for optimal rendering order.
    pub fn finish(mut self) -> Vec<Batch> {
        // Sort each batch by z-order
        for batch in self.batches.values_mut() {
            batch.sort_by_z_order();
        }

        // Collect and sort batches by pipeline type
        let mut batches: Vec<Batch> = self.batches.into_values().collect();
        batches.sort_by(|a, b| {
            // Sort by pipeline type for minimal pipeline switches
            let a_type = a.key.pipeline_type as u32;
            let b_type = b.key.pipeline_type as u32;
            let a_tex = a.key.texture_id.map(|t| t.as_u64()).unwrap_or(0);
            let b_tex = b.key.texture_id.map(|t| t.as_u64()).unwrap_or(0);
            let a_blend = a.key.blend_mode as u32;
            let b_blend = b.key.blend_mode as u32;
            a_type
                .cmp(&b_type)
                .then(a_tex.cmp(&b_tex))
                .then(a_blend.cmp(&b_blend))
        });

        batches
    }

    /// Get the number of batches.
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    /// Get the total number of commands.
    pub fn command_count(&self) -> usize {
        self.batches.values().map(|b| b.len()).sum()
    }
}

impl Default for BatchSorter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Batch renderer info
// ---------------------------------------------------------------------------

/// Information about the batched render pass.
#[derive(Debug, Clone)]
pub struct BatchInfo {
    /// Number of batches.
    pub batch_count: usize,
    /// Total number of instances.
    pub total_instances: usize,
    /// Number of pipeline switches.
    pub pipeline_switches: usize,
    /// Number of texture switches.
    pub texture_switches: usize,
    /// Batches by pipeline type.
    pub batches_by_pipeline: HashMap<PipelineType, usize>,
}

impl BatchInfo {
    /// Analyze batches and produce batch info.
    pub fn from_batches(batches: &[Batch]) -> Self {
        let mut pipeline_switches = 0;
        let mut texture_switches = 0;
        let mut last_pipeline = None;
        let mut last_texture = None;
        let mut batches_by_pipeline = HashMap::new();

        for batch in batches {
            *batches_by_pipeline
                .entry(batch.key.pipeline_type)
                .or_insert(0) += 1;

            if last_pipeline != Some(batch.key.pipeline_type) {
                pipeline_switches += 1;
                last_pipeline = Some(batch.key.pipeline_type);
            }

            if last_texture != batch.key.texture_id {
                texture_switches += 1;
                last_texture = batch.key.texture_id;
            }
        }

        Self {
            batch_count: batches.len(),
            total_instances: batches.iter().map(|b| b.len()).sum(),
            pipeline_switches,
            texture_switches,
            batches_by_pipeline,
        }
    }

    /// Check if the batching is efficient.
    ///
    /// Efficient batching uses fewer than 10 batches and fewer than 5
    /// texture switches.
    pub fn is_efficient(&self) -> bool {
        self.batch_count < 10 && self.texture_switches < 5
    }
}

// ---------------------------------------------------------------------------
// Instance buffer builder
// ---------------------------------------------------------------------------

/// Builds an instance buffer from batches.
///
/// Collects all quad instances from batches into a contiguous buffer
/// ready for GPU upload.
#[derive(Debug, Clone)]
pub struct InstanceBufferBuilder {
    /// Collected instances.
    instances: Vec<QuadInstance>,
    /// Capacity limit.
    capacity: usize,
}

impl InstanceBufferBuilder {
    /// Create a new instance buffer builder.
    pub fn new(capacity: usize) -> Self {
        Self {
            instances: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Add instances from a batch.
    pub fn add_batch(&mut self, batch: &Batch) {
        let remaining = self.capacity.saturating_sub(self.instances.len());
        let to_add = batch.len().min(remaining);

        for cmd in &batch.commands[..to_add] {
            self.instances.push(cmd.instance);
        }
    }

    /// Add a single instance.
    pub fn push(&mut self, instance: QuadInstance) {
        if self.instances.len() < self.capacity {
            self.instances.push(instance);
        }
    }

    /// Get the instances as a slice.
    pub fn as_slice(&self) -> &[QuadInstance] {
        &self.instances
    }

    /// Get the number of instances.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.instances.clear();
    }

    /// Take ownership of the instances.
    pub fn into_inner(self) -> Vec<QuadInstance> {
        self.instances
    }

    /// Get the buffer size in bytes.
    pub fn size_bytes(&self) -> u64 {
        (self.instances.len() as u64).saturating_mul(QuadInstance::SIZE as u64)
    }
}

impl Default for InstanceBufferBuilder {
    fn default() -> Self {
        Self::new(4096)
    }
}

// ---------------------------------------------------------------------------
// Draw call estimator
// ---------------------------------------------------------------------------

/// Estimates the number of draw calls needed for a set of draw commands.
///
/// Used for performance monitoring and batch efficiency analysis.
pub struct DrawCallEstimator;

impl DrawCallEstimator {
    /// Estimate the minimum number of draw calls for the given commands.
    ///
    /// Each unique combination of pipeline type, texture, and blend mode
    /// requires at least one draw call.
    pub fn estimate(commands: &[DrawCommand]) -> usize {
        let mut keys: std::collections::HashSet<(PipelineType, Option<TextureId>)> =
            std::collections::HashSet::new();

        for cmd in commands {
            match cmd {
                DrawCommand::Rectangle { .. } => {
                    keys.insert((PipelineType::Solid2D, None));
                }
                DrawCommand::Image { texture, .. } => {
                    keys.insert((PipelineType::TexturedQuad, Some(*texture)));
                }
                DrawCommand::Text { .. } => {
                    keys.insert((PipelineType::Text, None));
                }
                DrawCommand::Video { texture, .. } => {
                    keys.insert((PipelineType::TexturedQuad, Some(*texture)));
                }
                _ => {}
            }
        }

        keys.len()
    }

    /// Estimate the number of draw calls after batching.
    pub fn estimate_batched(batches: &[Batch]) -> usize {
        batches.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn test_batch_key_solid() {
        let key = BatchKey::solid(BlendMode::Alpha, 0);
        assert_eq!(key.pipeline_type, PipelineType::Solid2D);
        assert!(key.texture_id.is_none());
        assert_eq!(key.blend_mode, BlendMode::Alpha);
    }

    #[test]
    fn test_batch_key_textured() {
        let tex = TextureId::new(1);
        let key = BatchKey::textured(tex, BlendMode::Alpha, 0);
        assert_eq!(key.pipeline_type, PipelineType::TexturedQuad);
        assert_eq!(key.texture_id, Some(tex));
    }

    #[test]
    fn test_batch_add_command() {
        let mut batch = Batch::new(BatchKey::solid(BlendMode::Alpha, 0));
        let cmd = BatchedCommand {
            key: batch.key,
            z_order: 0,
            instance: QuadInstance::solid([10.0, 20.0], [100.0, 50.0], [1.0; 4]),
            original: DrawCommandType::Rectangle,
        };
        batch.add(cmd);
        assert_eq!(batch.len(), 1);
        assert!(!batch.is_empty());
    }

    #[test]
    fn test_batch_sort_by_z_order() {
        let mut batch = Batch::new(BatchKey::solid(BlendMode::Alpha, 0));
        batch.add(BatchedCommand {
            key: batch.key,
            z_order: 2,
            instance: QuadInstance::default(),
            original: DrawCommandType::Rectangle,
        });
        batch.add(BatchedCommand {
            key: batch.key,
            z_order: 0,
            instance: QuadInstance::default(),
            original: DrawCommandType::Rectangle,
        });
        batch.add(BatchedCommand {
            key: batch.key,
            z_order: 1,
            instance: QuadInstance::default(),
            original: DrawCommandType::Rectangle,
        });

        batch.sort_by_z_order();
        assert_eq!(batch.commands[0].z_order, 0);
        assert_eq!(batch.commands[1].z_order, 1);
        assert_eq!(batch.commands[2].z_order, 2);
    }

    #[test]
    fn test_batch_bounding_rect() {
        let mut batch = Batch::new(BatchKey::solid(BlendMode::Alpha, 0));
        batch.add(BatchedCommand {
            key: batch.key,
            z_order: 0,
            instance: QuadInstance::solid([0.0, 0.0], [100.0, 50.0], [1.0; 4]),
            original: DrawCommandType::Rectangle,
        });
        batch.add(BatchedCommand {
            key: batch.key,
            z_order: 1,
            instance: QuadInstance::solid([50.0, 25.0], [100.0, 50.0], [1.0; 4]),
            original: DrawCommandType::Rectangle,
        });

        // Bounding rect should encompass both quads
        assert!((batch.bounding_rect.x - 0.0).abs() < f32::EPSILON);
        assert!((batch.bounding_rect.y - 0.0).abs() < f32::EPSILON);
        assert!(batch.bounding_rect.width >= 150.0);
        assert!(batch.bounding_rect.height >= 75.0);
    }

    #[test]
    fn test_batch_sorter_rectangle() {
        let mut sorter = BatchSorter::new();
        sorter.add_command(&DrawCommand::Rectangle {
            rect: Rect::new(10.0, 20.0, 100.0, 50.0),
            color: Color::RED,
            radius: 0.0,
            stroke: None,
            stroke_width: 0.0,
        });

        assert_eq!(sorter.command_count(), 1);
        assert_eq!(sorter.batch_count(), 1);
    }

    #[test]
    fn test_batch_sorter_image() {
        let mut sorter = BatchSorter::new();
        sorter.add_command(&DrawCommand::Image {
            dest_rect: Rect::new(0.0, 0.0, 64.0, 64.0),
            texture: TextureId::new(1),
            source_uv: Rect::new(0.0, 0.0, 1.0, 1.0),
            tint: Color::WHITE,
        });

        let batches = sorter.finish();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
    }

    #[test]
    fn test_batch_sorter_groups_by_texture() {
        let mut sorter = BatchSorter::new();

        // Two images with the same texture
        sorter.add_command(&DrawCommand::Image {
            dest_rect: Rect::new(0.0, 0.0, 64.0, 64.0),
            texture: TextureId::new(1),
            source_uv: Rect::new(0.0, 0.0, 0.5, 0.5),
            tint: Color::WHITE,
        });
        sorter.add_command(&DrawCommand::Image {
            dest_rect: Rect::new(70.0, 0.0, 64.0, 64.0),
            texture: TextureId::new(1),
            source_uv: Rect::new(0.5, 0.0, 1.0, 0.5),
            tint: Color::WHITE,
        });

        // Image with different texture
        sorter.add_command(&DrawCommand::Image {
            dest_rect: Rect::new(140.0, 0.0, 64.0, 64.0),
            texture: TextureId::new(2),
            source_uv: Rect::new(0.0, 0.0, 1.0, 1.0),
            tint: Color::WHITE,
        });

        let batches = sorter.finish();
        // Should have 2 batches: one for texture 1, one for texture 2
        assert_eq!(batches.len(), 2);
    }

    #[test]
    fn test_batch_sorter_mixed_commands() {
        let mut sorter = BatchSorter::new();

        sorter.add_command(&DrawCommand::Rectangle {
            rect: Rect::new(0.0, 0.0, 200.0, 100.0),
            color: Color::BLACK,
            radius: 0.0,
            stroke: None,
            stroke_width: 0.0,
        });

        sorter.add_command(&DrawCommand::Image {
            dest_rect: Rect::new(10.0, 10.0, 64.0, 64.0),
            texture: TextureId::new(1),
            source_uv: Rect::new(0.0, 0.0, 1.0, 1.0),
            tint: Color::WHITE,
        });

        sorter.add_command(&DrawCommand::Text {
            position: crate::geometry::Point::new(10.0, 90.0),
            content: "Hello".to_string(),
            font_size: 14.0,
            color: Color::WHITE,
        });

        let batches = sorter.finish();
        assert!(batches.len() >= 2);
    }

    #[test]
    fn test_batch_sorter_clip() {
        let mut sorter = BatchSorter::new();

        sorter.add_command(&DrawCommand::Rectangle {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            color: Color::RED,
            radius: 0.0,
            stroke: None,
            stroke_width: 0.0,
        });

        sorter.add_command(&DrawCommand::PushClip {
            clip_rect: Rect::new(10.0, 10.0, 80.0, 80.0),
        });

        sorter.add_command(&DrawCommand::Rectangle {
            rect: Rect::new(20.0, 20.0, 60.0, 60.0),
            color: Color::BLUE,
            radius: 0.0,
            stroke: None,
            stroke_width: 0.0,
        });

        sorter.add_command(&DrawCommand::PopClip);

        let batches = sorter.finish();
        // The two rectangles should be in different batches due to clip index
        assert!(batches.len() >= 2);
    }

    #[test]
    fn test_batch_info() {
        let mut sorter = BatchSorter::new();

        for i in 0..5 {
            sorter.add_command(&DrawCommand::Image {
                dest_rect: Rect::new(i as f32 * 70.0, 0.0, 64.0, 64.0),
                texture: TextureId::new(1),
                source_uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                tint: Color::WHITE,
            });
        }

        let batches = sorter.finish();
        let info = BatchInfo::from_batches(&batches);

        assert_eq!(info.batch_count, 1);
        assert_eq!(info.total_instances, 5);
        assert!(info.is_efficient());
    }

    #[test]
    fn test_batch_info_inefficient() {
        let mut sorter = BatchSorter::new();

        // Many different textures
        for i in 0..20u64 {
            sorter.add_command(&DrawCommand::Image {
                dest_rect: Rect::new(0.0, 0.0, 64.0, 64.0),
                texture: TextureId::new(i),
                source_uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                tint: Color::WHITE,
            });
        }

        let batches = sorter.finish();
        let info = BatchInfo::from_batches(&batches);

        assert_eq!(info.batch_count, 20);
        assert!(!info.is_efficient());
    }

    #[test]
    fn test_instance_buffer_builder() {
        let mut builder = InstanceBufferBuilder::new(100);

        let batch = Batch {
            key: BatchKey::solid(BlendMode::Alpha, 0),
            commands: vec![
                BatchedCommand {
                    key: BatchKey::solid(BlendMode::Alpha, 0),
                    z_order: 0,
                    instance: QuadInstance::solid([0.0, 0.0], [10.0, 10.0], [1.0; 4]),
                    original: DrawCommandType::Rectangle,
                },
                BatchedCommand {
                    key: BatchKey::solid(BlendMode::Alpha, 0),
                    z_order: 1,
                    instance: QuadInstance::solid([20.0, 0.0], [10.0, 10.0], [0.0, 1.0, 0.0, 1.0]),
                    original: DrawCommandType::Rectangle,
                },
            ],
            bounding_rect: Rect::new(0.0, 0.0, 30.0, 10.0),
        };

        builder.add_batch(&batch);
        assert_eq!(builder.len(), 2);
        assert_eq!(builder.size_bytes(), 2 * QuadInstance::SIZE as u64);
    }

    #[test]
    fn test_instance_buffer_builder_capacity() {
        let mut builder = InstanceBufferBuilder::new(1);
        builder.push(QuadInstance::default());
        builder.push(QuadInstance::default()); // Should be ignored (exceeds capacity)
        assert_eq!(builder.len(), 1);
    }

    #[test]
    fn test_draw_call_estimator() {
        let zero_rect = Rect::new(0.0, 0.0, 0.0, 0.0);
        let commands = vec![
            DrawCommand::Rectangle {
                rect: zero_rect,
                color: Color::RED,
                radius: 0.0,
                stroke: None,
                stroke_width: 0.0,
            },
            DrawCommand::Image {
                dest_rect: zero_rect,
                texture: TextureId::new(1),
                source_uv: zero_rect,
                tint: Color::WHITE,
            },
            DrawCommand::Text {
                position: crate::geometry::Point::new(0.0, 0.0),
                content: "Test".to_string(),
                font_size: 14.0,
                color: Color::WHITE,
            },
        ];

        let estimated = DrawCallEstimator::estimate(&commands);
        assert_eq!(estimated, 3); // Solid, Textured, Text
    }

    #[test]
    fn test_batch_collect_instances() {
        let mut batch = Batch::new(BatchKey::solid(BlendMode::Alpha, 0));
        batch.add(BatchedCommand {
            key: batch.key,
            z_order: 0,
            instance: QuadInstance::solid([0.0, 0.0], [10.0, 10.0], [1.0; 4]),
            original: DrawCommandType::Rectangle,
        });

        let mut buffer = Vec::new();
        batch.collect_instances(&mut buffer);
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn test_batch_sorter_opacity() {
        let mut sorter = BatchSorter::new();

        sorter.add_command(&DrawCommand::SetOpacity { opacity: 0.5 });
        sorter.add_command(&DrawCommand::Rectangle {
            rect: Rect::new(0.0, 0.0, 100.0, 100.0),
            color: Color::RED,
            radius: 0.0,
            stroke: None,
            stroke_width: 0.0,
        });

        let batches = sorter.finish();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].key.blend_mode, BlendMode::Alpha);
    }

    #[test]
    fn test_batch_key_hash() {
        use std::collections::HashSet;

        let key1 = BatchKey::solid(BlendMode::Alpha, 0);
        let key2 = BatchKey::solid(BlendMode::Alpha, 0);
        let key3 = BatchKey::solid(BlendMode::Opaque, 0);

        let mut set = HashSet::new();
        set.insert(key1);
        set.insert(key2); // Same as key1, should not increase size
        set.insert(key3);

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_instance_buffer_builder_clear() {
        let mut builder = InstanceBufferBuilder::new(100);
        builder.push(QuadInstance::default());
        builder.push(QuadInstance::default());
        assert_eq!(builder.len(), 2);

        builder.clear();
        assert!(builder.is_empty());
    }

    #[test]
    fn test_draw_call_estimator_same_texture() {
        let commands = vec![
            DrawCommand::Image {
                dest_rect: Rect::new(0.0, 0.0, 64.0, 64.0),
                texture: TextureId::new(1),
                source_uv: Rect::new(0.0, 0.0, 0.5, 0.5),
                tint: Color::WHITE,
            },
            DrawCommand::Image {
                dest_rect: Rect::new(70.0, 0.0, 64.0, 64.0),
                texture: TextureId::new(1),
                source_uv: Rect::new(0.5, 0.0, 1.0, 0.5),
                tint: Color::WHITE,
            },
        ];

        let estimated = DrawCallEstimator::estimate(&commands);
        // Same texture, so only 1 draw call needed
        assert_eq!(estimated, 1);
    }

    #[test]
    fn test_batch_sorter_video() {
        let mut sorter = BatchSorter::new();
        sorter.add_command(&DrawCommand::Video {
            dest_rect: Rect::new(0.0, 0.0, 640.0, 480.0),
            texture: TextureId::new(100),
            source_uv: Rect::new(0.0, 0.0, 1.0, 1.0),
            tint: Color::WHITE,
        });

        let batches = sorter.finish();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0].commands[0].original, DrawCommandType::Video);
    }
}
