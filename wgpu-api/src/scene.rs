//! Retained scene graph for 2D rendering.
//!
//! Each node has position, size, opacity, scale, rotation, visibility,
//! z-order, parent relationship, and content. The scene graph is a tree
//! of nodes that the renderer traverses to produce draw calls.
//!
//! Icons are simply image nodes. Popups are groups of nodes with their
//! own clipping and positioning. No separate rendering system is needed
//! for specialized UI elements.

use std::collections::{HashMap, HashSet};

use crate::geometry::{Point, Size, Rect, NodeId};
use crate::color::Color;
use crate::texture::TextureId;
use crate::layout::{Insets, CornerRadius, Shadow};
use crate::brush::Brush;
use crate::text::{TextStyle, TextAlign, TextBaseline};
use crate::video::VideoNode;

// ---------------------------------------------------------------------------
// Node style
// ---------------------------------------------------------------------------

/// Style properties for a scene node.
///
/// Controls the transform, visibility, and compositing of a node within
/// the scene graph.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeStyle {
    /// Position in parent coordinates (top-left corner).
    pub position: Point,
    /// Size in logical pixels.
    pub size: Size,
    /// Opacity from 0.0 (transparent) to 1.0 (opaque).
    pub opacity: f32,
    /// Scale factor (1.0 = no scaling).
    pub scale: Point,
    /// Rotation in degrees (clockwise).
    pub rotation: f32,
    /// Whether the node and its children are visible.
    pub visible: bool,
    /// Z-order index for sibling ordering (higher = on top).
    pub z_index: i32,
    /// Anchor point within the node (0..1, where (0,0) is top-left).
    pub anchor: Point,
    /// Whether the node is enabled for interaction.
    pub enabled: bool,
}

impl NodeStyle {
    /// Default style with zero position, zero size, fully opaque and visible.
    pub const fn new() -> Self {
        Self {
            position: Point::ORIGIN,
            size: Size::ZERO,
            opacity: 1.0,
            scale: Point { x: 1.0, y: 1.0 },
            rotation: 0.0,
            visible: true,
            z_index: 0,
            anchor: Point::ORIGIN,
            enabled: true,
        }
    }

    /// Set the position.
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.position = Point::new(x, y);
        self
    }

    /// Set the size.
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    /// Set the opacity.
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Set the scale.
    pub fn with_scale(mut self, scale: Point) -> Self {
        self.scale = scale;
        self
    }

    /// Set uniform scale.
    pub fn with_uniform_scale(mut self, scale: f32) -> Self {
        self.scale = Point::new(scale, scale);
        self
    }

    /// Set the rotation in degrees.
    pub fn with_rotation(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }

    /// Set visibility.
    pub fn with_visibility(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Set the z-index.
    pub fn with_z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }

    /// Set the anchor point.
    pub fn with_anchor(mut self, anchor: Point) -> Self {
        self.anchor = anchor;
        self
    }

    /// Check if this node is effectively invisible.
    pub fn is_invisible(&self) -> bool {
        !self.visible || self.opacity <= 0.0
    }

    /// Compute the bounding rectangle in parent coordinates.
    pub fn bounding_rect(&self) -> Rect {
        Rect::new(self.position.x, self.position.y, self.size.width, self.size.height)
    }
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Node content
// ---------------------------------------------------------------------------

/// Rectangle content for a scene node.
#[derive(Debug, Clone, PartialEq)]
pub struct Rectangle {
    /// Fill color or brush.
    pub color: Color,
    /// Corner radius for rounded rectangles.
    pub radius: f32,
    /// Optional stroke color.
    pub stroke: Option<Color>,
    /// Stroke width.
    pub stroke_width: f32,
    /// Optional drop shadow.
    pub shadow: Option<Shadow>,
}

impl Rectangle {
    /// Create a solid rectangle with the given color.
    pub fn new(color: Color) -> Self {
        Self {
            color,
            radius: 0.0,
            stroke: None,
            stroke_width: 0.0,
            shadow: None,
        }
    }

    /// Create a rounded rectangle.
    pub fn rounded(color: Color, radius: f32) -> Self {
        Self {
            color,
            radius,
            stroke: None,
            stroke_width: 0.0,
            shadow: None,
        }
    }

    /// Set the corner radius.
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Set the stroke color and width.
    pub fn with_stroke(mut self, color: Color, width: f32) -> Self {
        self.stroke = Some(color);
        self.stroke_width = width;
        self
    }

    /// Set a drop shadow.
    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
}

impl From<Color> for Rectangle {
    fn from(color: Color) -> Self {
        Self::new(color)
    }
}

/// Image content for a scene node.
///
/// An icon is simply an image node. The texture reference points to a
/// GPU-resident texture managed by the Renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneImage {
    /// Texture to render.
    pub texture: TextureId,
    /// Optional tint color applied to the image.
    pub tint: Color,
    /// Source rectangle within the texture (UV coordinates, 0..1).
    pub source_rect: Option<Rect>,
    /// Whether to preserve the aspect ratio.
    pub preserve_aspect_ratio: bool,
    /// Optional clipping insets (nine-patch style).
    pub insets: Option<Insets>,
}

impl SceneImage {
    /// Create an image node from a texture.
    pub fn new(texture: TextureId) -> Self {
        Self {
            texture,
            tint: Color::WHITE,
            source_rect: None,
            preserve_aspect_ratio: false,
            insets: None,
        }
    }

    /// Apply a tint color.
    pub fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }

    /// Set the source rectangle (UV coordinates).
    pub fn with_source_rect(mut self, rect: Rect) -> Self {
        self.source_rect = Some(rect);
        self
    }

    /// Preserve the aspect ratio when scaling.
    pub fn with_preserve_aspect_ratio(mut self, preserve: bool) -> Self {
        self.preserve_aspect_ratio = preserve;
        self
    }

    /// Set nine-patch insets.
    pub fn with_insets(mut self, insets: Insets) -> Self {
        self.insets = Some(insets);
        self
    }
}

/// Text content for a scene node.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneText {
    /// Text string to render.
    pub content: String,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Text color or brush.
    pub color: Color,
    /// Optional text style for advanced formatting.
    pub style: Option<TextStyle>,
    /// Text alignment within the node's bounding box.
    pub alignment: TextAlign,
    /// Text baseline for vertical positioning.
    pub baseline: TextBaseline,
    /// Maximum number of lines (0 = unlimited).
    pub max_lines: u32,
    /// Truncation suffix for overflow (e.g., "...").
    pub truncation: Option<String>,
}

impl SceneText {
    /// Create a text node with the given content, font size, and color.
    pub fn new(content: impl Into<String>, font_size: f32, color: Color) -> Self {
        Self {
            content: content.into(),
            font_size,
            color,
            style: None,
            alignment: TextAlign::Left,
            baseline: TextBaseline::Alphabetic,
            max_lines: 0,
            truncation: None,
        }
    }

    /// Set the text style.
    pub fn with_style(mut self, style: TextStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Set the text alignment.
    pub fn with_alignment(mut self, alignment: TextAlign) -> Self {
        self.alignment = alignment;
        self
    }

    /// Set the maximum number of lines.
    pub fn with_max_lines(mut self, max: u32) -> Self {
        self.max_lines = max;
        self
    }

    /// Set the truncation suffix.
    pub fn with_truncation(mut self, suffix: impl Into<String>) -> Self {
        self.truncation = Some(suffix.into());
        self
    }
}

/// Group content — a container for child nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Optional clip region (clips children to this rectangle).
    pub clip: Option<Rect>,
    /// Whether to clip children to the group's bounds.
    pub clip_to_bounds: bool,
    /// Background fill for the group.
    pub background: Option<Brush>,
    /// Corner radius for clipped bounds.
    pub corner_radius: CornerRadius,
}

impl Group {
    /// Create an empty group.
    pub fn new() -> Self {
        Self {
            clip: None,
            clip_to_bounds: false,
            background: None,
            corner_radius: CornerRadius::ZERO,
        }
    }

    /// Clip children to the group's bounds.
    pub fn with_clip_to_bounds(mut self, clip: bool) -> Self {
        self.clip_to_bounds = clip;
        self
    }

    /// Set a background fill.
    pub fn with_background(mut self, brush: Brush) -> Self {
        self.background = Some(brush);
        self
    }

    /// Set a custom clip region.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Set corner radius for clipped bounds.
    pub fn with_corner_radius(mut self, radius: CornerRadius) -> Self {
        self.corner_radius = radius;
        self
    }
}

impl Default for Group {
    fn default() -> Self {
        Self::new()
    }
}

/// Content of a scene node.
///
/// Each node renders exactly one type of content. A Group is a container
/// that holds child nodes. Icons are represented as Image nodes. Popups
/// are groups of nodes with their own clipping and positioning.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeContent {
    /// A solid or rounded rectangle.
    Rectangle(Rectangle),
    /// An image from a texture.
    Image(SceneImage),
    /// Rendered text.
    Text(SceneText),
    /// A group of child nodes.
    Group(Group),
    /// A live video stream.
    Video(VideoNode),
}

impl NodeContent {
    /// Check if this content is a group (container).
    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group(_))
    }

    /// Get the rectangle content if applicable.
    pub fn as_rectangle(&self) -> Option<&Rectangle> {
        match self {
            Self::Rectangle(r) => Some(r),
            _ => None,
        }
    }

    /// Get the image content if applicable.
    pub fn as_image(&self) -> Option<&SceneImage> {
        match self {
            Self::Image(i) => Some(i),
            _ => None,
        }
    }

    /// Get the text content if applicable.
    pub fn as_text(&self) -> Option<&SceneText> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }

    /// Get the video content if applicable.
    pub fn as_video(&self) -> Option<&VideoNode> {
        match self {
            Self::Video(v) => Some(v),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Scene node
// ---------------------------------------------------------------------------

/// A node in the scene graph.
///
/// Each node has a unique ID, a parent relationship, style properties,
/// and content. The scene graph is a tree structure where groups contain
/// children and leaf nodes render visual content.
#[derive(Debug, Clone)]
pub struct Node {
    /// Unique identifier.
    pub id: NodeId,
    /// Parent node ID (None for root nodes).
    pub parent: Option<NodeId>,
    /// Style properties.
    pub style: NodeStyle,
    /// Visual content.
    pub content: NodeContent,
    /// Child node IDs (ordered by insertion).
    pub children: Vec<NodeId>,
    /// User data attached to this node.
    pub data: Option<String>,
}

impl Node {
    /// Create a new node with the given ID and content.
    pub fn new(id: NodeId, content: NodeContent) -> Self {
        Self {
            id,
            parent: None,
            style: NodeStyle::default(),
            content,
            children: Vec::new(),
            data: None,
        }
    }

    /// Create a group node.
    pub fn group(id: NodeId) -> Self {
        Self::new(id, NodeContent::Group(Group::new()))
    }

    /// Create a rectangle node.
    pub fn rectangle(id: NodeId, color: Color) -> Self {
        Self::new(id, NodeContent::Rectangle(Rectangle::new(color)))
    }

    /// Create an image node.
    pub fn image(id: NodeId, texture: TextureId) -> Self {
        Self::new(id, NodeContent::Image(SceneImage::new(texture)))
    }

    /// Create a text node.
    pub fn text(id: NodeId, content: impl Into<String>, font_size: f32, color: Color) -> Self {
        Self::new(id, NodeContent::Text(SceneText::new(content, font_size, color)))
    }

    /// Create a video node.
    pub fn video(id: NodeId, stream_id: crate::video::StreamId) -> Self {
        Self::new(id, NodeContent::Video(VideoNode::new(stream_id)))
    }

    /// Set user data.
    pub fn with_data(mut self, data: impl Into<String>) -> Self {
        self.data = Some(data.into());
        self
    }

    /// Check if this node is a leaf (no children).
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Check if this node is visible (considering style).
    pub fn is_visible(&self) -> bool {
        self.style.visible && !self.style.is_invisible()
    }
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------

/// A retained scene graph.
///
/// Manages a tree of nodes that the renderer traverses to produce draw calls.
/// Nodes are stored in a hash map for O(1) lookup by ID, and the tree
/// structure is maintained via parent/child relationships.
pub struct Scene {
    /// All nodes in the scene.
    nodes: HashMap<NodeId, Node>,
    /// Next node ID counter.
    next_id: u64,
    /// Root node IDs.
    roots: Vec<NodeId>,
}

impl Scene {
    /// Create a new empty scene.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            next_id: 1,
            roots: Vec::new(),
        }
    }

    /// Allocate a new node ID.
    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Add a node to the scene.
    pub fn add_node(&mut self, node: Node) -> NodeId {
        let id = node.id;
        if let Some(parent) = node.parent {
            if let Some(parent_node) = self.nodes.get_mut(&parent) {
                parent_node.children.push(id);
            }
        } else {
            self.roots.push(id);
        }
        self.nodes.insert(id, node);
        id
    }

    /// Create and add a new node with the given content.
    pub fn create_node(&mut self, content: NodeContent) -> NodeId {
        let id = self.alloc_id();
        let node = Node::new(id, content);
        self.add_node(node);
        id
    }

    /// Create a group node.
    pub fn create_group(&mut self) -> NodeId {
        let id = self.alloc_id();
        let node = Node::group(id);
        self.add_node(node);
        id
    }

    /// Create a group node as a child of the given parent.
    pub fn create_group_under(&mut self, parent: NodeId) -> NodeId {
        let id = self.alloc_id();
        let mut node = Node::group(id);
        node.parent = Some(parent);
        self.add_node(node);
        id
    }

    /// Create a rectangle node.
    pub fn create_rectangle(&mut self, color: Color) -> NodeId {
        let id = self.alloc_id();
        let node = Node::rectangle(id, color);
        self.add_node(node);
        id
    }

    /// Create a rectangle node under a parent.
    pub fn create_rectangle_under(&mut self, parent: NodeId, color: Color) -> NodeId {
        let id = self.alloc_id();
        let mut node = Node::rectangle(id, color);
        node.parent = Some(parent);
        self.add_node(node);
        id
    }

    /// Create an image node.
    pub fn create_image(&mut self, texture: TextureId) -> NodeId {
        let id = self.alloc_id();
        let node = Node::image(id, texture);
        self.add_node(node);
        id
    }

    /// Create a text node.
    pub fn create_text(&mut self, content: impl Into<String>, font_size: f32, color: Color) -> NodeId {
        let id = self.alloc_id();
        let node = Node::text(id, content, font_size, color);
        self.add_node(node);
        id
    }

    /// Create a text node under a parent.
    pub fn create_text_under(
        &mut self,
        parent: NodeId,
        content: impl Into<String>,
        font_size: f32,
        color: Color,
    ) -> NodeId {
        let id = self.alloc_id();
        let mut node = Node::text(id, content, font_size, color);
        node.parent = Some(parent);
        self.add_node(node);
        id
    }

    /// Get a node by ID.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Get a mutable reference to a node.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    /// Remove a node and all its children from the scene.
    pub fn remove_node(&mut self, id: NodeId) {
        self.remove_node_recursive(id);
    }

    /// Remove a node recursively.
    fn remove_node_recursive(&mut self, id: NodeId) {
        // Collect children first to avoid borrow conflicts
        let children: Vec<NodeId> = self.nodes.get(&id)
            .map(|node| node.children.clone())
            .unwrap_or_default();

        // Remove children first
        for child_id in children {
            self.remove_node_recursive(child_id);
        }

        // Remove from parent's children list
        if let Some(node) = self.nodes.get(&id) {
            if let Some(parent_id) = node.parent {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    parent.children.retain(|&c| c != id);
                }
            }
        }

        // Remove from roots
        self.roots.retain(|&r| r != id);

        // Remove the node
        self.nodes.remove(&id);
    }

    /// Add a child to a parent node.
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), String> {
        // Remove child from current parent
        if let Some(child_node) = self.nodes.get_mut(&child) {
            if let Some(old_parent) = child_node.parent {
                if let Some(parent_node) = self.nodes.get_mut(&old_parent) {
                    parent_node.children.retain(|&c| c != child);
                }
                // Remove from roots if it was a root
                self.roots.retain(|&r| r != child);
            }
        }

        // Set new parent
        if let Some(child_node) = self.nodes.get_mut(&child) {
            child_node.parent = Some(parent);
        }

        // Add to parent's children
        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            parent_node.children.push(child);
        } else {
            return Err(format!("Parent node {:?} not found", parent));
        }

        Ok(())
    }

    /// Remove a child from its parent.
    pub fn remove_child(&mut self, child: NodeId) -> Result<(), String> {
        // Extract parent_id to avoid double mutable borrow
        let parent_id = self.nodes.get(&child).and_then(|n| n.parent);
        if let Some(pid) = parent_id {
            if let Some(parent) = self.nodes.get_mut(&pid) {
                parent.children.retain(|&c| c != child);
            }
            if let Some(child_node) = self.nodes.get_mut(&child) {
                child_node.parent = None;
            }
        }
        Ok(())
    }

    /// Get the root nodes.
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// Get the children of a node.
    pub fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.nodes.get(&id).map(|n| n.children.as_slice())
    }

    /// Get the parent of a node.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.get(&id).and_then(|n| n.parent)
    }

    /// Get the number of nodes in the scene.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the scene is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Clear all nodes from the scene.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.roots.clear();
    }

    /// Traverse the scene graph in document order (depth-first).
    /// Yields only visible nodes.
    pub fn traverse_visible<F>(&self, mut visit: F)
    where
        F: FnMut(&Node),
    {
        let mut stack = Vec::new();
        for root_id in &self.roots {
            stack.push(*root_id);
        }

        while let Some(id) = stack.pop() {
            if let Some(node) = self.nodes.get(&id) {
                if node.is_visible() {
                    visit(node);
                }
                // Push children in reverse order so they are visited in order
                for child_id in node.children.iter().rev() {
                    stack.push(*child_id);
                }
            }
        }
    }

    /// Traverse the scene graph and collect visible nodes sorted by z-index.
    pub fn visible_nodes(&self) -> Vec<NodeId> {
        let mut result = Vec::new();
        self.traverse_visible(|node| {
            result.push(node.id);
        });
        result.sort_by_key(|id| {
            self.nodes.get(id).map(|n| n.style.z_index).unwrap_or(0)
        });
        result
    }

    /// Collect all textures referenced by the scene.
    pub fn referenced_textures(&self) -> HashSet<TextureId> {
        let mut textures = HashSet::new();
        self.traverse_visible(|node| {
            if let NodeContent::Image(img) = &node.content {
                textures.insert(img.texture);
            }
        });
        textures
    }

    /// Estimate the number of draw calls needed for this scene.
    pub fn estimate_draw_calls(&self) -> usize {
        let mut count = 0;
        self.traverse_visible(|node| {
            match &node.content {
                NodeContent::Rectangle(_) => count += 1,
                NodeContent::Image(_) => count += 1,
                NodeContent::Text(_) => count += 1,
                NodeContent::Group(g) => {
                    // Group background may add a draw call
                    if g.background.is_some() {
                        count += 1;
                    }
                }
                NodeContent::Video(_) => {
                    // Video renders as a textured quad
                    count += 1;
                }
            }
        });
        count
    }

    /// Hit-test: find the topmost node at the given point.
    ///
    /// Returns the node ID if a visible node at the given position is found.
    /// Searches in reverse z-order (topmost first).
    pub fn hit_test(&self, point: Point) -> Option<NodeId> {
        let mut nodes = self.visible_nodes();
        nodes.reverse();

        for id in nodes {
            if let Some(node) = self.nodes.get(&id) {
                let rect = node.style.bounding_rect();
                if rect.contains(&point) {
                    return Some(id);
                }
            }
        }
        None
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl std::iter::IntoIterator for Scene {
    type Item = Node;
    type IntoIter = std::collections::hash_map::IntoValues<NodeId, Node>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.into_values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_style_defaults() {
        let style = NodeStyle::new();
        assert!(style.visible);
        assert!((style.opacity - 1.0).abs() < f32::EPSILON);
        assert!(style.is_invisible() == false);
    }

    #[test]
    fn test_node_style_invisible() {
        let style = NodeStyle::new().with_visibility(false);
        assert!(style.is_invisible());

        let style = NodeStyle::new().with_opacity(0.0);
        assert!(style.is_invisible());
    }

    #[test]
    fn test_rectangle_creation() {
        let rect = Rectangle::new(Color::RED);
        assert_eq!(rect.color, Color::RED);
        assert!((rect.radius - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rounded_rectangle() {
        let rect = Rectangle::rounded(Color::BLUE, 8.0);
        assert!((rect.radius - 8.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_scene_image() {
        let img = SceneImage::new(TextureId::new(42));
        assert_eq!(img.texture, TextureId::new(42));
        assert_eq!(img.tint, Color::WHITE);
    }

    #[test]
    fn test_scene_text() {
        let text = SceneText::new("Hello", 16.0, Color::BLACK);
        assert_eq!(text.content, "Hello");
        assert!((text.font_size - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_scene_add_node() {
        let mut scene = Scene::new();
        let id = scene.create_rectangle(Color::GREEN);
        assert_eq!(scene.node_count(), 1);
        assert!(scene.get(id).is_some());
    }

    #[test]
    fn test_scene_parent_child() {
        let mut scene = Scene::new();
        let parent = scene.create_group();
        let child = scene.create_rectangle_under(parent, Color::RED);

        assert_eq!(scene.parent(child), Some(parent));
        let children = scene.children(parent).unwrap();
        assert!(children.contains(&child));
    }

    #[test]
    fn test_scene_remove_node() {
        let mut scene = Scene::new();
        let parent = scene.create_group();
        let _child = scene.create_rectangle_under(parent, Color::RED);

        scene.remove_node(parent);
        assert_eq!(scene.node_count(), 0);
    }

    #[test]
    fn test_scene_hit_test() {
        let mut scene = Scene::new();
        let id = scene.create_rectangle(Color::RED);
        if let Some(node) = scene.get_mut(id) {
            node.style.position = Point::new(0.0, 0.0);
            node.style.size = Size::new(100.0, 100.0);
        }

        let hit = scene.hit_test(Point::new(50.0, 50.0));
        assert_eq!(hit, Some(id));

        let miss = scene.hit_test(Point::new(200.0, 200.0));
        assert_eq!(miss, None);
    }

    #[test]
    fn test_scene_draw_calls() {
        let mut scene = Scene::new();
        scene.create_rectangle(Color::RED);
        scene.create_rectangle(Color::BLUE);
        scene.create_text("Hello", 12.0, Color::BLACK);

        assert_eq!(scene.estimate_draw_calls(), 3);
    }

    #[test]
    fn test_node_content_type_queries() {
        let rect_content = NodeContent::Rectangle(Rectangle::new(Color::RED));
        assert!(rect_content.as_rectangle().is_some());
        assert!(rect_content.as_image().is_none());
        assert!(!rect_content.is_group());

        let group_content = NodeContent::Group(Group::new());
        assert!(group_content.is_group());
    }

    #[test]
    fn test_group_with_background() {
        let group = Group::new().with_background(Brush::solid(Color::WHITE));
        assert!(group.background.is_some());
    }

    #[test]
    fn test_scene_traverse_visible() {
        let mut scene = Scene::new();
        let root = scene.create_group();
        let child1 = scene.create_rectangle_under(root, Color::RED);
        let child2 = scene.create_rectangle_under(root, Color::BLUE);

        // Hide child2
        if let Some(node) = scene.get_mut(child2) {
            node.style.visible = false;
        }

        let mut visited = Vec::new();
        scene.traverse_visible(|node| {
            visited.push(node.id);
        });

        // Should visit root and child1, but not child2
        assert!(visited.contains(&root));
        assert!(visited.contains(&child1));
        assert!(!visited.contains(&child2));
    }
}
