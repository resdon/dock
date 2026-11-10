//! Input event processing and routing for the interaction engine.
//!
//! This module provides the bridge between raw input events (mouse, touch, keyboard)
//! and the physics/interaction system. It normalizes input coordinates and routes
//! events to the appropriate handlers.

use crate::geometry::Point;
use crate::physics::body::BodyId;

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMouseButton {
    /// Left mouse button.
    Left,
    /// Middle mouse button.
    Middle,
    /// Right mouse button.
    Right,
    /// Custom button (for touch/pen).
    Other(u8),
}

/// Mouse button state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonState {
    /// Button was pressed.
    Pressed,
    /// Button was released.
    Released,
}

/// Type of pointer event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerEventType {
    /// Mouse button event.
    Button {
        /// Which button.
        button: InputMouseButton,
        /// Button state.
        state: MouseButtonState,
    },
    /// Pointer moved.
    Motion {
        /// Movement delta.
        delta: (f64, f64),
    },
    /// Pointer scrolled.
    Scroll {
        /// Scroll delta.
        delta: (f64, f64),
    },
}

/// A pointer input event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerInputEvent {
    /// Type of event.
    pub event_type: PointerEventType,
    /// Position in logical coordinates.
    pub position: Point,
    /// Physical position (device pixels).
    pub physical_position: (u32, u32),
    /// Timestamp in seconds.
    pub timestamp: f64,
}

/// Touch event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEventType {
    /// New touch point.
    Started,
    /// Touch point moved.
    Moved,
    /// Touch point ended.
    Ended,
    /// Touch point cancelled.
    Cancelled,
}

/// A touch input event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchInputEvent {
    /// Touch identifier.
    pub id: u64,
    /// Type of event.
    pub event_type: TouchEventType,
    /// Position in logical coordinates.
    pub position: Point,
    /// Pressure (0.0–1.0).
    pub pressure: f32,
    /// Timestamp in seconds.
    pub timestamp: f64,
}

/// Key state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// Key was pressed.
    Pressed,
    /// Key was released.
    Released,
    /// Key was repeated.
    Repeated,
}

/// Keyboard input event.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyInputEvent {
    /// Key code.
    pub key_code: String,
    /// Key state.
    pub state: KeyState,
    /// Whether modifiers are active.
    pub modifiers: InputModifiers,
    /// Timestamp in seconds.
    pub timestamp: f64,
}

/// Input modifier keys state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputModifiers {
    /// Shift key.
    pub shift: bool,
    /// Control key.
    pub control: bool,
    /// Alt key.
    pub alt: bool,
    /// Super/Meta key.
    pub super_: bool,
}

impl InputModifiers {
    /// Check if any modifier is active.
    pub fn is_any_active(&self) -> bool {
        self.shift || self.control || self.alt || self.super_
    }

    /// Check if Ctrl is held.
    pub fn is_control(&self) -> bool {
        self.control
    }

    /// Check if Shift is held.
    pub fn is_shift(&self) -> bool {
        self.shift
    }
}

/// Unified input event enum.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// Pointer (mouse/pen) event.
    Pointer(PointerInputEvent),
    /// Touch event.
    Touch(TouchInputEvent),
    /// Keyboard event.
    Key(KeyInputEvent),
}

/// Input state tracker.
#[derive(Debug, Clone)]
pub struct InputState {
    /// Current pointer position.
    pub pointer_position: Point,
    /// Previous pointer position.
    pub previous_position: Point,
    /// Pointer delta since last frame.
    pub pointer_delta: (f64, f64),
    /// Left button state.
    pub left_button: MouseButtonState,
    /// Right button state.
    pub right_button: MouseButtonState,
    /// Middle button state.
    pub middle_button: MouseButtonState,
    /// Active modifiers.
    pub modifiers: InputModifiers,
    /// Active touch points.
    pub touch_points: Vec<TouchInputEvent>,
    /// Scroll delta this frame.
    pub scroll_delta: (f64, f64),
    /// Whether pointer moved this frame.
    pub pointer_moved: bool,
}

impl InputState {
    /// Create a new empty input state.
    pub fn new() -> Self {
        Self {
            pointer_position: Point::ORIGIN,
            previous_position: Point::ORIGIN,
            pointer_delta: (0.0, 0.0),
            left_button: MouseButtonState::Released,
            right_button: MouseButtonState::Released,
            middle_button: MouseButtonState::Released,
            modifiers: InputModifiers::default(),
            touch_points: Vec::new(),
            scroll_delta: (0.0, 0.0),
            pointer_moved: false,
        }
    }

    /// Process an input event and update state.
    pub fn process(&mut self, event: &InputEvent) {
        match event {
            InputEvent::Pointer(ptr) => {
                self.previous_position = self.pointer_position;
                match ptr.event_type {
                    PointerEventType::Button { button, state } => {
                        self.pointer_position = ptr.position;
                        match button {
                            InputMouseButton::Left => self.left_button = state,
                            InputMouseButton::Right => self.right_button = state,
                            InputMouseButton::Middle => self.middle_button = state,
                            InputMouseButton::Other(_) => {}
                        }
                    }
                    PointerEventType::Motion { delta } => {
                        self.previous_position = self.pointer_position;
                        self.pointer_position = ptr.position;
                        self.pointer_delta = *delta;
                        self.pointer_moved = true;
                    }
                    PointerEventType::Scroll { delta } => {
                        self.scroll_delta.0 += delta.0 as f64;
                        self.scroll_delta.1 += delta.1 as f64;
                    }
                }
            }
            InputEvent::Touch(touch) => {
                match touch.event_type {
                    TouchEventType::Started | TouchEventType::Moved => {
                        if let Some(existing) = self.touch_points.iter_mut().find(|t| t.id == touch.id) {
                            *existing = *touch;
                        } else {
                            self.touch_points.push(*touch);
                        }
                    }
                    TouchEventType::Ended | TouchEventType::Cancelled => {
                        self.touch_points.retain(|t| t.id != touch.id);
                    }
                }
            }
            InputEvent::Key(key) => {
                self.modifiers = key.modifiers;
            }
        }
    }

    /// Reset per-frame state.
    pub fn begin_frame(&mut self) {
        self.pointer_delta = (0.0, 0.0);
        self.scroll_delta = (0.0, 0.0);
        self.pointer_moved = false;
    }

    /// Check if left button was just pressed.
    pub fn is_left_pressed(&self) -> bool {
        self.left_button == MouseButtonState::Pressed
    }

    /// Check if left button is held.
    pub fn is_left_held(&self) -> bool {
        // In a real implementation, this would track held state across frames
        self.left_button == MouseButtonState::Pressed
    }

    /// Get the center of all touch points.
    pub fn touch_center(&self) -> Option<Point> {
        if self.touch_points.is_empty() {
            return None;
        }
        let sum_x: f32 = self.touch_points.iter().map(|t| t.position.x).sum();
        let sum_y: f32 = self.touch_points.iter().map(|t| t.position.y).sum();
        let count = self.touch_points.len() as f32;
        Some(Point::new(sum_x / count, sum_y / count))
    }

    /// Get the distance between the first two touch points (for pinch).
    pub fn touch_distance(&self) -> Option<f32> {
        if self.touch_points.len() < 2 {
            return None;
        }
        let t1 = self.touch_points[0];
        let t2 = self.touch_points[1];
        let dx = t1.position.x - t2.position.x;
        let dy = t1.position.y - t2.position.y;
        Some((dx * dx + dy * dy).sqrt())
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Input router that dispatches events to handlers.
pub struct InputRouter {
    /// Current input state.
    pub state: InputState,
    /// Scale factor for converting physical to logical coordinates.
    pub scale_factor: f32,
}

impl InputRouter {
    /// Create a new input router.
    pub fn new() -> Self {
        Self {
            state: InputState::new(),
            scale_factor: 1.0,
        }
    }

    /// Convert physical coordinates to logical coordinates.
    pub fn to_logical(&self, physical: (u32, u32)) -> Point {
        Point::new(
            physical.0 as f32 / self.scale_factor,
            physical.1 as f32 / self.scale_factor,
        )
    }

    /// Process an input event.
    pub fn process(&mut self, event: InputEvent) {
        self.state.process(&event);
    }

    /// Begin a new frame.
    pub fn begin_frame(&mut self) {
        self.state.begin_frame();
    }

    /// Set the scale factor.
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale.max(0.1);
    }
}

impl Default for InputRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_state_new() {
        let state = InputState::new();
        assert_eq!(state.pointer_position, Point::ORIGIN);
        assert_eq!(state.left_button, MouseButtonState::Released);
    }

    #[test]
    fn test_pointer_button_event() {
        let mut state = InputState::new();
        let event = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Button {
                button: InputMouseButton::Left,
                state: MouseButtonState::Pressed,
            },
            position: Point::new(100.0, 200.0),
            physical_position: (200, 400),
            timestamp: 0.0,
        });
        state.process(&event);
        assert!(state.is_left_pressed());
        assert!((state.pointer_position.x - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pointer_motion_event() {
        let mut state = InputState::new();
        let event = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Motion {
                delta: (10.0, 20.0),
            },
            position: Point::new(110.0, 120.0),
            physical_position: (220, 240),
            timestamp: 0.0,
        });
        state.process(&event);
        assert!(state.pointer_moved);
        assert!((state.pointer_delta.0 - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_touch_events() {
        let mut state = InputState::new();
        let touch = InputEvent::Touch(TouchInputEvent {
            id: 1,
            event_type: TouchEventType::Started,
            position: Point::new(50.0, 50.0),
            pressure: 0.5,
            timestamp: 0.0,
        });
        state.process(&touch);
        assert_eq!(state.touch_points.len(), 1);
        assert_eq!(state.touch_points[0].id, 1);
    }

    #[test]
    fn test_touch_center() {
        let mut state = InputState::new();
        state.touch_points.push(TouchInputEvent {
            id: 1,
            event_type: TouchEventType::Started,
            position: Point::new(0.0, 0.0),
            pressure: 0.5,
            timestamp: 0.0,
        });
        state.touch_points.push(TouchInputEvent {
            id: 2,
            event_type: TouchEventType::Started,
            position: Point::new(100.0, 100.0),
            pressure: 0.5,
            timestamp: 0.0,
        });
        let center = state.touch_center().unwrap();
        assert!((center.x - 50.0).abs() < f32::EPSILON);
        assert!((center.y - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_touch_distance() {
        let mut state = InputState::new();
        state.touch_points.push(TouchInputEvent {
            id: 1,
            event_type: TouchEventType::Started,
            position: Point::new(0.0, 0.0),
            pressure: 0.5,
            timestamp: 0.0,
        });
        state.touch_points.push(TouchInputEvent {
            id: 2,
            event_type: TouchEventType::Started,
            position: Point::new(3.0, 4.0),
            pressure: 0.5,
            timestamp: 0.0,
        });
        let dist = state.touch_distance().unwrap();
        assert!((dist - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_modifiers() {
        let mods = InputModifiers {
            shift: true,
            control: false,
            alt: false,
            super_: false,
        };
        assert!(mods.is_any_active());
        assert!(mods.is_shift());
        assert!(!mods.is_control());
    }

    #[test]
    fn test_input_router_scale() {
        let mut router = InputRouter::new();
        router.set_scale_factor(2.0);
        let logical = router.to_logical((200, 400));
        assert!((logical.x - 100.0).abs() < f32::EPSILON);
        assert!((logical.y - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_begin_frame_resets() {
        let mut state = InputState::new();
        state.pointer_delta = (10.0, 10.0);
        state.pointer_moved = true;
        state.begin_frame();
        assert!((state.pointer_delta.0 - 0.0).abs() < f32::EPSILON);
        assert!(!state.pointer_moved);
    }

    #[test]
    fn test_touch_end_removes() {
        let mut state = InputState::new();
        state.touch_points.push(TouchInputEvent {
            id: 1,
            event_type: TouchEventType::Started,
            position: Point::ORIGIN,
            pressure: 0.5,
            timestamp: 0.0,
        });
        let end = InputEvent::Touch(TouchInputEvent {
            id: 1,
            event_type: TouchEventType::Ended,
            position: Point::ORIGIN,
            pressure: 0.0,
            timestamp: 0.0,
        });
        state.process(&end);
        assert!(state.touch_points.is_empty());
    }

    #[test]
    fn test_scroll_event() {
        let mut state = InputState::new();
        let event = InputEvent::Pointer(PointerInputEvent {
            event_type: PointerEventType::Scroll {
                delta: (0.0, 100.0),
            },
            position: Point::ORIGIN,
            physical_position: (0, 0),
            timestamp: 0.0,
        });
        state.process(&event);
        assert!((state.scroll_delta.1 - 100.0).abs() < f32::EPSILON);
    }
}
