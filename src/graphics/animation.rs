use std::fs;
use resvg::usvg::{Options, Tree};
use resvg::tiny_skia::{Pixmap, Transform};
use std::time::{Duration, Instant};

pub struct IconAnimation {
    frames: Vec<Frame>,
    current_frame: usize,
    last_step: Instant,
    frame_duration: Duration,
    pub is_active: bool,
}

impl IconAnimation {
    pub fn new(dir_path: &str, target_size: u32, target_fps: u32) -> Self {
        let frames = load_svg_animation_sequence(dir_path, target_size);
        let frame_duration = Duration::from_millis(1000 / target_fps as u64);

        Self {
            frames,
            current_frame: 0,
            last_step: Instant::now(),
            frame_duration,
            is_active: true,
        }
    }

    /// Advances the animation frame if the time interval has elapsed.
    /// Returns `true` ONLY if the frame index actually changed.
    pub fn update(&mut self) -> bool {
        if !self.is_active || self.frames.is_empty() {
            return false;
        }

        if self.last_step.elapsed() >= self.frame_duration {
            self.current_frame = (self.current_frame + 1) % self.frames.len();
            self.last_step = Instant::now();
            true
        } else {
            false
        }
    }

    /// Returns time remaining until the next frame step is due.
    pub fn time_until_next_frame(&self) -> Duration {
        let elapsed = self.last_step.elapsed();
        if elapsed < self.frame_duration {
            self.frame_duration - elapsed
        } else {
            Duration::ZERO
        }
    }

    /// Returns the active pre-rendered RGBA buffer for blitting.
    pub fn current_frame(&self) -> Option<&Frame> {
        self.frames.get(self.current_frame)
    }
}

pub struct Frame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn load_svg_animation_sequence(dir_path: &str, target_size: u32) -> Vec<Frame> {
    let mut frames = Vec::new();
    let mut entries = match fs::read_dir(dir_path) {
        Ok(e) => e.filter_map(|res| res.ok()).collect::<Vec<_>>(),
        Err(_) => return frames,
    };

    // Sort files to ensure correct frame order (e.g. 01.svg, 02.svg, ...)
    entries.sort_by_key(|e| {
        e.path()
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok())
            .unwrap_or(0)
    });

    let opt = Options::default();

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("svg") {
            if let Ok(svg_data) = fs::read(&path) {
                if let Ok(tree) = Tree::from_data(&svg_data, &opt) {
                    let mut pixmap = Pixmap::new(target_size, target_size).unwrap();
                    
                    let sx = target_size as f32 / tree.size().width();
                    let sy = target_size as f32 / tree.size().height();
                    let transform = Transform::from_scale(sx, sy);

                    resvg::render(&tree, transform, &mut pixmap.as_mut());

                    frames.push(Frame {
                        rgba: pixmap.data().to_vec(),
                        width: target_size,
                        height: target_size,
                    });
                }
            }
        }
    }
    println!("[ANIMATION RUNNNING!!!]");
    frames
}