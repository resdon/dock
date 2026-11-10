//! Core Wayland event dispatch.
//!
//! Main event queue dispatch for compositor, output, and layer-shell events.

use crate::events::{DomainEvent, OutputEvent, SystemEvent};
use crate::AppState;
use wayland_client::Proxy;

use smithay_client_toolkit::{
    compositor::CompositorHandler,
    dmabuf::{DmabufFeedback, DmabufHandler, DmabufState},
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryHandler, RegistryState},
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};

use wayland_client::protocol::{
    wl_output::{Transform, WlOutput},
    wl_region::WlRegion,
    wl_seat::WlSeat,
    wl_subcompositor::WlSubcompositor,
    wl_subsurface::WlSubsurface,
};
use wayland_client::{Connection, Dispatch, QueueHandle};

impl wayland_client::Dispatch<WlSubcompositor, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSubcompositor,
        _event: <WlSubcompositor as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl wayland_client::Dispatch<WlSubsurface, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &WlSubsurface,
        _event: <WlSubsurface as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlRegion, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegion,
        _event: <WlRegion as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl RegistryHandler<AppState> for AppState {
    fn new_global(
        state: &mut AppState,
        _conn: &Connection,
        qh: &QueueHandle<AppState>,
        name: u32,
        interface: &str,
        version: u32,
    ) {
        if interface == WlSubcompositor::interface().name {
            let subcompositor = state
                .registry_state
                .registry()
                .bind::<WlSubcompositor, _, _>(name, version.min(1), qh, ());
            state.subcompositor = Some(subcompositor);
        }
    }

    fn remove_global(
        _data: &mut AppState,
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
        _name: u32,
        _interface: &str,
    ) {
    }
}

impl ProvidesRegistryState for AppState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers!(OutputState, SeatState);
}

impl CompositorHandler for AppState {
    fn surface_enter(
        &mut self,
        _: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_surface::WlSurface,
        output: &WlOutput,
    ) {
        self.emit_event(DomainEvent::System(SystemEvent::SurfaceEnter {
            output: crate::models::system::OutputId(output.id()),
        }));
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_surface::WlSurface,
        output: &WlOutput,
    ) {
        self.emit_event(DomainEvent::System(SystemEvent::SurfaceLeave {
            output: crate::models::system::OutputId(output.id()),
        }));
    }

    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_surface::WlSurface,
        _: u32,
    ) {
        self.emit_event(DomainEvent::System(SystemEvent::FrameRequested));
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wayland_client::protocol::wl_surface::WlSurface,
        _: Transform,
    ) {
    }
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        let id = crate::models::system::OutputId(output.id());
        self.output_handles.insert(id.clone(), output);
        self.emit_event(DomainEvent::Output(OutputEvent::Seen {
            output: id,
            name: None,
        }));
    }
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        let id = crate::models::system::OutputId(output.id());
        self.output_handles.insert(id.clone(), output);
        self.emit_event(DomainEvent::Output(OutputEvent::Updated {
            output: id,
            name: None,
        }));
    }
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        let id = crate::models::system::OutputId(output.id());
        self.emit_event(DomainEvent::Output(OutputEvent::Destroyed { output: id }));
    }
}

impl LayerShellHandler for AppState {
    fn configure(
        &mut self,
        _: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // smithay-client-toolkit's delegate_layer ACKs the configure serial
        // before invoking this callback. We only need to publish the negotiated
        // size to the application state here.
        self.emit_event(DomainEvent::System(SystemEvent::LayerConfigured {
            surface_id: crate::models::system::SurfaceId(layer.wl_surface().id()),
            width: configure.new_size.0,
            height: configure.new_size.1,
        }));
    }

    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.emit_event(DomainEvent::System(SystemEvent::LayerClosed {
            surface_id: crate::models::system::SurfaceId(layer.wl_surface().id()),
        }));
    }
}

impl ShmHandler for AppState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl DmabufHandler for AppState {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_feedback(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _proxy: &wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
        _feedback: DmabufFeedback,
    ) {
    }

    fn created(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _params: &wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        _buffer: wayland_client::protocol::wl_buffer::WlBuffer,
    ) {
    }

    fn failed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _params: &wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    ) {
    }

    fn released(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _buffer: &wayland_client::protocol::wl_buffer::WlBuffer,
    ) {
    }
}

impl SeatHandler for AppState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Pointer {
            if let Some(ref ddm) = self.data_device_manager {
                self.data_device = Some(ddm.get_data_device(&seat, qh, ()));
            }
            let wl_pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to secure pointer handle");
            self.wl_pointer = Some(wl_pointer);
            self.wl_seat = Some(seat);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: WlSeat,
        cap: Capability,
    ) {
        if cap == Capability::Pointer {
            self.wl_pointer = None;
            self.data_device = None;
        }
    }
}

smithay_client_toolkit::delegate_registry!(AppState);
smithay_client_toolkit::delegate_compositor!(AppState);
smithay_client_toolkit::delegate_layer!(AppState);
smithay_client_toolkit::delegate_output!(AppState);
smithay_client_toolkit::delegate_seat!(AppState);
smithay_client_toolkit::delegate_pointer!(AppState);
smithay_client_toolkit::delegate_shm!(AppState);
smithay_client_toolkit::delegate_dmabuf!(AppState);
