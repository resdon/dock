//! Icon loading from disk.
//!
//! Background thread that resolves icon paths via XDG and decodes
//! PNG/SVG files into RGBA pixel data.

use std::sync::mpsc;
use std::thread;

use crate::state::IconLoadResult;

pub struct IconLoadRequest {
    pub app_id: String,
}

pub struct IconLoader {
    tx: mpsc::Sender<IconLoadRequest>,
}

impl IconLoader {
    pub fn new(result_tx: mpsc::Sender<IconLoadResult>) -> Self {
        let (tx, rx) = mpsc::channel::<IconLoadRequest>();

        thread::spawn(move || {
            while let Ok(req) = rx.recv() {
                let mut source_used = "none";
                let icon_path = match crate::resolvers::get_icon_path(&req.app_id) {
                    Some(path) => {
                        source_used = "XDG get_icon_path";
                        Some(path)
                    }
                    None => match crate::resolvers::search_icon_list_file(&req.app_id) {
                        Some(path) => {
                            source_used = "icon_list.txt cache";
                            Some(path)
                        }
                        None => None,
                    },
                };

                if let Some(ref path) = icon_path {
                    // Icon size matches the dock's icon size (48px). Slint renders
                    // icons directly at this resolution, no downsampling needed.
                    const ICON_SOURCE_SIZE: u32 = 48;
                    if let Some((width, _height, rgba)) =
                        crate::terminal_graphics::load_image_raw_rgba(path, ICON_SOURCE_SIZE)
                    {
                        let _ = result_tx.send(IconLoadResult {
                            app_id: req.app_id.clone(),
                            rgba,
                            size: width,
                            animation: None,
                        });
                        continue;
                    }
                }

                // Failed icon lookup gets a static fallback. A perpetual loading
                // animation used to keep the main loop rendering at 12 FPS forever,
                // which multiplied popup-fade CPU usage and prevented true idle.
                if let Some((default_bytes, size)) = crate::state::load_generic_fallback_bytes() {
                    let _ = result_tx.send(IconLoadResult {
                        app_id: req.app_id.clone(),
                        rgba: default_bytes,
                        size,
                        animation: None,
                    });
                }
            }
        });

        Self { tx }
    }

    pub fn request(&self, app_id: String) {
        let _ = self.tx.send(IconLoadRequest { app_id });
    }
}
