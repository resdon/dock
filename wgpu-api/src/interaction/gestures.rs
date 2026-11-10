//! Gesture recognition for the interaction engine.
//!
//! Recognizes common touch and pointer gestures: tap, double-tap, long-press,
//! swipe, pinch, and rotate. Gestures are detected from raw input events and
//! produce high-level interaction commands.

use crate::geometry::Point;
use super::input::{InputEvent, InputState, TouchEventType};

/// Type of recognized gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureType {
    /// Single tap/click.
    Tap,
    /// Double tap/click.
    DoubleTap,
    /// Long press (tap and hold).
    LongPress,
    /// Swipe in a direction.
    Swipe,
    /// Pinch (two-finger zoom).
    Pinch,
    /// Rotate (two-finger rotation).
    Rotate,
    /// Pan (drag with one finger).
    Pan,
}

/// Phase of a gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GesturePhase {
    /// Gesture just started.
    Started,
    /// Gesture is in progress.
    Updated,
    /// Gesture ended normally.
    Ended,
    /// Gesture was cancelled.
    Cancelled,
}

/// A recognized gesture event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gesture {
    /// Type of gesture.
    pub gesture_type: GestureType,
    /// Current phase.
    pub phase: GesturePhase,
    /// Position in logical coordinates.
    pub position: Point,
    /// Delta movement.
    pub delta: crate::geometry::Vector2D,
    /// For pinch: scale factor (1.0 = no change).
    pub scale: f32,
    /// For rotate: angle in radians.
    pub rotation: f32,
    /// Swipe direction (normalized).
    pub swipe_direction: crate::geometry::Vector2D,
    /// Swipe velocity (pixels/second).
    pub swipe_velocity: f32,
    /// Timestamp.
    pub timestamp: f64,
}

/// Configuration for gesture recognition.
#[derive(Debug, Clone, Copy)]
pub struct GestureConfig {
    /// Maximum time between taps for double-tap (seconds).
    pub double_tap_interval: f64,
    /// Minimum time for long press (seconds).
    pub long_press_duration: f64,
    /// Minimum swipe distance (pixels).
    pub min_swipe_distance: f32,
    /// Minimum swipe velocity (pixels/second).
    pub min_swipe_velocity: f32,
    /// Maximum swipe duration (seconds).
    pub max_swipe_duration: f64,
    /// Pinch scale sensitivity.
    pub pinch_sensitivity: f32,
    /// Rotation sensitivity (degrees per unit).
    pub rotation_sensitivity: f32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            double_tap_interval: 0.3,
            long_press_duration: 0.5,
            min_swipe_distance: 30.0,
            min_swipe_velocity: 100.0,
            max_swipe_duration: 0.5,
            pinch_sensitivity: 1.0,
            rotation_sensitivity: 1.0,
        }
    }
}

/// State for tracking gesture recognition.
#[derive(Debug, Clone)]
pub struct GestureState {
    /// Last tap position.
    pub last_tap_position: Point,
    /// Last tap timestamp.
    pub last_tap_time: f64,
    /// Press start position.
    pub press_start_position: Point,
    /// Press start timestamp.
    pub press_start_time: f64,
    /// Whether a press is currently active.
    pub is_pressing: bool,
    /// Whether long press has been triggered.
    pub long_press_triggered: bool,
    /// Previous touch distance for pinch.
    pub previous_touch_distance: f32,
    /// Previous touch angle for rotation.
    pub previous_touch_angle: f32,
    /// Swipe start position.
    pub swipe_start_position: Point,
    /// Swipe start timestamp.
    pub swipe_start_time: f64,
}

impl GestureState {
    /// Create a new empty gesture state.
    pub fn new() -> Self {
        Self {
            last_tap_position: Point::ORIGIN,
            last_tap_time: 0.0,
            press_start_position: Point::ORIGIN,
            press_start_time: 0.0,
            is_pressing: false,
            long_press_triggered: false,
            previous_touch_distance: 0.0,
            previous_touch_angle: 0.0,
            swipe_start_position: Point::ORIGIN,
            swipe_start_time: 0.0,
        }
    }

    /// Reset state.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for GestureState {
    fn default() -> Self {
        Self::new()
    }
}

/// Gesture recognizer that processes input events.
pub struct GestureRecognizer {
    /// Configuration.
    pub config: GestureConfig,
    /// Internal state.
    pub state: GestureState,
    /// Pending gestures.
    pub pending_gestures: Vec<Gesture>,
}

impl GestureRecognizer {
    /// Create a new gesture recognizer.
    pub fn new() -> Self {
        Self {
            config: GestureConfig::default(),
            state: GestureState::new(),
            pending_gestures: Vec::new(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: GestureConfig) -> Self {
        Self {
            config,
            state: GestureState::new(),
            pending_gestures: Vec::new(),
        }
    }

    /// Process an input event and detect gestures.
    pub fn process(&mut self, event: &InputEvent, input_state: &InputState) -> Vec<Gesture> {
        self.pending_gestures.clear();

        match event {
            InputEvent::Pointer(ptr) => {
                self.process_pointer(ptr, input_state);
            }
            InputEvent::Touch(touch) => {
                self.process_touch(touch, input_state);
            }
            InputEvent::Key(_) => {
                // Keyboard events don't produce gestures
            }
        }

        self.pending_gestures.clone()
    }

    /// Process a pointer event.
    fn process_pointer(
        &mut self,
        ptr: &super::input::PointerInputEvent,
        input_state: &InputState,
    ) {
        match ptr.event_type {
            super::input::PointerEventType::Button {
                state: super::input::MouseButtonState::Pressed,
                ..
            } => {
                self.on_pointer_down(ptr.position, ptr.timestamp);
            }
            super::input::PointerEventType::Button {
                state: super::input::MouseButtonState::Released,
                ..
            } => {
                self.on_pointer_up(ptr.position, ptr.timestamp, input_state);
            }
            super::input::PointerEventType::Motion { .. } => {
                self.on_pointer_move(ptr.position, ptr.timestamp);
            }
            super::input::PointerEventType::Scroll { .. } => {
                // Scroll is handled separately
            }
        }
    }

    /// Process a touch event.
    fn process_touch(
        &mut self,
        touch: &super::input::TouchInputEvent,
        input_state: &InputState,
    ) {
        match touch.event_type {
            TouchEventType::Started => {
                self.on_pointer_down(touch.position, touch.timestamp);
            }
            TouchEventType::Moved => {
                self.on_pointer_move(touch.position, touch.timestamp);
                self.process_pinch_rotate(input_state, touch.timestamp);
            }
            TouchEventType::Ended | TouchEventType::Cancelled => {
                self.on_pointer_up(touch.position, touch.timestamp, input_state);
                if touch.event_type == TouchEventType::Cancelled {
                    self.cancel_gestures(touch.timestamp);
                }
            }
        }
    }

    /// Handle pointer down.
    fn on_pointer_down(&mut self, position: Point, timestamp: f64) {
        // Check for double tap
        let time_since_last_tap = timestamp - self.state.last_tap_time;
        let dist = position.distance_to(&self.state.last_tap_position);

        if time_since_last_tap < self.config.double_tap_interval as f64 && dist < 20.0 {
            self.pending_gestures.push(Gesture {
                gesture_type: GestureType::DoubleTap,
                phase: GesturePhase::Started,
                position,
                delta: crate::geometry::Vector2D::ZERO,
                scale: 1.0,
                rotation: 0.0,
                swipe_direction: crate::geometry::Vector2D::ZERO,
                swipe_velocity: 0.0,
                timestamp,
            });
        }

        self.state.last_tap_position = position;
        self.state.last_tap_time = timestamp;
        self.state.press_start_position = position;
        self.state.press_start_time = timestamp;
        self.state.is_pressing = true;
        self.state.long_press_triggered = false;
        self.state.swipe_start_position = position;
        self.state.swipe_start_time = timestamp;
    }

    /// Handle pointer up.
    fn on_pointer_up(
        &mut self,
        position: Point,
        timestamp: f64,
        input_state: &InputState,
    ) {
        if !self.state.is_pressing {
            return;
        }

        let duration = timestamp - self.state.press_start_time;
        let dx = position.x - self.state.swipe_start_position.x;
        let dy = position.y - self.state.swipe_start_position.y;
        let distance = (dx * dx + dy * dy).sqrt();

        if !self.state.long_press_triggered {
            if duration < self.config.double_tap_interval as f64 && distance < 10.0 {
                // Tap
                self.pending_gestures.push(Gesture {
                    gesture_type: GestureType::Tap,
                    phase: GesturePhase::Ended,
                    position,
                    delta: crate::geometry::Vector2D::ZERO,
                    scale: 1.0,
                    rotation: 0.0,
                    swipe_direction: crate::geometry::Vector2D::ZERO,
                    swipe_velocity: 0.0,
                    timestamp,
                });
            } else if distance >= self.config.min_swipe_distance {
                // Swipe
                let swipe_duration = (timestamp - self.state.swipe_start_time).max(0.001);
                let velocity = distance / swipe_duration;

                if velocity >= self.config.min_swipe_velocity {
                    let dir = if distance > 0.0 {
                        crate::geometry::Vector2D::new(dx / distance, dy / distance)
                    } else {
                        crate::geometry::Vector2D::ZERO
                    };

                    self.pending_gestures.push(Gesture {
                        gesture_type: GestureType::Swipe,
                        phase: GesturePhase::Ended,
                        position,
                        delta: crate::geometry::Vector2D::new(dx, dy),
                        scale: 1.0,
                        rotation: 0.0,
                        swipe_direction: dir,
                        swipe_velocity: velocity,
                        timestamp,
                    });
                }
            }
        }

        if self.state.is_pressing && duration >= self.config.long_press_duration as f64 {
            // Long press ended
            self.pending_gestures.push(Gesture {
                gesture_type: GestureType::LongPress,
                phase: GesturePhase::Ended,
                position,
                delta: crate::geometry::Vector2D::ZERO,
                scale: 1.0,
                rotation: 0.0,
                swipe_direction: crate::geometry::Vector2D::ZERO,
                swipe_velocity: 0.0,
                timestamp,
            });
        }

        self.state.is_pressing = false;
    }

    /// Handle pointer move.
    fn on_pointer_move(&mut self, position: Point, timestamp: f64) {
        if self.state.is_pressing {
            let duration = timestamp - self.state.press_start_time;

            // Check for long press
            if !self.state.long_press_triggered
                && duration >= self.config.long_press_duration as f64
            {
                self.state.long_press_triggered = true;
                self.pending_gestures.push(Gesture {
                    gesture_type: GestureType::LongPress,
                    phase: GesturePhase::Started,
                    position: self.state.press_start_position,
                    delta: crate::geometry::Vector2D::ZERO,
                    scale: 1.0,
                    rotation: 0.0,
                    swipe_direction: crate::geometry::Vector2D::ZERO,
                    swipe_velocity: 0.0,
                    timestamp,
                });
            }

            // Check for pan
            let dx = position.x - self.state.press_start_position.x;
            let dy = position.y - self.state.press_start_position.y;
            let distance = (dx * dx + dy * dy).sqrt();

            if distance > 10.0 && !self.state.long_press_triggered {
                self.pending_gestures.push(Gesture {
                    gesture_type: GestureType::Pan,
                    phase: GesturePhase::Updated,
                    position,
                    delta: crate::geometry::Vector2D::new(dx, dy),
                    scale: 1.0,
                    rotation: 0.0,
                    swipe_direction: crate::geometry::Vector2D::ZERO,
                    swipe_velocity: 0.0,
                    timestamp,
                });
            }
        }
    }

    /// Process pinch and rotate gestures from multi-touch.
    fn process_pinch_rotate(&mut self, input_state: &InputState, timestamp: f64) {
        if let Some(distance) = input_state.touch_distance() {
            if self.state.previous_touch_distance > 0.0 {
                let scale = distance / self.state.previous_touch_distance;
                if (scale - 1.0).abs() > 0.01 {
                    self.pending_gestures.push(Gesture {
                        gesture_type: GestureType::Pinch,
                        phase: GesturePhase::Updated,
                        position: input_state.touch_center().unwrap_or(Point::ORIGIN),
                        delta: crate::geometry::Vector2D::ZERO,
                        scale: (scale - 1.0) * self.config.pinch_sensitivity,
                        rotation: 0.0,
                        swipe_direction: crate::geometry::Vector2D::ZERO,
                        swipe_velocity: 0.0,
                        timestamp,
                    });
                }
            }
            self.state.previous_touch_distance = distance;
        }
    }

    /// Cancel all active gestures.
    fn cancel_gestures(&mut self, timestamp: f64) {
        self.pending_gestures.clear();
        self.state.is_pressing = false;
        self.state.long_press_triggered = false;
    }

    /// Check if a long press is currently active.
    pub fn is_long_pressing(&self, timestamp: f64) -> bool {
        self.state.is_pressing
            && !self.state.long_press_triggered
            && (timestamp - self.state.press_start_time) >= self.config.long_press_duration as f64
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.state.reset();
        self.pending_gestures.clear();
    }
}

impl Default for GestureRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::input::{
        PointerInputEvent, PointerEventType, InputMouseButton, MouseButtonState,
    };

    #[test]
    fn test_tap_gesture() {
        let mut recognizer = GestureRecognizer::new();
        let mut input_state = InputState::new();

        // Press
        let press = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Button {
                button: InputMouseButton::Left,
                state: MouseButtonState::Pressed,
            },
            position: Point::new(100.0, 100.0),
            physical_position: (200, 200),
            timestamp: 0.0,
        });
        recognizer.process(&press, &input_state);

        // Release
        let release = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Button {
                button: InputMouseButton::Left,
                state: MouseButtonState::Released,
            },
            position: Point::new(100.0, 100.0),
            physical_position: (200, 200),
            timestamp: 0.1,
        });
        let gestures = recognizer.process(&release, &input_state);
        let has_tap = gestures.iter().any(|g| g.gesture_type == GestureType::Tap);
        assert!(has_tap);
    }

    #[test]
    fn test_swipe_gesture() {
        let mut recognizer = GestureRecognizer::with_config(GestureConfig {
            min_swipe_distance: 30.0,
            min_swipe_velocity: 50.0,
            ..Default::default()
        });
        let mut input_state = InputState::new();

        // Press
        let press = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Button {
                button: InputMouseButton::Left,
                state: MouseButtonState::Pressed,
            },
            position: Point::new(100.0, 100.0),
            physical_position: (200, 200),
            timestamp: 0.0,
        });
        recognizer.process(&press, &input_state);

        // Release far away
        let release = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Button {
                button: InputMouseButton::Left,
                state: MouseButtonState::Released,
            },
            position: Point::new(200.0, 100.0),
            physical_position: (400, 200),
            timestamp: 0.1,
        });
        let gestures = recognizer.process(&release, &input_state);
        let has_swipe = gestures.iter().any(|g| g.gesture_type == GestureType::Swipe);
        assert!(has_swipe);
    }

    #[test]
    fn test_gesture_config_defaults() {
        let config = GestureConfig::default();
        assert!((config.double_tap_interval - 0.3).abs() < f64::EPSILON);
        assert!((config.long_press_duration - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_gesture_state_reset() {
        let mut state = GestureState::new();
        state.is_pressing = true;
        state.reset();
        assert!(!state.is_pressing);
    }

    #[test]
    fn test_clear() {
        let mut recognizer = GestureRecognizer::new();
        recognizer.state.is_pressing = true;
        recognizer.clear();
        assert!(!recognizer.state.is_pressing);
        assert!(recognizer.pending_gestures.is_empty());
    }

    #[test]
    fn test_pan_gesture() {
        let mut recognizer = GestureRecognizer::new();
        let mut input_state = InputState::new();

        // Press
        let press = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Button {
                button: InputMouseButton::Left,
                state: MouseButtonState::Pressed,
            },
            position: Point::new(100.0, 100.0),
            physical_position: (200, 200),
            timestamp: 0.0,
        });
        recognizer.process(&press, &input_state);

        // Move
        let move_event = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Motion {
                delta: (50.0, 50.0),
            },
            position: Point::new(160.0, 160.0),
            physical_position: (320, 320),
            timestamp: 0.05,
        });
        let gestures = recognizer.process(&move_event, &input_state);
        let has_pan = gestures.iter().any(|g| g.gesture_type == GestureType::Pan);
        assert!(has_pan);
    }

    #[test]
    fn test_gesture_phase() {
        assert_eq!(GesturePhase::Started as u8, 0);
        assert_eq!(GesturePhase::Updated as u8, 1);
        assert_eq!(GesturePhase::Ended as u8, 2);
        assert_eq!(GesturePhase::Cancelled as u8, 3);
    }

    #[test]
    fn test_is_long_pressing() {
        let mut recognizer = GestureRecognizer::with_config(GestureConfig {
            long_press_duration: 0.1,
            ..Default::default()
        });
        let input_state = InputState::new();

        let press = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Button {
                button: InputMouseButton::Left,
                state: MouseButtonState::Pressed,
            },
            position: Point::new(100.0, 100.0),
            physical_position: (200, 200),
            timestamp: 0.0,
        });
        recognizer.process(&press, &input_state);

        assert!(!recognizer.is_long_pressing(0.05));
        assert!(recognizer.is_long_pressing(0.15));
    }
}
