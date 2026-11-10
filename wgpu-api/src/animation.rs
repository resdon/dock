//! Animation system for property interpolation.
//!
//! Animations interpolate between values over time using easing functions.
//! They can be applied to any numeric property (position, scale, opacity, etc.).

use crate::color::Color;
use crate::geometry::{Point, Size, Vector2D};
use crate::transform::Transform;

/// Opaque identifier for an active animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimationId(u64);

impl AnimationId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Current state of an animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationState {
    /// Animation is running.
    Running,
    /// Animation is paused.
    Paused,
    /// Animation has completed.
    Completed,
    /// Animation was cancelled.
    Cancelled,
}

/// Easing function for animation interpolation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    /// Linear interpolation (constant speed).
    Linear,
    /// Ease in (slow start, fast end).
    EaseIn,
    /// Ease out (fast start, slow end).
    EaseOut,
    /// Ease in-out (slow start and end).
    EaseInOut,
    /// Back easing (overshoots then returns).
    Back,
    /// Bounce easing (bounces at the end).
    Bounce,
    /// Elastic easing (oscillates).
    Elastic,
    /// Custom cubic bezier easing.
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    /// Compute the eased value for a given progress (0.0–1.0).
    pub fn compute(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => t * (2.0 - t),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Self::Back => {
                let s = 1.70158;
                t * t * ((s + 1.0) * t - s)
            }
            Self::Bounce => {
                if t < 1.0 / 2.75 {
                    7.5625 * t * t
                } else if t < 2.0 / 2.75 {
                    let t2 = t - 1.5 / 2.75;
                    7.5625 * t2 * t2 + 0.75
                } else if t < 2.5 / 2.75 {
                    let t2 = t - 2.25 / 2.75;
                    7.5625 * t2 * t2 + 0.9375
                } else {
                    let t2 = t - 2.625 / 2.75;
                    7.5625 * t2 * t2 + 0.984375
                }
            }
            Self::Elastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    t * t * t * ((1.70158 + 1.0) * t - 1.70158)
                }
            }
            Self::CubicBezier(_x1, y1, _x2, y2) => {
                // Approximate cubic bezier using de Casteljau's algorithm
                let one_minus_t = 1.0 - t;
                one_minus_t * one_minus_t * one_minus_t * y1
                    + 3.0 * one_minus_t * one_minus_t * t * y2
                    + 3.0 * one_minus_t * t * t * y2
                    + t * t * t
            }
        }
    }
}

/// Type of animation property.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationType {
    /// Animate a single float value.
    Float { start: f32, end: f32 },
    /// Animate a point (position).
    Point { start: Point, end: Point },
    /// Animate a size (dimensions).
    Size { start: Size, end: Size },
    /// Animate a color.
    Color { start: Color, end: Color },
    /// Animate a transform matrix.
    Transform { start: Transform, end: Transform },
    /// Animate a vector.
    Vector { start: Vector2D, end: Vector2D },
}

/// Current interpolated value of an animation.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationValue {
    Float(f32),
    Point(Point),
    Size(Size),
    Color(Color),
    Transform(Transform),
    Vector(Vector2D),
}

impl AnimationValue {
    /// Get the float value (panics if not Float).
    pub fn as_float(&self) -> f32 {
        match self {
            Self::Float(v) => *v,
            _ => panic!("AnimationValue is not a float"),
        }
    }

    /// Get the point value (panics if not Point).
    pub fn as_point(&self) -> Point {
        match self {
            Self::Point(p) => *p,
            _ => panic!("AnimationValue is not a point"),
        }
    }

    /// Get the color value (panics if not Color).
    pub fn as_color(&self) -> Color {
        match self {
            Self::Color(c) => *c,
            _ => panic!("AnimationValue is not a color"),
        }
    }
}

/// Repetition behavior for animations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationRepeat {
    /// Run once and stop.
    Once,
    /// Repeat indefinitely.
    Infinite,
    /// Repeat a specific number of times.
    Count(usize),
}

impl Default for AnimationRepeat {
    fn default() -> Self {
        Self::Once
    }
}

/// An animation definition.
///
/// Describes how a property interpolates from a start value to an end value
/// over a given duration with an easing function.
#[derive(Debug, Clone)]
pub struct Animation {
    /// Type of animation (defines start/end values).
    pub animation_type: AnimationType,
    /// Duration in seconds.
    pub duration: f32,
    /// Easing function.
    pub easing: Easing,
    /// Delay before starting (in seconds).
    pub delay: f32,
    /// Repeat behavior.
    pub repeat: AnimationRepeat,
    /// Whether to reverse on alternate iterations.
    pub alternate: bool,
    /// Current progress (0.0–1.0).
    pub progress: f32,
    /// Current state.
    pub state: AnimationState,
    /// Number of completed iterations.
    pub iterations: usize,
}

impl Animation {
    /// Create a new float animation.
    pub fn float(start: f32, end: f32, duration: f32) -> Self {
        Self {
            animation_type: AnimationType::Float { start, end },
            duration,
            easing: Easing::EaseOut,
            delay: 0.0,
            repeat: AnimationRepeat::default(),
            alternate: false,
            progress: 0.0,
            state: AnimationState::Running,
            iterations: 0,
        }
    }

    /// Create a new point animation.
    pub fn point(start: Point, end: Point, duration: f32) -> Self {
        Self {
            animation_type: AnimationType::Point { start, end },
            duration,
            easing: Easing::EaseOut,
            delay: 0.0,
            repeat: AnimationRepeat::default(),
            alternate: false,
            progress: 0.0,
            state: AnimationState::Running,
            iterations: 0,
        }
    }

    /// Create a new color animation.
    pub fn color(start: Color, end: Color, duration: f32) -> Self {
        Self {
            animation_type: AnimationType::Color { start, end },
            duration,
            easing: Easing::EaseOut,
            delay: 0.0,
            repeat: AnimationRepeat::default(),
            alternate: false,
            progress: 0.0,
            state: AnimationState::Running,
            iterations: 0,
        }
    }

    /// Set the easing function.
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Set the delay.
    pub fn with_delay(mut self, delay: f32) -> Self {
        self.delay = delay;
        self
    }

    /// Set the repeat behavior.
    pub fn with_repeat(mut self, repeat: AnimationRepeat) -> Self {
        self.repeat = repeat;
        self
    }

    /// Enable alternate direction on repeat.
    pub fn with_alternate(mut self) -> Self {
        self.alternate = true;
        self
    }

    /// Update the animation by a time delta (in seconds).
    /// Returns the current interpolated value.
    pub fn update(&mut self, dt: f32) -> Option<AnimationValue> {
        if self.state != AnimationState::Running {
            return None;
        }

        if self.delay > 0.0 {
            self.delay -= dt;
            if self.delay > 0.0 {
                return self.sample(0.0);
            }
        }

        self.progress += dt / self.duration;

        if self.progress >= 1.0 {
            // Check repeat
            match self.repeat {
                AnimationRepeat::Once => {
                    self.progress = 1.0;
                    self.state = AnimationState::Completed;
                }
                AnimationRepeat::Infinite => {
                    self.progress = self.progress.fract();
                    self.iterations += 1;
                }
                AnimationRepeat::Count(count) => {
                    if self.iterations + 1 >= count {
                        self.progress = 1.0;
                        self.state = AnimationState::Completed;
                    } else {
                        self.progress = self.progress.fract();
                        self.iterations += 1;
                    }
                }
            }
        }

        self.sample(self.progress)
    }

    /// Sample the animation at a given progress (0.0–1.0).
    pub fn sample(&self, progress: f32) -> Option<AnimationValue> {
        let t = self.easing.compute(progress);

        match &self.animation_type {
            AnimationType::Float { start, end } => {
                Some(AnimationValue::Float(start + (end - start) * t))
            }
            AnimationType::Point { start, end } => {
                Some(AnimationValue::Point(Point::lerp(start, end, t)))
            }
            AnimationType::Size { start, end } => {
                let w = start.width + (end.width - start.width) * t;
                let h = start.height + (end.height - start.height) * t;
                Some(AnimationValue::Size(Size::new(w, h)))
            }
            AnimationType::Color { start, end } => {
                Some(AnimationValue::Color(Color::lerp(start, end, t)))
            }
            AnimationType::Transform { start, end } => {
                // Interpolate matrix elements
                let mut elements = [0.0f32; 9];
                for i in 0..9 {
                    elements[i] = start.matrix.elements[i]
                        + (end.matrix.elements[i] - start.matrix.elements[i]) * t;
                }
                Some(AnimationValue::Transform(Transform {
                    matrix: crate::transform::Matrix3::from_elements(elements),
                }))
            }
            AnimationType::Vector { start, end } => {
                Some(AnimationValue::Vector(Vector2D::lerp(start, end, t)))
            }
        }
    }

    /// Get the current value without updating.
    pub fn current_value(&self) -> Option<AnimationValue> {
        self.sample(self.progress)
    }

    /// Pause the animation.
    pub fn pause(&mut self) {
        self.state = AnimationState::Paused;
    }

    /// Resume a paused animation.
    pub fn resume(&mut self) {
        if self.state == AnimationState::Paused {
            self.state = AnimationState::Running;
        }
    }

    /// Cancel the animation.
    pub fn cancel(&mut self) {
        self.state = AnimationState::Cancelled;
    }

    /// Reset the animation to the beginning.
    pub fn reset(&mut self) {
        self.progress = 0.0;
        self.state = AnimationState::Running;
        self.iterations = 0;
        self.delay = 0.0;
    }

    /// Check if the animation is complete.
    pub fn is_complete(&self) -> bool {
        self.state == AnimationState::Completed
    }
}

// ---------------------------------------------------------------------------
// Continuous animations for icons and dynamic geometry
// ---------------------------------------------------------------------------

/// A continuous pulse animation for scaling icons.
///
/// Produces a scale factor oscillating around 1.0 using a sine wave:
/// `s(t) = 1 + A * sin(2π * f * t)`
///
/// Where `A` is the amplitude and `f` is the frequency in Hz.
/// For a subtle pulse between ~92% and ~108% scale:
///
/// ```no_run
/// use wgpu_api::PulseAnimation;
/// let pulse = PulseAnimation {
///     amplitude: 0.08,
///     frequency: 0.5,
///     elapsed: 0.0,
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct PulseAnimation {
    /// Amplitude of the pulse (0.0–1.0). Typical: 0.05–0.15.
    pub amplitude: f32,
    /// Frequency in Hz (cycles per second). Typical: 0.2–2.0.
    pub frequency: f32,
    /// Elapsed time in seconds. Update this each frame.
    pub elapsed: f32,
}

impl PulseAnimation {
    /// Create a new pulse animation.
    pub const fn new(amplitude: f32, frequency: f32) -> Self {
        Self {
            amplitude,
            frequency,
            elapsed: 0.0,
        }
    }

    /// Evaluate the current scale factor.
    ///
    /// Returns a value oscillating around 1.0. Use this as the node's
    /// uniform scale. Scale around the icon's center by setting
    /// `anchor` to `(0.5, 0.5)` in the node's style.
    pub fn evaluate(&self) -> f32 {
        1.0 + self.amplitude * (self.elapsed * self.frequency * std::f32::consts::TAU).sin()
    }

    /// Advance the animation by a time delta (in seconds).
    pub fn update(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    /// Get the minimum scale value.
    pub fn min_scale(&self) -> f32 {
        1.0 - self.amplitude
    }

    /// Get the maximum scale value.
    pub fn max_scale(&self) -> f32 {
        1.0 + self.amplitude
    }
}

impl Default for PulseAnimation {
    fn default() -> Self {
        Self {
            amplitude: 0.08,
            frequency: 0.5,
            elapsed: 0.0,
        }
    }
}

/// A continuous rotation animation.
///
/// Rotates a node at a constant angular velocity.
/// Useful for loading spinners, compass needles, etc.
#[derive(Debug, Clone, Copy)]
pub struct RotateAnimation {
    /// Angular velocity in degrees per second.
    pub speed: f32,
    /// Elapsed time in seconds.
    pub elapsed: f32,
    /// Initial rotation offset in degrees.
    pub offset: f32,
}

impl RotateAnimation {
    /// Create a new rotation animation.
    pub const fn new(speed_deg_per_sec: f32) -> Self {
        Self {
            speed: speed_deg_per_sec,
            elapsed: 0.0,
            offset: 0.0,
        }
    }

    /// Evaluate the current rotation in degrees.
    pub fn evaluate(&self) -> f32 {
        self.offset + self.speed * self.elapsed
    }

    /// Advance the animation by a time delta.
    pub fn update(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    /// Set the initial rotation offset.
    pub fn with_offset(mut self, offset_deg: f32) -> Self {
        self.offset = offset_deg;
        self
    }
}

impl Default for RotateAnimation {
    fn default() -> Self {
        Self {
            speed: 360.0,
            elapsed: 0.0,
            offset: 0.0,
        }
    }
}

/// A continuous opacity fade animation.
///
/// Oscillates opacity between a minimum and maximum value.
/// Useful for attention-grabbing indicators.
#[derive(Debug, Clone, Copy)]
pub struct FadeAnimation {
    /// Minimum opacity (0.0–1.0).
    pub min_opacity: f32,
    /// Maximum opacity (0.0–1.0).
    pub max_opacity: f32,
    /// Frequency in Hz.
    pub frequency: f32,
    /// Elapsed time in seconds.
    pub elapsed: f32,
}

impl FadeAnimation {
    /// Create a new fade animation.
    pub const fn new(min_opacity: f32, max_opacity: f32, frequency: f32) -> Self {
        Self {
            min_opacity,
            max_opacity,
            frequency,
            elapsed: 0.0,
        }
    }

    /// Evaluate the current opacity.
    pub fn evaluate(&self) -> f32 {
        let t = (self.elapsed * self.frequency * std::f32::consts::TAU).sin();
        // Map sin(-1..1) to (min..max)
        self.min_opacity + (self.max_opacity - self.min_opacity) * (t * 0.5 + 0.5)
    }

    /// Advance the animation by a time delta.
    pub fn update(&mut self, dt: f32) {
        self.elapsed += dt;
    }
}

/// A bounce animation that oscillates between two values with decay.
///
/// Unlike PulseAnimation, this can have a target value and will
/// gradually settle when the amplitude decays to zero.
#[derive(Debug, Clone, Copy)]
pub struct BounceAnimation {
    /// Target value the animation settles toward.
    pub target: f32,
    /// Current amplitude (decays over time).
    pub amplitude: f32,
    /// Frequency in Hz.
    pub frequency: f32,
    /// Damping factor per second (0.0 = no damping, 1.0+ = fast decay).
    pub damping: f32,
    /// Elapsed time in seconds.
    pub elapsed: f32,
}

impl BounceAnimation {
    /// Create a new bounce animation.
    pub const fn new(target: f32, initial_amplitude: f32, frequency: f32, damping: f32) -> Self {
        Self {
            target,
            amplitude: initial_amplitude,
            frequency,
            damping,
            elapsed: 0.0,
        }
    }

    /// Evaluate the current value.
    pub fn evaluate(&self) -> f32 {
        let decay = (-self.damping * self.elapsed).exp();
        let current_amplitude = self.amplitude * decay;
        self.target
            + current_amplitude * (self.elapsed * self.frequency * std::f32::consts::TAU).sin()
    }

    /// Advance the animation by a time delta.
    pub fn update(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    /// Check if the animation has effectively settled.
    pub fn is_settled(&self, threshold: f32) -> bool {
        let decay = (-self.damping * self.elapsed).exp();
        (self.amplitude * decay).abs() < threshold
    }
}

/// A spring animation simulating a physical spring.
///
/// Uses a damped spring formula for natural-looking motion.
#[derive(Debug, Clone, Copy)]
pub struct SpringAnimation {
    /// Target value.
    pub target: f32,
    /// Initial displacement from target.
    pub initial_displacement: f32,
    /// Spring stiffness (higher = bouncier).
    pub stiffness: f32,
    /// Damping ratio (0.0 = no damping, 1.0 = critically damped).
    pub damping_ratio: f32,
    /// Elapsed time in seconds.
    pub elapsed: f32,
}

impl SpringAnimation {
    /// Create a new spring animation.
    pub const fn new(
        target: f32,
        initial_displacement: f32,
        stiffness: f32,
        damping_ratio: f32,
    ) -> Self {
        Self {
            target,
            initial_displacement,
            stiffness,
            damping_ratio,
            elapsed: 0.0,
        }
    }

    /// Evaluate the current value.
    pub fn evaluate(&self) -> f32 {
        if self.damping_ratio >= 1.0 {
            // Critically damped or overdamped — no oscillation
            let decay = (-self.stiffness * self.elapsed).exp();
            self.target + self.initial_displacement * decay
        } else {
            // Underdamped — oscillation with decay
            let omega = (self.stiffness * (1.0 - self.damping_ratio * self.damping_ratio)).sqrt();
            let decay = (-(self.damping_ratio * self.stiffness) * self.elapsed).exp();
            self.target + self.initial_displacement * decay * (omega * self.elapsed).sin()
        }
    }

    /// Advance the animation by a time delta.
    pub fn update(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    /// Check if the spring has settled.
    pub fn is_settled(&self, threshold: f32) -> bool {
        let value = self.evaluate();
        (value - self.target).abs() < threshold
    }
}

/// A keyframe-based animation for complex motion sequences.
///
/// Defines a series of keyframes with timestamps and values,
/// interpolating between them.
#[derive(Debug, Clone)]
pub struct KeyframeAnimation {
    /// Keyframes sorted by time.
    pub keyframes: Vec<Keyframe>,
    /// Elapsed time in seconds.
    pub elapsed: f32,
    /// Whether the animation loops.
    pub loop_animation: bool,
    /// Easing function for interpolation between keyframes.
    pub easing: Easing,
}

/// A single keyframe in a keyframe animation.
#[derive(Debug, Clone, Copy)]
pub struct Keyframe {
    /// Time in seconds.
    pub time: f32,
    /// Value at this keyframe.
    pub value: f32,
}

impl KeyframeAnimation {
    /// Create a new keyframe animation.
    pub fn new(keyframes: Vec<Keyframe>) -> Self {
        Self {
            keyframes,
            elapsed: 0.0,
            loop_animation: false,
            easing: Easing::Linear,
        }
    }

    /// Add a keyframe.
    pub fn add_keyframe(&mut self, time: f32, value: f32) {
        self.keyframes.push(Keyframe { time, value });
        self.keyframes.sort_by_key(|k| (k.time * 1000.0) as i32);
    }

    /// Evaluate the current value.
    pub fn evaluate(&self) -> f32 {
        if self.keyframes.is_empty() {
            return 0.0;
        }

        let mut t = self.elapsed;
        let duration = self.keyframes.last().map(|k| k.time).unwrap_or(1.0);

        if self.loop_animation && duration > 0.0 {
            t = t % duration;
        }

        // Find the surrounding keyframes
        let mut lower = &self.keyframes[0];
        let mut upper = *lower;

        for kf in &self.keyframes {
            if kf.time <= t {
                lower = kf;
            } else {
                upper = *kf;
                break;
            }
        }

        if lower.time == upper.time {
            lower.value
        } else {
            let local_t = (t - lower.time) / (upper.time - lower.time);
            let eased_t = self.easing.compute(local_t);
            lower.value + (upper.value - lower.value) * eased_t
        }
    }

    /// Advance the animation by a time delta.
    pub fn update(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    /// Get the total duration.
    pub fn duration(&self) -> f32 {
        self.keyframes.last().map(|k| k.time).unwrap_or(0.0)
    }

    /// Set the easing function.
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Enable looping.
    pub fn with_loop(mut self, loop_anim: bool) -> Self {
        self.loop_animation = loop_anim;
        self
    }
}

/// Property that can be animated on a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimatedProperty {
    /// Uniform scale factor.
    Scale,
    /// X component of scale.
    ScaleX,
    /// Y component of scale.
    ScaleY,
    /// Rotation in degrees.
    Rotation,
    /// Opacity.
    Opacity,
    /// X position.
    PositionX,
    /// Y position.
    PositionY,
}

/// A continuous animation attached to a specific node property.
///
/// This is the bridge between continuous animation types and the scene graph.
/// Only nodes with active animations are updated each tick.
#[derive(Debug, Clone)]
pub struct ContinuousAnimation {
    /// The property to animate.
    pub property: AnimatedProperty,
    /// The animation type.
    pub animation: ContinuousAnimationType,
    /// Whether the animation is currently active.
    pub active: bool,
}

impl ContinuousAnimation {
    /// Create a pulse scale animation.
    pub fn pulse_scale(amplitude: f32, frequency: f32) -> Self {
        Self {
            property: AnimatedProperty::Scale,
            animation: ContinuousAnimationType::Pulse(PulseAnimation::new(amplitude, frequency)),
            active: true,
        }
    }

    /// Create a rotation animation.
    pub fn rotate(speed: f32) -> Self {
        Self {
            property: AnimatedProperty::Rotation,
            animation: ContinuousAnimationType::Rotate(RotateAnimation::new(speed)),
            active: true,
        }
    }

    /// Create a fade animation.
    pub fn fade(min_opacity: f32, max_opacity: f32, frequency: f32) -> Self {
        Self {
            property: AnimatedProperty::Opacity,
            animation: ContinuousAnimationType::Fade(FadeAnimation::new(
                min_opacity,
                max_opacity,
                frequency,
            )),
            active: true,
        }
    }

    /// Update the animation by a time delta and return the current value.
    pub fn update(&mut self, dt: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        match &mut self.animation {
            ContinuousAnimationType::Pulse(p) => {
                p.update(dt);
                p.evaluate()
            }
            ContinuousAnimationType::Rotate(r) => {
                r.update(dt);
                r.evaluate()
            }
            ContinuousAnimationType::Fade(f) => {
                f.update(dt);
                f.evaluate()
            }
            ContinuousAnimationType::Bounce(b) => {
                b.update(dt);
                b.evaluate()
            }
            ContinuousAnimationType::Spring(s) => {
                s.update(dt);
                s.evaluate()
            }
            ContinuousAnimationType::Keyframe(k) => {
                k.update(dt);
                k.evaluate()
            }
        }
    }

    /// Pause the animation.
    pub fn pause(&mut self) {
        self.active = false;
    }

    /// Resume the animation.
    pub fn resume(&mut self) {
        self.active = true;
    }
}

/// Types of continuous animations.
#[derive(Debug, Clone)]
pub enum ContinuousAnimationType {
    /// Pulsing scale animation.
    Pulse(PulseAnimation),
    /// Continuous rotation.
    Rotate(RotateAnimation),
    /// Opacity fade.
    Fade(FadeAnimation),
    /// Bounce with decay.
    Bounce(BounceAnimation),
    /// Spring physics.
    Spring(SpringAnimation),
    /// Keyframe sequence.
    Keyframe(KeyframeAnimation),
}

/// Tracker for animated nodes in a scene.
///
/// Maintains a list of nodes with active continuous animations.
/// Only these nodes are updated each animation tick, avoiding
/// unnecessary work on static nodes.
#[derive(Debug, Clone)]
pub struct AnimatedNode {
    /// The node ID.
    pub node_id: crate::geometry::NodeId,
    /// Active continuous animations for this node.
    pub animations: Vec<ContinuousAnimation>,
}

impl AnimatedNode {
    /// Create a new animated node.
    pub fn new(node_id: crate::geometry::NodeId) -> Self {
        Self {
            node_id,
            animations: Vec::new(),
        }
    }

    /// Add a continuous animation.
    pub fn add_animation(&mut self, anim: ContinuousAnimation) {
        self.animations.push(anim);
    }

    /// Update all animations by a time delta.
    /// Returns a list of (property, value) pairs to apply to the node.
    pub fn update(&mut self, dt: f32) -> Vec<(AnimatedProperty, f32)> {
        self.animations
            .iter_mut()
            .filter(|a| a.active)
            .map(|a| (a.property, a.update(dt)))
            .collect()
    }

    /// Check if this node has any active animations.
    pub fn has_active_animations(&self) -> bool {
        self.animations.iter().any(|a| a.active)
    }

    /// Get the number of active animations.
    pub fn active_animation_count(&self) -> usize {
        self.animations.iter().filter(|a| a.active).count()
    }
}

/// Manager for all animated nodes in a scene.
///
/// Provides efficient per-tick updates for only the animated nodes.
#[derive(Debug, Clone)]
pub struct AnimationManager {
    /// Animated nodes indexed by node ID.
    animated_nodes: Vec<AnimatedNode>,
}

impl AnimationManager {
    /// Create a new animation manager.
    pub fn new() -> Self {
        Self {
            animated_nodes: Vec::new(),
        }
    }

    /// Register a node for animation.
    pub fn register_node(&mut self, node_id: crate::geometry::NodeId) {
        if !self.animated_nodes.iter().any(|an| an.node_id == node_id) {
            self.animated_nodes.push(AnimatedNode::new(node_id));
        }
    }

    /// Add a continuous animation to a node.
    pub fn add_animation(&mut self, node_id: crate::geometry::NodeId, anim: ContinuousAnimation) {
        self.register_node(node_id);
        if let Some(animated) = self
            .animated_nodes
            .iter_mut()
            .find(|an| an.node_id == node_id)
        {
            animated.add_animation(anim);
        }
    }

    /// Remove all animations for a node.
    pub fn remove_node(&mut self, node_id: crate::geometry::NodeId) {
        self.animated_nodes.retain(|an| an.node_id != node_id);
    }

    /// Update all animated nodes by a time delta.
    /// Returns a map of node_id -> (property, value) pairs.
    pub fn update_all(
        &mut self,
        dt: f32,
    ) -> Vec<(crate::geometry::NodeId, Vec<(AnimatedProperty, f32)>)> {
        self.animated_nodes
            .iter_mut()
            .filter(|an| an.has_active_animations())
            .map(|an| {
                let updates = an.update(dt);
                (an.node_id, updates)
            })
            .filter(|(_, updates)| !updates.is_empty())
            .collect()
    }

    /// Get the number of animated nodes.
    pub fn animated_node_count(&self) -> usize {
        self.animated_nodes.len()
    }

    /// Get the number of nodes with active animations.
    pub fn active_node_count(&self) -> usize {
        self.animated_nodes
            .iter()
            .filter(|an| an.has_active_animations())
            .count()
    }

    /// Get the total number of active animations.
    pub fn active_animation_count(&self) -> usize {
        self.animated_nodes
            .iter()
            .map(|an| an.active_animation_count())
            .sum()
    }

    /// Check if a node is being animated.
    pub fn is_animated(&self, node_id: crate::geometry::NodeId) -> bool {
        self.animated_nodes.iter().any(|an| an.node_id == node_id)
    }

    /// Get the animated node data.
    pub fn get(&self, node_id: crate::geometry::NodeId) -> Option<&AnimatedNode> {
        self.animated_nodes.iter().find(|an| an.node_id == node_id)
    }

    /// Get mutable access to animated node data.
    pub fn get_mut(&mut self, node_id: crate::geometry::NodeId) -> Option<&mut AnimatedNode> {
        self.animated_nodes
            .iter_mut()
            .find(|an| an.node_id == node_id)
    }

    /// Clear all animations.
    pub fn clear(&mut self) {
        self.animated_nodes.clear();
    }
}

impl Default for AnimationManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Interaction types for icon and node interaction
// ---------------------------------------------------------------------------

/// Mouse button for interaction events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionMouseButton {
    /// Left mouse button.
    Left,
    /// Middle mouse button.
    Middle,
    /// Right mouse button.
    Right,
}

/// State of a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonState {
    /// Button was pressed.
    Pressed,
    /// Button was released.
    Released,
}

/// Type of interaction event.
#[derive(Debug, Clone, PartialEq)]
pub enum InteractionEvent {
    /// Mouse button pressed or released.
    MouseButton {
        /// Button that was pressed/released.
        button: InteractionMouseButton,
        /// Button state.
        state: MouseButtonState,
        /// Position in logical coordinates.
        position: Point,
    },
    /// Mouse moved.
    MouseMove {
        /// New position in logical coordinates.
        position: Point,
        /// Delta from previous position.
        delta: Vector2D,
    },
    /// Mouse wheel scrolled.
    MouseWheel {
        /// Scroll delta (positive = up/right).
        delta: Vector2D,
    },
    /// Pointer entered a node's bounds.
    PointerEnter {
        /// Position in logical coordinates.
        position: Point,
    },
    /// Pointer left a node's bounds.
    PointerLeave,
    /// Double-click event.
    DoubleClick {
        /// Position in logical coordinates.
        position: Point,
    },
    /// Long press (press and hold).
    LongPress {
        /// Position in logical coordinates.
        position: Point,
        /// Duration of the press in seconds.
        duration: f32,
    },
}

/// Hover state of an interactive node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverState {
    /// Pointer is not over the node.
    Normal,
    /// Pointer is over the node.
    Hovered,
    /// Pointer is over the node and button is pressed.
    Pressed,
}

/// Drag state for interactive nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragState {
    /// Not currently dragging.
    None,
    /// Drag is in progress.
    InProgress {
        /// Offset from the drag start point.
        offset: Vector2D,
        /// Initial position when drag started.
        start_position: Point,
    },
}

/// Gesture type for touch/multi-touch interaction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GestureType {
    /// Pinch to zoom.
    Pinch,
    /// Two-finger pan.
    Pan,
    /// Rotation gesture.
    Rotate,
}

/// Gesture state tracking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GestureState {
    /// Type of gesture.
    pub gesture_type: GestureType,
    /// Scale factor for pinch gestures.
    pub scale: f32,
    /// Rotation in degrees for rotate gestures.
    pub rotation: f32,
    /// Translation for pan gestures.
    pub translation: Vector2D,
}

/// Interactive behavior configuration for a node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractionConfig {
    /// Whether the node responds to hover events.
    pub hoverable: bool,
    /// Whether the node responds to click events.
    pub clickable: bool,
    /// Whether the node can be dragged.
    pub draggable: bool,
    /// Whether the node responds to long press.
    pub long_pressable: bool,
    /// Long press threshold in seconds.
    pub long_press_threshold: f32,
    /// Whether the node responds to double-click.
    pub double_clickable: bool,
    /// Double-click threshold in milliseconds.
    pub double_click_threshold_ms: u32,
}

impl InteractionConfig {
    /// Create a fully interactive config.
    pub fn all() -> Self {
        Self {
            hoverable: true,
            clickable: true,
            draggable: true,
            long_pressable: true,
            long_press_threshold: 0.5,
            double_clickable: true,
            double_click_threshold_ms: 300,
        }
    }

    /// Create a hover-only config.
    pub fn hover_only() -> Self {
        Self {
            hoverable: true,
            ..Self::none()
        }
    }

    /// Create a click-only config.
    pub fn click_only() -> Self {
        Self {
            clickable: true,
            ..Self::none()
        }
    }

    /// Create a non-interactive config.
    pub fn none() -> Self {
        Self {
            hoverable: false,
            clickable: false,
            draggable: false,
            long_pressable: false,
            long_press_threshold: 0.5,
            double_clickable: false,
            double_click_threshold_ms: 300,
        }
    }
}

impl Default for InteractionConfig {
    fn default() -> Self {
        Self::none()
    }
}

/// State tracker for interactive nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionState {
    /// Current hover state.
    pub hover: HoverState,
    /// Current drag state.
    pub drag: DragState,
    /// Last click time in milliseconds (for double-click detection).
    pub last_click_time_ms: u64,
    /// Current long press duration in seconds.
    pub long_press_duration: f32,
    /// Whether a long press has been triggered.
    pub long_press_triggered: bool,
}

impl InteractionState {
    /// Create a new interaction state.
    pub fn new() -> Self {
        Self {
            hover: HoverState::Normal,
            drag: DragState::None,
            last_click_time_ms: 0,
            long_press_duration: 0.0,
            long_press_triggered: false,
        }
    }

    /// Reset the interaction state.
    pub fn reset(&mut self) {
        self.hover = HoverState::Normal;
        self.drag = DragState::None;
        self.long_press_duration = 0.0;
        self.long_press_triggered = false;
    }

    /// Check if a double-click has occurred.
    pub fn is_double_click(&self, current_time_ms: u64, threshold_ms: u32) -> bool {
        if self.last_click_time_ms == 0 {
            false
        } else {
            (current_time_ms - self.last_click_time_ms) < threshold_ms as u64
        }
    }
}

impl Default for InteractionState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Dynamic geometry for procedural shapes
// ---------------------------------------------------------------------------

/// A dynamic star shape with configurable points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarShape {
    /// Number of points on the star.
    pub points: u32,
    /// Outer radius.
    pub outer_radius: f32,
    /// Inner radius (ratio to outer, 0.0–1.0).
    pub inner_radius_ratio: f32,
    /// Rotation offset in degrees.
    pub rotation: f32,
}

impl StarShape {
    /// Create a new star shape.
    pub const fn new(points: u32, outer_radius: f32, inner_radius_ratio: f32) -> Self {
        Self {
            points,
            outer_radius,
            inner_radius_ratio,
            rotation: 0.0,
        }
    }

    /// Compute the vertices of the star.
    pub fn vertices(&self, center: Point) -> Vec<Point> {
        let mut verts = Vec::new();
        let total_points = self.points * 2;

        for i in 0..total_points {
            let angle = (i as f32 / total_points as f32 * std::f32::consts::TAU)
                + self.rotation.to_radians();
            let radius = if i % 2 == 0 {
                self.outer_radius
            } else {
                self.outer_radius * self.inner_radius_ratio
            };

            verts.push(Point::new(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            ));
        }

        verts
    }
}

/// A dynamic polygon with configurable sides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolygonShape {
    /// Number of sides.
    pub sides: u32,
    /// Radius of the circumscribed circle.
    pub radius: f32,
    /// Rotation offset in degrees.
    pub rotation: f32,
}

impl PolygonShape {
    /// Create a new regular polygon.
    pub const fn new(sides: u32, radius: f32) -> Self {
        Self {
            sides,
            radius,
            rotation: 0.0,
        }
    }

    /// Compute the vertices of the polygon.
    pub fn vertices(&self, center: Point) -> Vec<Point> {
        let mut verts = Vec::new();

        for i in 0..self.sides {
            let angle =
                (i as f32 / self.sides as f32 * std::f32::consts::TAU) + self.rotation.to_radians();

            verts.push(Point::new(
                center.x + angle.cos() * self.radius,
                center.y + angle.sin() * self.radius,
            ));
        }

        verts
    }
}

/// A dynamic ring/arc shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingShape {
    /// Inner radius.
    pub inner_radius: f32,
    /// Outer radius.
    pub outer_radius: f32,
    /// Start angle in degrees.
    pub start_angle: f32,
    /// End angle in degrees.
    pub end_angle: f32,
    /// Number of segments for smoothness.
    pub segments: u32,
}

impl RingShape {
    /// Create a new ring shape.
    pub const fn new(inner_radius: f32, outer_radius: f32) -> Self {
        Self {
            inner_radius,
            outer_radius,
            start_angle: 0.0,
            end_angle: 360.0,
            segments: 32,
        }
    }

    /// Set the arc angles.
    pub fn with_arc(mut self, start_angle: f32, end_angle: f32) -> Self {
        self.start_angle = start_angle;
        self.end_angle = end_angle;
        self
    }

    /// Set the number of segments.
    pub fn with_segments(mut self, segments: u32) -> Self {
        self.segments = segments;
        self
    }

    /// Check if this is a full ring (360 degrees).
    pub fn is_full_ring(&self) -> bool {
        (self.end_angle - self.start_angle).abs() >= 359.9
    }
}

/// Progress indicator shape (circular progress bar).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressRing {
    /// Radius of the ring.
    pub radius: f32,
    /// Thickness of the ring.
    pub thickness: f32,
    /// Current progress (0.0–1.0).
    pub progress: f32,
    /// Start angle in degrees (0 = right, 90 = down).
    pub start_angle: f32,
}

impl ProgressRing {
    /// Create a new progress ring.
    pub const fn new(radius: f32, thickness: f32) -> Self {
        Self {
            radius,
            thickness,
            progress: 0.0,
            start_angle: -90.0,
        }
    }

    /// Set the progress value.
    pub fn with_progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self
    }

    /// Set the start angle.
    pub fn with_start_angle(mut self, angle: f32) -> Self {
        self.start_angle = angle;
        self
    }

    /// Get the sweep angle in degrees.
    pub fn sweep_angle(&self) -> f32 {
        self.progress * 360.0
    }
}

/// A dynamic particle for particle systems.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    /// Current position.
    pub position: Point,
    /// Velocity.
    pub velocity: Vector2D,
    /// Lifetime in seconds.
    pub lifetime: f32,
    /// Remaining life in seconds.
    pub remaining: f32,
    /// Size.
    pub size: f32,
    /// Color.
    pub color: Color,
}

impl Particle {
    /// Create a new particle.
    pub fn new(
        position: Point,
        velocity: Vector2D,
        lifetime: f32,
        size: f32,
        color: Color,
    ) -> Self {
        Self {
            position,
            velocity,
            lifetime,
            remaining: lifetime,
            size,
            color,
        }
    }

    /// Update the particle by a time delta.
    pub fn update(&mut self, dt: f32, gravity: Vector2D) {
        self.velocity.x += gravity.x * dt;
        self.velocity.y += gravity.y * dt;
        self.position.x += self.velocity.x * dt;
        self.position.y += self.velocity.y * dt;
        self.remaining = (self.remaining - dt).max(0.0);
    }

    /// Check if the particle is still alive.
    pub fn is_alive(&self) -> bool {
        self.remaining > 0.0
    }

    /// Get the life ratio (1.0 = just born, 0.0 = dead).
    pub fn life_ratio(&self) -> f32 {
        self.remaining / self.lifetime
    }
}

/// Configuration for a particle system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleSystemConfig {
    /// Emitter position.
    pub emitter: Point,
    /// Velocity range (min, max).
    pub velocity_range: (Vector2D, Vector2D),
    /// Particle lifetime range (min, max) in seconds.
    pub lifetime_range: (f32, f32),
    /// Particle size range (min, max).
    pub size_range: (f32, f32),
    /// Emission rate (particles per second).
    pub emission_rate: f32,
    /// Gravity applied to particles.
    pub gravity: Vector2D,
    /// Maximum number of particles.
    pub max_particles: usize,
}

impl Default for ParticleSystemConfig {
    fn default() -> Self {
        Self {
            emitter: Point::ORIGIN,
            velocity_range: (Vector2D::new(-10.0, -100.0), Vector2D::new(10.0, -100.0)),
            lifetime_range: (0.5, 2.0),
            size_range: (2.0, 8.0),
            emission_rate: 50.0,
            gravity: Vector2D::new(0.0, 200.0),
            max_particles: 1000,
        }
    }
}

/// A particle system for dynamic visual effects.
#[derive(Debug, Clone)]
pub struct ParticleSystem {
    /// Configuration.
    pub config: ParticleSystemConfig,
    /// Active particles.
    pub particles: Vec<Particle>,
    /// Accumulated time for emission.
    pub emission_accumulator: f32,
}

impl ParticleSystem {
    /// Create a new particle system.
    pub fn new(config: ParticleSystemConfig) -> Self {
        Self {
            config,
            particles: Vec::new(),
            emission_accumulator: 0.0,
        }
    }

    /// Emit a single particle with random properties.
    fn emit_particle(&mut self, _seed: u64) {
        // Simple emission — in production, use a proper RNG
        let lifetime = self.config.lifetime_range.0
            + (self.config.lifetime_range.1 - self.config.lifetime_range.0)
                * (self.particles.len() as f32 % 1.0);

        let size = self.config.size_range.0
            + (self.config.size_range.1 - self.config.size_range.0)
                * (self.particles.len() as f32 % 1.0);

        let velocity = Vector2D::new(
            self.config.velocity_range.0.x
                + (self.config.velocity_range.1.x - self.config.velocity_range.0.x)
                    * (self.particles.len() as f32 % 1.0),
            self.config.velocity_range.0.y
                + (self.config.velocity_range.1.y - self.config.velocity_range.0.y)
                    * (self.particles.len() as f32 % 1.0),
        );

        self.particles.push(Particle::new(
            self.config.emitter,
            velocity,
            lifetime,
            size,
            Color::WHITE,
        ));
    }

    /// Update the particle system by a time delta.
    pub fn update(&mut self, dt: f32) {
        // Emit new particles
        self.emission_accumulator += dt * self.config.emission_rate;
        while self.emission_accumulator >= 1.0 && self.particles.len() < self.config.max_particles {
            self.emit_particle(self.particles.len() as u64);
            self.emission_accumulator -= 1.0;
        }

        // Update existing particles
        self.particles.iter_mut().for_each(|p| {
            p.update(dt, self.config.gravity);
        });

        // Remove dead particles
        self.particles.retain(|p| p.is_alive());
    }

    /// Get the number of active particles.
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    /// Get a reference to all particles.
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// Clear all particles.
    pub fn clear(&mut self) {
        self.particles.clear();
        self.emission_accumulator = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_animation() {
        let mut anim = Animation::float(0.0, 100.0, 1.0);
        let val = anim.update(0.5).unwrap();
        assert!(val.as_float() > 0.0);
        assert!(val.as_float() <= 100.0);
    }

    #[test]
    fn test_easing_linear() {
        assert!((Easing::Linear.compute(0.5) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_easing_bounds() {
        assert!((Easing::EaseOut.compute(0.0) - 0.0).abs() < f32::EPSILON);
        assert!((Easing::EaseOut.compute(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_color_animation() {
        let mut anim = Animation::color(Color::RED, Color::BLUE, 1.0);
        let val = anim.update(0.5).unwrap();
        let color = val.as_color();
        assert!(color.r > 0.0 && color.r < 1.0);
        assert!(color.b > 0.0 && color.b < 1.0);
    }

    // -----------------------------------------------------------------------
    // Continuous animation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_pulse_animation_default() {
        let pulse = PulseAnimation::default();
        let scale = pulse.evaluate();
        assert!((scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pulse_animation_range() {
        let mut pulse = PulseAnimation::new(0.08, 0.5);
        assert!((pulse.min_scale() - 0.92).abs() < f32::EPSILON);
        assert!((pulse.max_scale() - 1.08).abs() < f32::EPSILON);

        // Advance and check range
        pulse.update(1.0);
        let scale = pulse.evaluate();
        assert!(scale >= 0.92 - f32::EPSILON);
        assert!(scale <= 1.08 + f32::EPSILON);
    }

    #[test]
    fn test_pulse_animation_center_scale() {
        let pulse = PulseAnimation::new(0.1, 1.0);
        // At t=0, sin(0) = 0, so scale = 1.0
        assert!((pulse.evaluate() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rotate_animation() {
        let mut rot = RotateAnimation::new(360.0);
        assert!((rot.evaluate() - 0.0).abs() < f32::EPSILON);

        rot.update(0.5);
        assert!((rot.evaluate() - 180.0).abs() < f32::EPSILON);

        rot.update(0.5);
        assert!((rot.evaluate() - 360.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_fade_animation() {
        let mut fade = FadeAnimation::new(0.3, 1.0, 1.0);
        let opacity = fade.evaluate();
        assert!(opacity >= 0.3 - f32::EPSILON);
        assert!(opacity <= 1.0 + f32::EPSILON);

        fade.update(0.5);
        let opacity2 = fade.evaluate();
        assert!(opacity2 >= 0.3 - f32::EPSILON);
        assert!(opacity2 <= 1.0 + f32::EPSILON);
    }

    #[test]
    fn test_bounce_animation_settles() {
        let mut bounce = BounceAnimation::new(1.0, 0.5, 2.0, 2.0);
        assert!(!bounce.is_settled(0.001));

        bounce.update(2.0);
        assert!(bounce.is_settled(0.01));
    }

    #[test]
    fn test_spring_animation() {
        let mut spring = SpringAnimation::new(1.0, 0.5, 5.0, 0.7);
        let value = spring.evaluate();
        assert!(value > 0.0);

        spring.update(1.0);
        assert!(spring.is_settled(0.1));
    }

    #[test]
    fn test_keyframe_animation() {
        let mut kf = KeyframeAnimation::new(vec![
            Keyframe {
                time: 0.0,
                value: 0.0,
            },
            Keyframe {
                time: 1.0,
                value: 100.0,
            },
            Keyframe {
                time: 2.0,
                value: 50.0,
            },
        ]);

        assert!((kf.evaluate() - 0.0).abs() < f32::EPSILON);
        kf.update(0.5);
        assert!((kf.evaluate() - 50.0).abs() < f32::EPSILON);
        kf.update(0.5);
        assert!((kf.evaluate() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_continuous_animation_pulse() {
        let mut ca = ContinuousAnimation::pulse_scale(0.1, 1.0);
        assert_eq!(ca.property, AnimatedProperty::Scale);
        assert!(ca.active);

        let value = ca.update(0.25);
        assert!(value > 0.0);
    }

    #[test]
    fn test_animated_node() {
        let mut node = AnimatedNode::new(crate::geometry::NodeId::new(1));
        assert!(!node.has_active_animations());

        node.add_animation(ContinuousAnimation::pulse_scale(0.1, 1.0));
        assert!(node.has_active_animations());
        assert_eq!(node.active_animation_count(), 1);
    }

    #[test]
    fn test_animation_manager() {
        let mut manager = AnimationManager::new();
        let id1 = crate::geometry::NodeId::new(1);
        let id2 = crate::geometry::NodeId::new(2);

        manager.add_animation(id1, ContinuousAnimation::pulse_scale(0.1, 1.0));
        manager.add_animation(id2, ContinuousAnimation::rotate(360.0));

        assert_eq!(manager.animated_node_count(), 2);
        assert_eq!(manager.active_node_count(), 2);
        assert_eq!(manager.active_animation_count(), 2);
        assert!(manager.is_animated(id1));

        let updates = manager.update_all(0.016);
        assert_eq!(updates.len(), 2);
    }

    #[test]
    fn test_animation_manager_remove() {
        let mut manager = AnimationManager::new();
        let id1 = crate::geometry::NodeId::new(1);

        manager.add_animation(id1, ContinuousAnimation::pulse_scale(0.1, 1.0));
        manager.remove_node(id1);

        assert_eq!(manager.animated_node_count(), 0);
        assert!(!manager.is_animated(id1));
    }

    // -----------------------------------------------------------------------
    // Interaction tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_interaction_config_all() {
        let config = InteractionConfig::all();
        assert!(config.hoverable);
        assert!(config.clickable);
        assert!(config.draggable);
    }

    #[test]
    fn test_interaction_config_none() {
        let config = InteractionConfig::none();
        assert!(!config.hoverable);
        assert!(!config.clickable);
    }

    #[test]
    fn test_interaction_state_double_click() {
        let mut state = InteractionState::new();
        state.last_click_time_ms = 1000;

        assert!(state.is_double_click(1200, 300));
        assert!(!state.is_double_click(1500, 300));
    }

    #[test]
    fn test_hover_state() {
        assert_eq!(HoverState::Normal, HoverState::Normal);
        assert_ne!(HoverState::Normal, HoverState::Hovered);
    }

    // -----------------------------------------------------------------------
    // Dynamic geometry tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_star_shape_vertices() {
        let star = StarShape::new(5, 50.0, 0.4);
        let verts = star.vertices(Point::new(0.0, 0.0));
        assert_eq!(verts.len(), 10); // 5 points * 2
    }

    #[test]
    fn test_polygon_shape_vertices() {
        let hex = PolygonShape::new(6, 100.0);
        let verts = hex.vertices(Point::new(0.0, 0.0));
        assert_eq!(verts.len(), 6);
    }

    #[test]
    fn test_ring_shape_full() {
        let ring = RingShape::new(10.0, 20.0);
        assert!(ring.is_full_ring());

        let arc = RingShape::new(10.0, 20.0).with_arc(0.0, 180.0);
        assert!(!arc.is_full_ring());
    }

    #[test]
    fn test_progress_ring() {
        let ring = ProgressRing::new(50.0, 5.0).with_progress(0.5);
        assert!((ring.sweep_angle() - 180.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_particle_lifecycle() {
        let mut p = Particle::new(
            Point::ORIGIN,
            Vector2D::new(0.0, -100.0),
            1.0,
            5.0,
            Color::WHITE,
        );

        assert!(p.is_alive());
        assert!((p.life_ratio() - 1.0).abs() < f32::EPSILON);

        p.update(0.5, Vector2D::new(0.0, 0.0));
        assert!(p.is_alive());
        assert!((p.life_ratio() - 0.5).abs() < f32::EPSILON);

        p.update(0.6, Vector2D::new(0.0, 0.0));
        assert!(!p.is_alive());
    }

    #[test]
    fn test_particle_system() {
        let mut system = ParticleSystem::new(ParticleSystemConfig {
            emission_rate: 100.0,
            max_particles: 10,
            ..ParticleSystemConfig::default()
        });

        system.update(0.1);
        assert!(system.particle_count() > 0);
        assert!(system.particle_count() <= 10);
    }
}
