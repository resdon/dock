//! Geometry morphing and window animation.
//!
//! This module provides types for animating window geometry: expanding,
//! contracting, and morphing between different shapes and sizes.
//!
//! ## Morphing
//!
//! For expanding and contracting windows, the geometry itself is animated:
//!
//! ```text
//! R(t) = R0 + (R1 - R0) * E(t)
//! ```
//!
//! Where `E(t)` is an easing function. Useful easing functions include:
//! - Linear (constant speed)
//! - Ease-in (slow start, fast end)
//! - Ease-out (fast start, slow end)
//! - Ease-in-out (slow start and end)
//! - Cubic Bézier (custom curves)
//!
//! ## Key Distinction
//!
//! Animating the visual geometry of a window is **not** the same as asking
//! the Wayland compositor to resize the actual window. The compositor controls
//! actual toplevel and popup surfaces. The renderer controls the pixels inside
//! them.
//!
//! For popups, use a separate logical scene or child node hierarchy depending
//! on whether you need an actual Wayland popup or merely a visual overlay.

use crate::animation::Easing;
use crate::geometry::{Point, Rect, Size};

// ---------------------------------------------------------------------------
// Geometry animation
// ---------------------------------------------------------------------------

/// Animates a rectangle from one state to another over time.
///
/// Interpolates position, width, and height using an easing function:
///
/// ```text
/// R(t) = R0 + (R1 - R0) * E(t)
/// ```
///
/// # Example
///
/// ```
/// use wgpu_api::{GeometryAnimation, Rect, Easing};
///
/// let anim = GeometryAnimation::new(
///     Rect::new(0.0, 0.0, 100.0, 100.0),
///     Rect::new(0.0, 0.0, 200.0, 200.0),
///     1.0,
///     Easing::Linear,
/// );
///
/// // At 50% with linear easing, width should be 150
/// let rect = anim.evaluate_at(0.5);
/// assert!((rect.width - 150.0).abs() < f32::EPSILON);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryAnimation {
    /// Starting rectangle.
    from: Rect,
    /// Target rectangle.
    to: Rect,
    /// Total animation duration in seconds.
    duration: f32,
    /// Elapsed time in seconds.
    elapsed: f32,
    /// Easing function for interpolation.
    easing: Easing,
    /// Whether the animation is complete.
    complete: bool,
}

impl GeometryAnimation {
    /// Create a new geometry animation.
    pub fn new(from: Rect, to: Rect, duration: f32, easing: Easing) -> Self {
        Self {
            from,
            to,
            duration: duration.max(0.001),
            elapsed: 0.0,
            easing,
            complete: false,
        }
    }

    /// Advance the animation by the given delta time.
    ///
    /// Returns `true` if the animation is now complete.
    pub fn advance(&mut self, delta: f32) -> bool {
        if self.complete {
            return true;
        }

        self.elapsed += delta;
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.complete = true;
        }

        self.complete
    }

    /// Evaluate the animation at the current elapsed time.
    ///
    /// Returns the interpolated rectangle.
    pub fn evaluate(&self) -> Rect {
        self.evaluate_at(self.elapsed / self.duration)
    }

    /// Evaluate the animation at a specific time.
    pub fn evaluate_at(&self, time: f32) -> Rect {
        let t = (time / self.duration).clamp(0.0, 1.0);
        let e = self.easing.compute(t);
        self.interpolate(e)
    }

    /// Interpolate between `from` and `to` rectangles by the given factor (0.0–1.0).
    pub fn interpolate(&self, t: f32) -> Rect {
        let t = t.clamp(0.0, 1.0);
        Rect::new(
            self.from.x + (self.to.x - self.from.x) * t,
            self.from.y + (self.to.y - self.from.y) * t,
            self.from.width + (self.to.width - self.from.width) * t,
            self.from.height + (self.to.height - self.from.height) * t,
        )
    }

    /// Get the starting rectangle.
    pub fn from_rect(&self) -> Rect {
        self.from
    }

    /// Get the target rectangle.
    pub fn to_rect(&self) -> Rect {
        self.to
    }

    /// Get the animation duration.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// Get the elapsed time.
    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// Get the progress (0.0–1.0).
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    /// Get the easing function.
    pub fn easing(&self) -> Easing {
        self.easing
    }

    /// Check if the animation is complete.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Reset the animation to the beginning.
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
        self.complete = false;
    }

    /// Reverse the animation (swap from and to).
    pub fn reverse(&mut self) {
        std::mem::swap(&mut self.from, &mut self.to);
        self.reset();
    }

    /// Set the target rectangle and reset.
    pub fn set_to(&mut self, to: Rect) {
        self.to = to;
        self.reset();
    }

    /// Set the easing function.
    pub fn set_easing(&mut self, easing: Easing) {
        self.easing = easing;
    }
}

// ---------------------------------------------------------------------------
// Size animation
// ---------------------------------------------------------------------------

/// Animates a size change (width and height only).
///
/// Useful for animating window resize without position change.
#[derive(Debug, Clone, PartialEq)]
pub struct SizeAnimation {
    /// Starting size.
    from: Size,
    /// Target size.
    to: Size,
    /// Duration in seconds.
    duration: f32,
    /// Elapsed time.
    elapsed: f32,
    /// Easing function.
    easing: Easing,
    /// Whether complete.
    complete: bool,
}

impl SizeAnimation {
    /// Create a new size animation.
    pub fn new(from: Size, to: Size, duration: f32, easing: Easing) -> Self {
        Self {
            from,
            to,
            duration: duration.max(0.001),
            elapsed: 0.0,
            easing,
            complete: false,
        }
    }

    /// Advance by delta time.
    pub fn advance(&mut self, delta: f32) -> bool {
        if self.complete {
            return true;
        }
        self.elapsed += delta;
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.complete = true;
        }
        self.complete
    }

    /// Evaluate the current size.
    pub fn evaluate(&self) -> Size {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let e = self.easing.compute(t);
        Size::new(
            self.from.width + (self.to.width - self.from.width) * e,
            self.from.height + (self.to.height - self.from.height) * e,
        )
    }

    /// Get the progress (0.0–1.0).
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    /// Check if complete.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Reset to the beginning.
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
        self.complete = false;
    }
}

// ---------------------------------------------------------------------------
// Position animation
// ---------------------------------------------------------------------------

/// Animates a position change (x and y only).
#[derive(Debug, Clone, PartialEq)]
pub struct PositionAnimation {
    /// Starting position.
    from: Point,
    /// Target position.
    to: Point,
    /// Duration in seconds.
    duration: f32,
    /// Elapsed time.
    elapsed: f32,
    /// Easing function.
    easing: Easing,
    /// Whether complete.
    complete: bool,
}

impl PositionAnimation {
    /// Create a new position animation.
    pub fn new(from: Point, to: Point, duration: f32, easing: Easing) -> Self {
        Self {
            from,
            to,
            duration: duration.max(0.001),
            elapsed: 0.0,
            easing,
            complete: false,
        }
    }

    /// Advance by delta time.
    pub fn advance(&mut self, delta: f32) -> bool {
        if self.complete {
            return true;
        }
        self.elapsed += delta;
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.complete = true;
        }
        self.complete
    }

    /// Evaluate the current position.
    pub fn evaluate(&self) -> Point {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let e = self.easing.compute(t);
        Point::new(
            self.from.x + (self.to.x - self.from.x) * e,
            self.from.y + (self.to.y - self.from.y) * e,
        )
    }

    /// Get the progress (0.0–1.0).
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    /// Check if complete.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Reset to the beginning.
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
        self.complete = false;
    }
}

// ---------------------------------------------------------------------------
// Morph type
// ---------------------------------------------------------------------------

/// The type of morphing transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorphType {
    /// Expand from a point to a rectangle.
    Expand,
    /// Contract from a rectangle to a point.
    Contract,
    /// Resize from one rectangle to another.
    Resize,
    /// Slide from one position to another.
    Slide,
    /// Fade in (opacity 0 → 1 with geometry).
    FadeIn,
    /// Fade out (opacity 1 → 0 with geometry).
    FadeOut,
}

impl MorphType {
    /// Get the default easing for this morph type.
    pub fn default_easing(&self) -> Easing {
        match self {
            Self::Expand => Easing::EaseOut,
            Self::Contract => Easing::EaseIn,
            Self::Resize => Easing::EaseInOut,
            Self::Slide => Easing::EaseInOut,
            Self::FadeIn => Easing::EaseOut,
            Self::FadeOut => Easing::EaseIn,
        }
    }

    /// Get the default duration for this morph type.
    pub fn default_duration(&self) -> f32 {
        match self {
            Self::Expand | Self::Contract => 0.25,
            Self::Resize => 0.3,
            Self::Slide => 0.2,
            Self::FadeIn | Self::FadeOut => 0.15,
        }
    }
}

// ---------------------------------------------------------------------------
// Popup animation
// ---------------------------------------------------------------------------

/// Animation for popup appearance and disappearance.
///
/// A popup can be an actual Wayland popup surface or merely a visual overlay.
/// Use a separate logical scene or child node hierarchy for the popup content.
#[derive(Debug, Clone, PartialEq)]
pub struct PopupAnimation {
    /// The anchor rectangle (where the popup appears relative to).
    anchor: Rect,
    /// The popup's target rectangle.
    target: Rect,
    /// Current opacity (0.0–1.0).
    opacity: f32,
    /// Target opacity.
    target_opacity: f32,
    /// Duration in seconds.
    duration: f32,
    /// Elapsed time.
    elapsed: f32,
    /// Easing function.
    easing: Easing,
    /// Whether the popup is appearing or disappearing.
    appearing: bool,
    /// Whether the animation is complete.
    complete: bool,
}

impl PopupAnimation {
    /// Create a new popup animation (appearing).
    pub fn new(anchor: Rect, target: Rect, duration: f32) -> Self {
        Self {
            anchor,
            target,
            opacity: 0.0,
            target_opacity: 1.0,
            duration: duration.max(0.001),
            elapsed: 0.0,
            easing: Easing::EaseOut,
            appearing: true,
            complete: false,
        }
    }

    /// Create a disappearing popup animation.
    pub fn disappearing(anchor: Rect, target: Rect, duration: f32) -> Self {
        Self {
            anchor,
            target,
            opacity: 1.0,
            target_opacity: 0.0,
            duration: duration.max(0.001),
            elapsed: 0.0,
            easing: Easing::EaseIn,
            appearing: false,
            complete: false,
        }
    }

    /// Advance by delta time.
    pub fn advance(&mut self, delta: f32) -> bool {
        if self.complete {
            return true;
        }
        self.elapsed += delta;
        if self.elapsed >= self.duration {
            self.elapsed = self.duration;
            self.complete = true;
        }
        self.complete
    }

    /// Evaluate the current state.
    ///
    /// Returns the current rectangle and opacity.
    pub fn evaluate(&self) -> (Rect, f32) {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let e = self.easing.compute(t);

        // For appearing popups, start from the anchor and expand to target
        let from_rect = if self.appearing {
            self.anchor
        } else {
            self.target
        };
        let to_rect = if self.appearing {
            self.target
        } else {
            self.anchor
        };

        let rect = Rect::new(
            from_rect.x + (to_rect.x - from_rect.x) * e,
            from_rect.y + (to_rect.y - from_rect.y) * e,
            from_rect.width + (to_rect.width - from_rect.width) * e,
            from_rect.height + (to_rect.height - from_rect.height) * e,
        );

        let opacity = self.opacity + (self.target_opacity - self.opacity) * e;

        (rect, opacity.clamp(0.0, 1.0))
    }

    /// Get the current opacity.
    pub fn opacity(&self) -> f32 {
        let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let e = self.easing.compute(t);
        (self.opacity + (self.target_opacity - self.opacity) * e).clamp(0.0, 1.0)
    }

    /// Get the progress (0.0–1.0).
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    /// Check if complete.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Check if the popup is appearing.
    pub fn is_appearing(&self) -> bool {
        self.appearing
    }

    /// Reset the animation.
    pub fn reset(&mut self) {
        self.elapsed = 0.0;
        self.complete = false;
    }
}

// ---------------------------------------------------------------------------
// Morph manager
// ---------------------------------------------------------------------------

/// Manages multiple active geometry animations.
///
/// Tracks which nodes are being animated and updates only those,
/// similar to how AnimationManager handles property animations.
#[derive(Debug)]
pub struct MorphManager {
    /// Active geometry animations keyed by node ID.
    animations: std::collections::HashMap<u64, GeometryAnimation>,
    /// Active size animations.
    size_animations: std::collections::HashMap<u64, SizeAnimation>,
    /// Active popup animations.
    popup_animations: std::collections::HashMap<u64, PopupAnimation>,
}

impl MorphManager {
    /// Create a new morph manager.
    pub fn new() -> Self {
        Self {
            animations: std::collections::HashMap::new(),
            size_animations: std::collections::HashMap::new(),
            popup_animations: std::collections::HashMap::new(),
        }
    }

    /// Start a geometry animation for a node.
    pub fn animate_geometry(
        &mut self,
        node_id: u64,
        from: Rect,
        to: Rect,
        duration: f32,
        easing: Easing,
    ) {
        self.animations
            .insert(node_id, GeometryAnimation::new(from, to, duration, easing));
    }

    /// Start a size animation for a node.
    pub fn animate_size(
        &mut self,
        node_id: u64,
        from: Size,
        to: Size,
        duration: f32,
        easing: Easing,
    ) {
        self.size_animations
            .insert(node_id, SizeAnimation::new(from, to, duration, easing));
    }

    /// Start a popup animation.
    pub fn animate_popup(
        &mut self,
        node_id: u64,
        anchor: Rect,
        target: Rect,
        duration: f32,
        appearing: bool,
    ) {
        let anim = if appearing {
            PopupAnimation::new(anchor, target, duration)
        } else {
            PopupAnimation::disappearing(anchor, target, duration)
        };
        self.popup_animations.insert(node_id, anim);
    }

    /// Advance all animations by the given delta time.
    ///
    /// Returns the number of animations that completed this frame.
    pub fn advance(&mut self, delta: f32) -> usize {
        let mut completed = 0;

        // Advance geometry animations
        self.animations.retain(|_id, anim| {
            if anim.advance(delta) {
                completed += 1;
                false
            } else {
                true
            }
        });

        // Advance size animations
        self.size_animations.retain(|_id, anim| {
            if anim.advance(delta) {
                completed += 1;
                false
            } else {
                true
            }
        });

        // Advance popup animations
        self.popup_animations.retain(|_id, anim| {
            if anim.advance(delta) {
                completed += 1;
                false
            } else {
                true
            }
        });

        completed
    }

    /// Get the current rectangle for a node's geometry animation.
    pub fn get_geometry(&self, node_id: u64) -> Option<Rect> {
        self.animations.get(&node_id).map(|a| a.evaluate())
    }

    /// Get the current size for a node's size animation.
    pub fn get_size(&self, node_id: u64) -> Option<Size> {
        self.size_animations.get(&node_id).map(|a| a.evaluate())
    }

    /// Get the current popup state (rect, opacity).
    pub fn get_popup(&self, node_id: u64) -> Option<(Rect, f32)> {
        self.popup_animations.get(&node_id).map(|a| a.evaluate())
    }

    /// Check if a node has an active geometry animation.
    pub fn has_geometry(&self, node_id: u64) -> bool {
        self.animations.contains_key(&node_id)
    }

    /// Check if a node has an active size animation.
    pub fn has_size(&self, node_id: u64) -> bool {
        self.size_animations.contains_key(&node_id)
    }

    /// Check if a node has an active popup animation.
    pub fn has_popup(&self, node_id: u64) -> bool {
        self.popup_animations.contains_key(&node_id)
    }

    /// Stop an animation for a node.
    pub fn stop(&mut self, node_id: u64) {
        self.animations.remove(&node_id);
        self.size_animations.remove(&node_id);
        self.popup_animations.remove(&node_id);
    }

    /// Stop all animations.
    pub fn stop_all(&mut self) {
        self.animations.clear();
        self.size_animations.clear();
        self.popup_animations.clear();
    }

    /// Get the number of active animations.
    pub fn active_count(&self) -> usize {
        self.animations.len() + self.size_animations.len() + self.popup_animations.len()
    }

    /// Check if there are any active animations.
    pub fn is_idle(&self) -> bool {
        self.active_count() == 0
    }
}

impl Default for MorphManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geometry_animation_creation() {
        let anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(50.0, 50.0, 200.0, 300.0),
            1.0,
            Easing::Linear,
        );
        assert!(!anim.is_complete());
        assert!((anim.progress() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_geometry_animation_linear() {
        let anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        // At 50% with linear easing, width should be 150
        let rect = anim.evaluate_at(0.5);
        assert!((rect.width - 150.0).abs() < f32::EPSILON);
        assert!((rect.height - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_geometry_animation_advance() {
        let mut anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        anim.advance(0.5);
        assert!(!anim.is_complete());
        assert!((anim.progress() - 0.5).abs() < f32::EPSILON);

        anim.advance(0.5);
        assert!(anim.is_complete());
    }

    #[test]
    fn test_geometry_animation_ease_out() {
        let anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::EaseOut,
        );
        // EaseOut at 50% gives ~0.75, so width ≈ 175
        let rect = anim.evaluate_at(0.5);
        assert!(rect.width > 150.0);
        assert!(rect.width < 200.0);
    }

    #[test]
    fn test_geometry_animation_reset() {
        let mut anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        anim.advance(1.0);
        assert!(anim.is_complete());
        anim.reset();
        assert!(!anim.is_complete());
        assert!((anim.progress() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_geometry_animation_reverse() {
        let mut anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        anim.reverse();
        assert!((anim.from_rect().width - 200.0).abs() < f32::EPSILON);
        assert!((anim.to_rect().width - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_size_animation() {
        let mut anim = SizeAnimation::new(
            Size::new(100.0, 100.0),
            Size::new(200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        anim.advance(0.5);
        let size = anim.evaluate();
        assert!((size.width - 150.0).abs() < f32::EPSILON);
        assert!((size.height - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_position_animation() {
        let mut anim = PositionAnimation::new(
            Point::new(0.0, 0.0),
            Point::new(100.0, 100.0),
            1.0,
            Easing::Linear,
        );
        anim.advance(0.5);
        let point = anim.evaluate();
        assert!((point.x - 50.0).abs() < f32::EPSILON);
        assert!((point.y - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_morph_type_defaults() {
        assert_eq!(MorphType::Expand.default_easing(), Easing::EaseOut);
        assert_eq!(MorphType::Contract.default_easing(), Easing::EaseIn);
        assert_eq!(MorphType::Resize.default_easing(), Easing::EaseInOut);
        assert!((MorphType::Expand.default_duration() - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn test_popup_animation_appearing() {
        let mut anim = PopupAnimation::new(
            Rect::new(50.0, 50.0, 0.0, 0.0),
            Rect::new(50.0, 50.0, 200.0, 150.0),
            1.0,
        );
        anim.advance(0.5);
        let (rect, opacity) = anim.evaluate();
        assert!(opacity > 0.0 && opacity < 1.0);
        assert!(rect.width > 0.0 && rect.width < 200.0);
    }

    #[test]
    fn test_popup_animation_disappearing() {
        let mut anim = PopupAnimation::disappearing(
            Rect::new(50.0, 50.0, 0.0, 0.0),
            Rect::new(50.0, 50.0, 200.0, 150.0),
            1.0,
        );
        assert!(!anim.is_appearing());
        anim.advance(0.5);
        let (_rect, opacity) = anim.evaluate();
        assert!(opacity < 1.0 && opacity > 0.0);
    }

    #[test]
    fn test_morph_manager() {
        let mut manager = MorphManager::new();
        manager.animate_geometry(
            1,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        assert_eq!(manager.active_count(), 1);
        assert!(manager.has_geometry(1));

        manager.advance(0.5);
        let rect = manager.get_geometry(1).unwrap();
        assert!((rect.width - 150.0).abs() < f32::EPSILON);

        manager.advance(0.5);
        assert!(manager.is_idle());
    }

    #[test]
    fn test_morph_manager_stop() {
        let mut manager = MorphManager::new();
        manager.animate_geometry(
            1,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        manager.stop(1);
        assert!(manager.is_idle());
    }

    #[test]
    fn test_morph_manager_stop_all() {
        let mut manager = MorphManager::new();
        manager.animate_geometry(
            1,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        manager.animate_size(
            2,
            Size::new(100.0, 100.0),
            Size::new(200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        assert_eq!(manager.active_count(), 2);
        manager.stop_all();
        assert!(manager.is_idle());
    }

    #[test]
    fn test_geometry_animation_interpolate() {
        let anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(50.0, 50.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        let rect = anim.interpolate(0.0);
        assert!((rect.x - 0.0).abs() < f32::EPSILON);
        assert!((rect.width - 100.0).abs() < f32::EPSILON);

        let rect = anim.interpolate(1.0);
        assert!((rect.x - 50.0).abs() < f32::EPSILON);
        assert!((rect.width - 200.0).abs() < f32::EPSILON);

        let rect = anim.interpolate(0.5);
        assert!((rect.x - 25.0).abs() < f32::EPSILON);
        assert!((rect.width - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_geometry_animation_set_to() {
        let mut anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        anim.set_to(Rect::new(0.0, 0.0, 300.0, 300.0));
        let rect = anim.evaluate_at(1.0);
        assert!((rect.width - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_geometry_animation_easing() {
        let mut anim = GeometryAnimation::new(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Rect::new(0.0, 0.0, 200.0, 200.0),
            1.0,
            Easing::Linear,
        );
        anim.set_easing(Easing::EaseOut);
        assert_eq!(anim.easing(), Easing::EaseOut);
    }
}
