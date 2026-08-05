use std::sync::mpsc;
use std::thread;
use dockman_lib::animations::IconAnimation;
use std::path::PathBuf;

use super::state::IconLoadResult; 

pub struct IconLoadRequest {
    pub app_id: String,
}

pub struct IconLoader {
    tx: mpsc::Sender<IconLoadRequest>,
}

fn rgba_to_bgra(mut data: Vec<u8>) -> Vec<u8> {
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2); // Swap Red (0) and Blue (2)
    }
    data
}

impl IconLoader {
    pub fn new(result_tx: mpsc::Sender<IconLoadResult>) -> Self {
        let (tx, rx) = mpsc::channel::<IconLoadRequest>();

        let home = std::env::var("HOME").unwrap_or_default();
        let anim_dir = [
            PathBuf::from("assets/24"),
            PathBuf::from(format!("{}/.local/share/dock/24", home)),
            PathBuf::from("/usr/share/dock/24"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("assets/24"));

        let anim_dir_str = anim_dir.to_str().unwrap_or("assets/24").to_string();        
        
        thread::spawn(move || {
            while let Ok(req) = rx.recv() {
                let mut source_used = "none";
                let icon_path = match crate::resolvers::get_icon_path(&req.app_id) {
                    Some(path) => {
                        source_used = "XDG get_icon_path";
                        Some(path)
                    }
                    None => {
                        match crate::resolvers::search_icon_list_file(&req.app_id) {
                            Some(path) => {
                                source_used = "icon_list.txt cache";
                                Some(path)
                            }
                            None => None,
                        }
                    }
                };

                if let Some(ref path) = icon_path {
                    println!("[ICON LOADER] App '{}' resolved via [{}] -> path: {:?}", req.app_id, source_used, path);
                    
                    let is_svg = path.extension()
                        .and_then(|e| e.to_str())
                        .map_or(false, |e| e.eq_ignore_ascii_case("svg"));

                    if is_svg {
                        let opt = resvg::usvg::Options::default();
                        if let Ok(svg_data) = std::fs::read(path) {
                            if let Ok(rtree) = resvg::usvg::Tree::from_data(&svg_data, &opt) {
                                let size = rtree.size();
                                let w = size.width() as u32;
                                let h = size.height() as u32;
                                if let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) {
                                    resvg::render(&rtree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
                                    let _ = result_tx.send(IconLoadResult {
                                        app_id: req.app_id.clone(),
                                        rgba: rgba_to_bgra(pixmap.data().to_vec()),
                                        size: w,
                                        animation: None,
                                    });
                                    continue;
                                }
                            }
                        }
                    } else {
                        if let Ok(img) = image::open(path) {
                            let rgba_img = img.to_rgba8();
                            let (w, _h) = rgba_img.dimensions();

                            let _ = result_tx.send(IconLoadResult {
                                app_id: req.app_id.clone(),
                                rgba: rgba_to_bgra(rgba_img.into_raw()),
                                size: w,
                                animation: None,
                            });
                            continue;
                        }
                    }
                }

                let anim = IconAnimation::new(&anim_dir_str, 48, 12);
                if let Some(frame) = anim.current_frame() {
                    let _ = result_tx.send(IconLoadResult {
                        app_id: req.app_id.clone(),
                        rgba: rgba_to_bgra(frame.rgba.clone()),
                        size: frame.width,
                        animation: Some(anim),
                    });
                } else if let Some((default_bytes, size)) = super::state::load_generic_fallback_bytes() {
                    let _ = result_tx.send(IconLoadResult {
                        app_id: req.app_id.clone(),
                        rgba: rgba_to_bgra(default_bytes),
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
