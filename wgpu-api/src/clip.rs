//! Clipping support for rectangular and rounded regions.
//!
//! This module provides `ClipRect` for scissor rectangle clipping and
//! `ClipStack` for managing nested clipping regions. A popup should be
//! able to clip its children independently of its parent.
//!
//! ## Clipping Strategies
//!
//! - **Scissor rectangles**: Simple rectangular clipping implemented with
//!   GPU scissor tests. Fast and efficient for most cases.
//!
//! - **Rounded clipping**: More complex, requires shaders or stencil
//!   techniques. Can be added later for rounded corners.
//!
//! ## Nested Clipping
//!
//! Multiple clip regions are intersected to produce the effective clip.
//! If the intersection is empty, everything is clipped out.

use std::cell::RefCell;

use crate::geometry::{Point, Rect, Size};

// ---------------------------------------------------------------------------
// Clip rectangle
// ---------------------------------------------------------------------------

/// A rectangular clipping region.
///
/// Used for scissor rectangle clipping: any content outside this rectangle
/// is not rendered. Multiple clip regions are intersected to produce the
/// effective clip.
///
/// # Example
///
/// ```
/// use wgpu_api::{ClipRect, Rect, Point};
///
/// let clip = ClipRect::new(Rect::new(0.0, 0.0, 100.0, 100.0));
/// assert!(clip.contains(&Point::new(50.0, 50.0)));
/// assert!(!clip.contains(&Point::new(150.0, 50.0)));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipRect {
    /// The clipping rectangle in screen space.
    rect: Rect,
}

impl ClipRect {
    /// Create a new clip rectangle.
    pub fn new(rect: Rect) -> Self {
        Self { rect }
    }

    /// Create from position and size.
    pub fn from_origin_size(origin: Point, size: Size) -> Self {
        Self::new(Rect::from_origin_size(&origin, &size))
    }

    /// Get the clipping rectangle.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Check if a point is inside the clip region.
    pub fn contains(&self, point: &Point) -> bool {
        self.rect.contains(point)
    }

    /// Check if a rectangle is fully inside the clip region.
    pub fn contains_rect(&self, other: &Rect) -> bool {
        self.rect.contains_rect(other)
    }

    /// Compute the intersection with another clip rectangle.
    ///
    /// Returns `None` if the rectangles do not intersect.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        self.rect.intersection(&other.rect).map(Self::new)
    }

    /// Compute the intersection with a rectangle.
    pub fn intersection_rect(&self, other: &Rect) -> Option<Self> {
        self.rect.intersection(other).map(Self::new)
    }

    /// Check if this clip rectangle is empty.
    pub fn is_empty(&self) -> bool {
        self.rect.is_empty()
    }

    /// Translate the clip rectangle.
    pub fn translate(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.rect.translate(dx, dy))
    }

    /// Inflate the clip rectangle (expand or shrink).
    pub fn inflate(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.rect.inflate(dx, dy))
    }

    /// Convert to physical pixel coordinates.
    pub fn to_physical(&self, scale_factor: f32) -> Self {
        Self::new(Rect::new(
            (self.rect.x * scale_factor).floor(),
            (self.rect.y * scale_factor).floor(),
            (self.rect.width * scale_factor).ceil(),
            (self.rect.height * scale_factor).ceil(),
        ))
    }
}

impl From<Rect> for ClipRect {
    fn from(rect: Rect) -> Self {
        Self::new(rect)
    }
}

impl From<Point> for ClipRect {
    fn from(point: Point) -> Self {
        Self::new(Rect::new(point.x, point.y, 0.0, 0.0))
    }
}

// ---------------------------------------------------------------------------
// Clip stack
// ---------------------------------------------------------------------------

/// A stack of nested clipping regions.
///
/// Each push adds a new clip region that intersects with all previous
/// regions. The effective clip is the intersection of all regions on
/// the stack.
#[derive(Debug)]
pub struct ClipStack {
    /// The stack of clip rectangles.
    clips: Vec<ClipRect>,
    /// Cached effective clip (intersection of all clips).
    effective: RefCell<Option<ClipRect>>,
}

impl ClipStack {
    /// Create a new empty clip stack.
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            effective: RefCell::new(None),
        }
    }

    /// Push a new clip rectangle onto the stack.
    pub fn push(&mut self, clip: ClipRect) {
        self.clips.push(clip);
        *self.effective.borrow_mut() = None; // Invalidate cache
    }

    /// Push a clip rectangle from a Rect.
    pub fn push_rect(&mut self, rect: Rect) {
        self.push(ClipRect::new(rect));
    }

    /// Pop the most recent clip rectangle.
    pub fn pop(&mut self) -> Option<ClipRect> {
        let clip = self.clips.pop();
        *self.effective.borrow_mut() = None; // Invalidate cache
        clip
    }

    /// Get the effective clip (intersection of all clips).
    ///
    /// Returns `None` if there are no clips (no clipping).
    pub fn effective_clip(&self) -> Option<ClipRect> {
        if self.effective.borrow().is_some() {
            return *self.effective.borrow();
        }

        let effective = self.compute_effective();
        *self.effective.borrow_mut() = effective;
        effective
    }

    /// Compute the effective clip from scratch.
    fn compute_effective(&self) -> Option<ClipRect> {
        if self.clips.is_empty() {
            return None;
        }

        let mut result = self.clips[0];
        for clip in &self.clips[1..] {
            match result.intersection(clip) {
                Some(intersection) => result = intersection,
                None => {
                    // No intersection — everything is clipped out
                    return Some(ClipRect::new(Rect::ZERO));
                }
            }
        }
        Some(result)
    }

    /// Get the number of clips on the stack.
    pub fn len(&self) -> usize {
        self.clips.len()
    }

    /// Check if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    /// Clear all clips.
    pub fn clear(&mut self) {
        self.clips.clear();
        *self.effective.borrow_mut() = None;
    }

    /// Check if a point is inside the effective clip.
    pub fn contains_point(&self, point: &Point) -> bool {
        match self.effective_clip() {
            Some(clip) => clip.contains(point),
            None => true, // No clip means everything is visible
        }
    }

    /// Check if a rectangle intersects the effective clip.
    pub fn intersects_rect(&self, rect: &Rect) -> bool {
        match self.effective_clip() {
            Some(clip) => clip.rect().intersects(rect),
            None => true, // No clip means everything is visible
        }
    }

    /// Clip a rectangle to the effective clip region.
    ///
    /// Returns the clipped rectangle, or `None` if the rectangle is
    /// completely outside the clip.
    pub fn clip_rect(&self, rect: &Rect) -> Option<Rect> {
        match self.effective_clip() {
            Some(clip) => clip.rect().intersection(rect),
            None => Some(*rect), // No clip
        }
    }

    /// Get the top clip (most recently pushed).
    pub fn top(&self) -> Option<ClipRect> {
        self.clips.last().copied()
    }

    /// Get a clip by index (0 = bottom of stack).
    pub fn get(&self, index: usize) -> Option<ClipRect> {
        self.clips.get(index).copied()
    }

    /// Iterate over all clips (bottom to top).
    pub fn iter(&self) -> impl Iterator<Item = ClipRect> + '_ {
        self.clips.iter().copied()
    }
}

impl Default for ClipStack {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for ClipStack {
    type Target = [ClipRect];

    fn deref(&self) -> &Self::Target {
        &self.clips
    }
}

// ---------------------------------------------------------------------------
// Rounded clip
// ---------------------------------------------------------------------------

/// A clip region with rounded corners.
///
/// Requires shader or stencil-based clipping. Simple scissor rectangles
/// cannot handle rounded corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundedClip {
    /// The bounding rectangle.
    rect: Rect,
    /// Top-left corner radius.
    top_left: f32,
    /// Top-right corner radius.
    top_right: f32,
    /// Bottom-right corner radius.
    bottom_right: f32,
    /// Bottom-left corner radius.
    bottom_left: f32,
}

impl RoundedClip {
    /// Create a new rounded clip with uniform corner radius.
    pub fn new(rect: Rect, radius: f32) -> Self {
        Self {
            rect,
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    /// Create with per-corner radii.
    pub fn with_corners(
        rect: Rect,
        top_left: f32,
        top_right: f32,
        bottom_right: f32,
        bottom_left: f32,
    ) -> Self {
        Self {
            rect,
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    /// Get the bounding rectangle.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Get the top-left corner radius.
    pub fn top_left(&self) -> f32 {
        self.top_left
    }

    /// Get the top-right corner radius.
    pub fn top_right(&self) -> f32 {
        self.top_right
    }

    /// Get the bottom-right corner radius.
    pub fn bottom_right(&self) -> f32 {
        self.bottom_right
    }

    /// Get the bottom-left corner radius.
    pub fn bottom_left(&self) -> f32 {
        self.bottom_left
    }

    /// Check if any corner has a non-zero radius.
    pub fn has_rounded_corners(&self) -> bool {
        self.top_left > 0.0
            || self.top_right > 0.0
            || self.bottom_right > 0.0
            || self.bottom_left > 0.0
    }

    /// Convert to a simple ClipRect (ignoring rounded corners).
    ///
    /// Useful as a fast culling check before the more expensive
    /// rounded clip test.
    pub fn to_clip_rect(&self) -> ClipRect {
        ClipRect::new(self.rect)
    }

    /// Check if a point is inside the rounded clip.
    ///
    /// This is an approximation: it checks the bounding rectangle first,
    /// then refines for the corners.
    pub fn contains(&self, point: &Point) -> bool {
        // Fast reject: outside bounding rect
        if !self.rect.contains(point) {
            return false;
        }

        // If no rounded corners, it's inside
        if !self.has_rounded_corners() {
            return true;
        }

        // Check corner regions
        let rx = self.rect.x;
        let ry = self.rect.y;
        let rw = self.rect.width;
        let rh = self.rect.height;

        // Top-left corner
        if self.top_left > 0.0 {
            let cx = rx + self.top_left;
            let cy = ry + self.top_left;
            if point.x < cx && point.y < cy {
                let dx = point.x - cx;
                let dy = point.y - cy;
                if dx * dx + dy * dy > self.top_left * self.top_left {
                    return false;
                }
            }
        }

        // Top-right corner
        if self.top_right > 0.0 {
            let cx = rx + rw - self.top_right;
            let cy = ry + self.top_right;
            if point.x >= cx && point.y < cy {
                let dx = point.x - cx;
                let dy = point.y - cy;
                if dx * dx + dy * dy > self.top_right * self.top_right {
                    return false;
                }
            }
        }

        // Bottom-right corner
        if self.bottom_right > 0.0 {
            let cx = rx + rw - self.bottom_right;
            let cy = ry + rh - self.bottom_right;
            if point.x >= cx && point.y >= cy {
                let dx = point.x - cx;
                let dy = point.y - cy;
                if dx * dx + dy * dy > self.bottom_right * self.bottom_right {
                    return false;
                }
            }
        }

        // Bottom-left corner
        if self.bottom_left > 0.0 {
            let cx = rx + self.bottom_left;
            let cy = ry + rh - self.bottom_left;
            if point.x < cx && point.y >= cy {
                let dx = point.x - cx;
                let dy = point.y - cy;
                if dx * dx + dy * dy > self.bottom_left * self.bottom_left {
                    return false;
                }
            }
        }

        true
    }

    /// Check if this clip is empty.
    pub fn is_empty(&self) -> bool {
        self.rect.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Scissor rectangle
// ---------------------------------------------------------------------------

/// A GPU scissor rectangle in physical pixel coordinates.
///
/// The scissor test discards any fragments outside this rectangle.
/// Multiple scissor rectangles are intersected by the GPU.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScissorRect {
    /// X offset in physical pixels.
    pub x: i32,
    /// Y offset in physical pixels.
    pub y: i32,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
}

impl ScissorRect {
    /// Create a new scissor rectangle.
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Create from a ClipRect and scale factor.
    pub fn from_clip(clip: &ClipRect, scale_factor: f32) -> Self {
        let physical = clip.to_physical(scale_factor);
        let rect = physical.rect();
        Self {
            x: rect.x.floor() as i32,
            y: rect.y.floor() as i32,
            width: rect.width.ceil() as u32,
            height: rect.height.ceil() as u32,
        }
    }

    /// Create a full-screen scissor (no clipping).
    pub fn full_screen(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// Intersect with another scissor rectangle.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width as i32).min(other.x + other.width as i32);
        let bottom = (self.y + self.height as i32).min(other.y + other.height as i32);

        if right > x && bottom > y {
            Some(Self {
                x,
                y,
                width: (right - x) as u32,
                height: (bottom - y) as u32,
            })
        } else {
            None
        }
    }

    /// Check if this scissor rectangle is empty.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Check if a point (in physical pixels) is inside.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width as i32
            && y >= self.y
            && y < self.y + self.height as i32
    }
}

// ---------------------------------------------------------------------------
// Clip mode
// ---------------------------------------------------------------------------

/// The clipping mode for a render region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    /// No clipping.
    None,
    /// Simple rectangular scissor clipping.
    Scissor,
    /// Rounded corner clipping (requires shader/stencil).
    Rounded,
    /// Complex path-based clipping (future).
    Path,
}

impl ClipMode {
    /// Check if this mode can be handled by a simple scissor rectangle.
    pub fn is_scissor(&self) -> bool {
        matches!(self, Self::Scissor)
    }

    /// Check if this mode requires advanced clipping.
    pub fn needs_advanced(&self) -> bool {
        matches!(self, Self::Rounded | Self::Path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_rect_creation() {
        let clip = ClipRect::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(clip.contains(&Point::new(50.0, 50.0)));
        assert!(!clip.contains(&Point::new(150.0, 50.0)));
    }

    #[test]
    fn test_clip_rect_from_rect() {
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        let clip: ClipRect = rect.into();
        assert!((clip.rect().x - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_clip_rect_intersection() {
        let a = ClipRect::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let b = ClipRect::new(Rect::new(50.0, 50.0, 100.0, 100.0));
        let intersection = a.intersection(&b).unwrap();
        assert!((intersection.rect().x - 50.0).abs() < f32::EPSILON);
        assert!((intersection.rect().width - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_clip_rect_no_intersection() {
        let a = ClipRect::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let b = ClipRect::new(Rect::new(100.0, 100.0, 50.0, 50.0));
        assert!(a.intersection(&b).is_none());
    }

    #[test]
    fn test_clip_rect_to_physical() {
        let clip = ClipRect::new(Rect::new(10.5, 20.5, 100.5, 50.5));
        let physical = clip.to_physical(2.0);
        let rect = physical.rect();
        assert!(rect.x <= 21.0);
        assert!(rect.width >= 201.0);
    }

    #[test]
    fn test_clip_stack_push_pop() {
        let mut stack = ClipStack::new();
        assert!(stack.is_empty());

        stack.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(stack.len(), 1);

        stack.push_rect(Rect::new(50.0, 50.0, 100.0, 100.0));
        assert_eq!(stack.len(), 2);

        let effective = stack.effective_clip().unwrap();
        assert!((effective.rect().x - 50.0).abs() < f32::EPSILON);
        assert!((effective.rect().width - 50.0).abs() < f32::EPSILON);

        stack.pop();
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_clip_stack_no_intersection() {
        let mut stack = ClipStack::new();
        stack.push_rect(Rect::new(0.0, 0.0, 50.0, 50.0));
        stack.push_rect(Rect::new(100.0, 100.0, 50.0, 50.0));

        let effective = stack.effective_clip().unwrap();
        assert!(effective.is_empty());
    }

    #[test]
    fn test_clip_stack_contains_point() {
        let mut stack = ClipStack::new();
        stack.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0));

        assert!(stack.contains_point(&Point::new(50.0, 50.0)));
        assert!(!stack.contains_point(&Point::new(150.0, 50.0)));
    }

    #[test]
    fn test_clip_stack_clip_rect() {
        let mut stack = ClipStack::new();
        stack.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0));

        let rect = Rect::new(50.0, 50.0, 100.0, 100.0);
        let clipped = stack.clip_rect(&rect).unwrap();
        assert!((clipped.x - 50.0).abs() < f32::EPSILON);
        assert!((clipped.width - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_clip_stack_clear() {
        let mut stack = ClipStack::new();
        stack.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        stack.clear();
        assert!(stack.is_empty());
        assert!(stack.effective_clip().is_none());
    }

    #[test]
    fn test_rounded_clip_creation() {
        let clip = RoundedClip::new(Rect::new(0.0, 0.0, 100.0, 100.0), 10.0);
        assert!(clip.has_rounded_corners());
        assert!(clip.contains(&Point::new(50.0, 50.0)));
    }

    #[test]
    fn test_rounded_clip_corner() {
        let clip = RoundedClip::new(Rect::new(0.0, 0.0, 100.0, 100.0), 20.0);
        // Point in the corner cutout should be outside
        assert!(!clip.contains(&Point::new(5.0, 5.0)));
        // Point near center of corner should be inside
        assert!(clip.contains(&Point::new(15.0, 15.0)));
    }

    #[test]
    fn test_rounded_clip_to_clip_rect() {
        let clip = RoundedClip::new(Rect::new(10.0, 20.0, 100.0, 50.0), 10.0);
        let rect_clip = clip.to_clip_rect();
        assert!((rect_clip.rect().x - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_scissor_rect_from_clip() {
        let clip = ClipRect::new(Rect::new(10.0, 20.0, 100.0, 50.0));
        let scissor = ScissorRect::from_clip(&clip, 1.0);
        assert!(scissor.x >= 10);
        assert!(scissor.y >= 20);
    }

    #[test]
    fn test_scissor_rect_intersection() {
        let a = ScissorRect::new(0, 0, 100, 100);
        let b = ScissorRect::new(50, 50, 100, 100);
        let intersection = a.intersection(&b).unwrap();
        assert_eq!(intersection.x, 50);
        assert_eq!(intersection.y, 50);
        assert_eq!(intersection.width, 50);
        assert_eq!(intersection.height, 50);
    }

    #[test]
    fn test_scissor_rect_no_intersection() {
        let a = ScissorRect::new(0, 0, 50, 50);
        let b = ScissorRect::new(100, 100, 50, 50);
        assert!(a.intersection(&b).is_none());
    }

    #[test]
    fn test_scissor_rect_full_screen() {
        let scissor = ScissorRect::full_screen(800, 600);
        assert_eq!(scissor.x, 0);
        assert_eq!(scissor.y, 0);
        assert_eq!(scissor.width, 800);
        assert_eq!(scissor.height, 600);
    }

    #[test]
    fn test_scissor_rect_contains() {
        let scissor = ScissorRect::new(0, 0, 100, 100);
        assert!(scissor.contains(50, 50));
        assert!(!scissor.contains(150, 50));
    }

    #[test]
    fn test_clip_mode() {
        assert!(!ClipMode::None.is_scissor());
        assert!(ClipMode::Scissor.is_scissor());
        assert!(ClipMode::Rounded.needs_advanced());
        assert!(ClipMode::Path.needs_advanced());
    }

    #[test]
    fn test_clip_rect_translate() {
        let clip = ClipRect::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        let translated = clip.translate(50.0, 50.0);
        assert!((translated.rect().x - 50.0).abs() < f32::EPSILON);
        assert!((translated.rect().y - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_clip_rect_inflate() {
        let clip = ClipRect::new(Rect::new(10.0, 10.0, 80.0, 80.0));
        let inflated = clip.inflate(10.0, 10.0);
        assert!((inflated.rect().x - 0.0).abs() < f32::EPSILON);
        assert!((inflated.rect().width - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_clip_stack_iter() {
        let mut stack = ClipStack::new();
        stack.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        stack.push_rect(Rect::new(50.0, 50.0, 100.0, 100.0));

        let clips: Vec<ClipRect> = stack.iter().collect();
        assert_eq!(clips.len(), 2);
    }

    #[test]
    fn test_clip_stack_top() {
        let mut stack = ClipStack::new();
        assert!(stack.top().is_none());

        stack.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
        let top = stack.top().unwrap();
        assert!((top.rect().width - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rounded_clip_per_corner() {
        let clip =
            RoundedClip::with_corners(Rect::new(0.0, 0.0, 100.0, 100.0), 10.0, 20.0, 30.0, 40.0);
        assert!((clip.top_left() - 10.0).abs() < f32::EPSILON);
        assert!((clip.top_right() - 20.0).abs() < f32::EPSILON);
        assert!((clip.bottom_right() - 30.0).abs() < f32::EPSILON);
        assert!((clip.bottom_left() - 40.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rounded_clip_empty() {
        let clip = RoundedClip::new(Rect::ZERO, 10.0);
        assert!(clip.is_empty());
    }

    #[test]
    fn test_clip_rect_contains_rect() {
        let clip = ClipRect::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(clip.contains_rect(&Rect::new(10.0, 10.0, 80.0, 80.0)));
        assert!(!clip.contains_rect(&Rect::new(50.0, 50.0, 100.0, 100.0)));
    }

    #[test]
    fn test_clip_stack_intersects_rect() {
        let mut stack = ClipStack::new();
        stack.push_rect(Rect::new(0.0, 0.0, 100.0, 100.0));

        assert!(stack.intersects_rect(&Rect::new(50.0, 50.0, 100.0, 100.0)));
        assert!(!stack.intersects_rect(&Rect::new(200.0, 200.0, 100.0, 100.0)));
    }

    #[test]
    fn test_clip_rect_from_point() {
        let clip: ClipRect = Point::new(10.0, 20.0).into();
        assert!((clip.rect().x - 10.0).abs() < f32::EPSILON);
        assert!((clip.rect().y - 20.0).abs() < f32::EPSILON);
        assert!(clip.is_empty());
    }
}
