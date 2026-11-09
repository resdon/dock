//! Application UI components.
//!
//! Contains all dock UI elements: draw coordinator, window list, context menu,
//! widget bars, audio player, Bluetooth panel, system tray, and more.

/// Copies tightly packed popup pixels into a Wayland SHM canvas.
///
/// `SlotPool` may return a canvas whose allocation is slightly larger than
/// `stride * height` because its internal allocator rounds slot sizes up.
/// Only the pixel payload belongs to the image; any allocation tail is zeroed.
pub(crate) fn copy_popup_content(canvas: &mut [u8], content: &[u8]) -> bool {
    if canvas.len() < content.len() {
        return false;
    }

    canvas[..content.len()].copy_from_slice(content);
    canvas[content.len()..].fill(0);
    true
}

#[cfg(test)]
mod popup_buffer_tests {
    use super::copy_popup_content;

    #[test]
    fn copies_pixels_into_padded_shm_canvas() {
        let source = vec![0x5Au8; 270_000];
        let mut canvas = vec![0xCCu8; 270_016];

        assert!(copy_popup_content(&mut canvas, &source));
        assert_eq!(&canvas[..source.len()], source.as_slice());
        assert!(canvas[source.len()..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn rejects_canvas_smaller_than_pixel_payload() {
        let source = vec![0x5Au8; 16];
        let mut canvas = vec![0xCCu8; 15];

        assert!(!copy_popup_content(&mut canvas, &source));
        assert!(canvas.iter().all(|byte| *byte == 0xCC));
    }
}

pub mod audio;
pub mod audio_player_bar;
pub mod audio_popup;
pub mod bluetooth;
pub mod bluetooth_popup;
pub mod context_menu;
pub mod dock;
pub mod draw;
pub mod icon_load;
pub mod network;
pub mod widget_bar;
pub mod window_list;

pub use icon_load::IconLoader;

pub mod process_monitor;
pub mod process_popup;
pub mod rclone;
pub mod rclone_popup;
pub mod system_bar;

pub mod side_bars;
