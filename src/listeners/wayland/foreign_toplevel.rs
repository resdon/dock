//! Wayland foreign toplevel listener.
//!
//! Tracks window lifecycle via zwlr_foreign_toplevel_manager_v1 protocol.

use crate::events::{DomainEvent, WindowEvent};
use crate::models::system::{AppId, OutputId, WindowId};
use crate::AppState;
use wayland_client::event_created_child;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    self, ZwlrForeignToplevelHandleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    self, ZwlrForeignToplevelManagerV1,
};

fn window_id(handle: &ZwlrForeignToplevelHandleV1) -> WindowId {
    WindowId(handle.id())
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            let id = toplevel.id();
            state.window_handles.insert(id.clone(), toplevel.clone());
            state.emit_event(DomainEvent::Window(WindowEvent::Created {
                window_id: WindowId(id),
            }));
        }
    }

    event_created_child!(AppState, ZwlrForeignToplevelManagerV1, [
        0 => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let window = window_id(handle);

        match event {
            zwlr_foreign_toplevel_handle_v1::Event::OutputEnter { output } => {
                state.emit_event(DomainEvent::Window(WindowEvent::OutputEntered {
                    window_id: window,
                    output: OutputId(output.id()),
                }));
            }
            zwlr_foreign_toplevel_handle_v1::Event::OutputLeave { output } => {
                state.emit_event(DomainEvent::Window(WindowEvent::OutputLeft {
                    window_id: window,
                    output: OutputId(output.id()),
                }));
            }
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                state.emit_event(DomainEvent::Window(WindowEvent::Closed {
                    window_id: window,
                }));
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                state.emit_event(DomainEvent::Window(WindowEvent::Done { window_id: window }));
            }
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                state.emit_event(DomainEvent::Window(WindowEvent::TitleChanged {
                    window_id: window,
                    title,
                }));
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                state.emit_event(DomainEvent::Window(WindowEvent::AppIdChanged {
                    window_id: window,
                    app_id: AppId::from(app_id),
                }));
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state: state_bytes } => {
                let (activated, minimized) =
                    crate::events::utils::parse_window_states(&state_bytes);
                state.emit_event(DomainEvent::Window(WindowEvent::StateChanged {
                    window_id: window,
                    activated,
                    minimized,
                }));
            }
            _ => {}
        }
    }
}
