//! Wayland drag-and-drop event listener.
//!
//! Handles wl_data_device events for drag operations.

use crate::events::{DomainEvent, DragDropEvent};
use crate::models::system::OfferId;
use crate::AppState;
use std::io::Read;
use std::os::fd::AsFd;
use wayland_client::backend::ObjectData;
use wayland_client::protocol::wl_data_device::{Event as DndEvent, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::WlDataDeviceManager;
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

impl Dispatch<WlDataDeviceManager, ()> for AppState {
    fn event(
        _state: &mut Self,
        _manager: &WlDataDeviceManager,
        _event: <WlDataDeviceManager as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataOffer, ()> for AppState {
    fn event(
        state: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let offer_id = OfferId::from(offer.id());
        state.dnd_offers.insert(offer.id(), offer.clone());

        if let wl_data_offer::Event::Offer { mime_type } = event {
            state
                .dnd_protocol
                .mime_types
                .entry(offer_id.clone())
                .or_default()
                .insert(mime_type.clone());
            state.emit_event(DomainEvent::DragDrop(DragDropEvent::OfferMimeType {
                offer_id,
                mime_type,
            }));
        }
    }

    fn event_created_child(opcode: u16, qh: &QueueHandle<Self>) -> std::sync::Arc<dyn ObjectData> {
        match opcode {
            0 => qh.make_data::<WlDataOffer, _>(()),
            _ => unreachable!(),
        }
    }
}

impl Dispatch<WlDataDevice, ()> for AppState {
    fn event(
        state: &mut Self,
        _device: &WlDataDevice,
        event: DndEvent,
        _data: &(),
        conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            DndEvent::DataOffer { id } => {
                let offer_id = OfferId::from(id.id());
                state
                    .dnd_offers
                    .entry(id.id())
                    .or_insert_with(|| id.clone());
                state
                    .dnd_protocol
                    .mime_types
                    .entry(offer_id.clone())
                    .or_default();
                state.emit_event(DomainEvent::DragDrop(DragDropEvent::DataOffer { offer_id }));
            }
            DndEvent::Enter {
                serial, x, y, id, ..
            } => {
                let offer_id = id.as_ref().map(|offer| OfferId::from(offer.id()));
                state.dnd_protocol.current_offer = offer_id.clone();
                state.dnd_protocol.current_serial = Some(serial);
                state.dnd_protocol.drag_x = x;
                state.dnd_protocol.drag_y = y;

                if let Some(ref offer_id) = offer_id {
                    if let Some(offer) = state.dnd_offers.get(&offer_id.0) {
                        let has_uri_list = state
                            .dnd_protocol
                            .mime_types
                            .get(offer_id)
                            .map(|types| types.contains("text/uri-list"))
                            .unwrap_or(false);
                        if has_uri_list {
                            offer.accept(serial, Some("text/uri-list".to_string()));
                            if offer.version() >= 3 {
                                offer.set_actions(
                                    wayland_client::protocol::wl_data_device_manager::DndAction::Copy,
                                    wayland_client::protocol::wl_data_device_manager::DndAction::Copy,
                                );
                            }
                        } else {
                            offer.accept(serial, None);
                        }
                    }
                }
                let _ = conn.flush();
                state.emit_event(DomainEvent::DragDrop(DragDropEvent::Enter {
                    offer_id,
                    serial,
                    x,
                    y,
                }));
            }
            DndEvent::Motion { x, y, .. } => {
                state.dnd_protocol.drag_x = x;
                state.dnd_protocol.drag_y = y;
                state.emit_event(DomainEvent::DragDrop(DragDropEvent::Motion { x, y }));
            }
            DndEvent::Leave => {
                state.dnd_protocol.current_offer = None;
                state.dnd_protocol.current_serial = None;
                state.dnd_protocol.mime_types.clear();
                state.emit_event(DomainEvent::DragDrop(DragDropEvent::Leave));
            }
            DndEvent::Drop => {
                let x = state.dnd_protocol.drag_x;
                let y = state.dnd_protocol.drag_y;
                let offer_id = state.dnd_protocol.current_offer.take();
                state.dnd_protocol.current_serial = None;

                let mut paths = Vec::new();
                if let Some(ref offer_id) = offer_id {
                    if let Some(offer) = state.dnd_offers.remove(&offer_id.0) {
                        let has_uri_list = state
                            .dnd_protocol
                            .mime_types
                            .get(offer_id)
                            .map(|types| types.contains("text/uri-list"))
                            .unwrap_or(false);
                        if has_uri_list {
                            if let Ok((read_pipe, write_pipe)) = os_pipe::pipe() {
                                offer.receive("text/uri-list".to_string(), write_pipe.as_fd());
                                let _ = conn.flush();
                                drop(write_pipe);

                                let mut reader = read_pipe;
                                let mut buffer = String::new();
                                if reader.read_to_string(&mut buffer).is_ok() {
                                    paths = crate::events::utils::parse_uri_list(&buffer);
                                }
                            }
                            if offer.version() >= 3 {
                                offer.finish();
                            }
                        }
                        offer.destroy();
                    }
                    state.dnd_protocol.clear_offer(offer_id);
                }

                state.emit_event(DomainEvent::DragDrop(DragDropEvent::Drop { x, y, paths }));
            }
            _ => {}
        }
    }

    fn event_created_child(opcode: u16, qh: &QueueHandle<Self>) -> std::sync::Arc<dyn ObjectData> {
        match opcode {
            0 => qh.make_data::<WlDataOffer, _>(()),
            _ => unreachable!(),
        }
    }
}
