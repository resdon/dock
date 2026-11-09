//! A compact native rendering of the linked Uiverse record-player card.
use crate::models::{AudioSnapshot, MediaControl};
use crate::render::font::FontManager;
use crate::render::text::draw_text;

pub const PLAYER_WIDTH: i32 = 320;
pub const PLAYER_HEIGHT: i32 = 222;
pub const PLAYER_GAP: i32 = 8;
pub const COMPACT_WIDTH: i32 = 160;
pub const COMPACT_HEIGHT: i32 = 98;
pub const COMPACT_X: i32 = PLAYER_WIDTH - COMPACT_WIDTH;
pub const COMPACT_Y: i32 = PLAYER_HEIGHT - COMPACT_HEIGHT;
pub const DISC_SIZE: i32 = 148;
pub const DISC_CENTER_X: i32 = COMPACT_X + COMPACT_WIDTH / 2;
pub const DISC_CENTER_Y: i32 = COMPACT_Y - 45 + DISC_SIZE / 2;

pub fn player_position() -> (i32, i32) {
    (
        -(PLAYER_WIDTH + PLAYER_GAP),
        crate::state::dock::DOCK_HEIGHT as i32 - PLAYER_HEIGHT,
    )
}

pub fn compact_region_contains(x: i32, y: i32) -> bool {
    x >= COMPACT_X && x < PLAYER_WIDTH && y >= COMPACT_Y - 45 && y < PLAYER_HEIGHT
}

pub fn player_area_contains(x: i32, y: i32, expanded: bool) -> bool {
    if expanded {
        (0..PLAYER_WIDTH).contains(&x) && (0..PLAYER_HEIGHT).contains(&y)
    } else {
        compact_region_contains(x, y)
    }
}

pub fn control_at(x: i32, y: i32, expanded: bool) -> Option<MediaControl> {
    if expanded {
        if !(126..160).contains(&y) {
            return None;
        }
        if (88..128).contains(&x) {
            Some(MediaControl::Previous)
        } else if (140..180).contains(&x) {
            Some(MediaControl::PlayPause)
        } else if (192..232).contains(&x) {
            Some(MediaControl::Next)
        } else {
            None
        }
    } else {
        if !(COMPACT_Y + 72..PLAYER_HEIGHT).contains(&y) {
            return None;
        }
        if (COMPACT_X + 22..COMPACT_X + 58).contains(&x) {
            Some(MediaControl::Previous)
        } else if (COMPACT_X + 62..COMPACT_X + 98).contains(&x) {
            Some(MediaControl::PlayPause)
        } else if (COMPACT_X + 102..COMPACT_X + 138).contains(&x) {
            Some(MediaControl::Next)
        } else {
            None
        }
    }
}

pub fn render(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    scale: f64,
    alpha: f32,
    audio: &AudioSnapshot,
    font: &mut FontManager,
    expanded: bool,
) {
    let w = width as i32;
    let h = height as i32;
    if w <= 0 || h <= 0 || canvas.len() < w as usize * h as usize * 4 {
        return;
    }
    let s = |v: i32| (v as f64 * scale).round() as i32;
    let title = audio
        .media_title
        .as_deref()
        .or_else(|| audio.streams.first().map(|stream| stream.name.as_str()))
        .unwrap_or(if audio.available {
            "Nothing playing"
        } else {
            "Audio unavailable"
        });
    let artist = audio.media_artist.as_deref().unwrap_or_else(|| {
        if audio.media_status.is_some() {
            "Media session"
        } else if audio.streams.is_empty() {
            "Start a player to begin"
        } else {
            "Live audio stream"
        }
    });
    let duration = audio.media_duration_seconds.unwrap_or(0.0).max(0.0);
    let position = audio
        .media_position_seconds
        .unwrap_or(0.0)
        .clamp(0.0, duration.max(0.0));
    let progress = if duration > 0.0 {
        (position / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let enabled = audio.media_status.is_some();
    let playing = audio.media_status.as_deref() == Some("Playing");
    let red = [220, 38, 38];
    let yellow = [250, 204, 21];
    let dark = [24, 24, 27];
    let muted = [92, 83, 58];
    let control_ink = if enabled { dark } else { [150, 130, 70] };
    let play_color = if enabled { red } else { [190, 165, 85] };

    if expanded {
        rounded(
            canvas,
            w,
            h,
            0,
            0,
            s(PLAYER_WIDTH),
            s(PLAYER_HEIGHT),
            s(22),
            yellow,
            alpha,
        );
        draw_disc(canvas, w, h, s(44), s(43), s(29), alpha);
        draw_text(
            canvas,
            w,
            h,
            font,
            "NOW PLAYING",
            s(7),
            s(84),
            s(25),
            (120, 64, 8),
        );
        draw_text(
            canvas,
            w,
            h,
            font,
            &truncate(title, 28),
            s(11),
            s(84),
            s(40),
            (28, 25, 20),
        );
        draw_text(
            canvas,
            w,
            h,
            font,
            &truncate(artist, 32),
            s(8),
            s(84),
            s(53),
            (82, 73, 54),
        );
        rounded(
            canvas,
            w,
            h,
            s(16),
            s(82),
            s(288),
            s(4),
            s(2),
            [255, 241, 180],
            alpha,
        );
        rounded(
            canvas,
            w,
            h,
            s(16),
            s(82),
            s((288.0 * progress).round() as i32),
            s(4),
            s(2),
            red,
            alpha,
        );
        draw_text(
            canvas,
            w,
            h,
            font,
            &time_label(position),
            s(6),
            s(16),
            s(93),
            (82, 73, 54),
        );
        draw_text(
            canvas,
            w,
            h,
            font,
            &time_label(duration),
            s(6),
            s(272),
            s(93),
            (82, 73, 54),
        );

        // The same five-button transport row as the expanded HTML player.
        draw_shuffle(canvas, w, h, s(51), s(139), control_ink, alpha);
        chevron(canvas, w, h, s(108), s(139), false, control_ink, alpha);
        rounded(
            canvas,
            w,
            h,
            s(141),
            s(119),
            s(38),
            s(38),
            s(19),
            play_color,
            alpha,
        );
        draw_play_pause(
            canvas,
            w,
            h,
            s(160),
            s(138),
            playing,
            [255, 255, 255],
            alpha,
        );
        chevron(canvas, w, h, s(212), s(139), true, control_ink, alpha);
        draw_repeat(canvas, w, h, s(269), s(139), control_ink, alpha);

        // Output level track and speaker glyph.
        draw_speaker(canvas, w, h, s(18), s(190), muted, alpha);
        rounded(
            canvas,
            w,
            h,
            s(48),
            s(190),
            s(226),
            s(4),
            s(2),
            [255, 241, 180],
            alpha,
        );
        rounded(
            canvas,
            w,
            h,
            s(48),
            s(190),
            s((226.0 * audio.output_volume.clamp(0.0, 1.0)).round() as i32),
            s(4),
            s(2),
            red,
            alpha,
        );
        draw_text(
            canvas,
            w,
            h,
            font,
            &format!(
                "{}%",
                (audio.output_volume.clamp(0.0, 1.0) * 100.0).round() as u32
            ),
            s(7),
            s(282),
            s(193),
            (82, 73, 54),
        );
    } else {
        // Compact mode: a yellow control tray with the large record peeking
        // above it, right-aligned so it sits beside the app dock.
        let panel_x = s(COMPACT_X);
        let panel_y = s(COMPACT_Y + 40);
        rounded(
            canvas,
            w,
            h,
            panel_x,
            panel_y + s(2),
            s(COMPACT_WIDTH),
            s(58),
            s(18),
            [70, 52, 0],
            alpha * 0.25,
        );
        rounded(
            canvas,
            w,
            h,
            panel_x,
            panel_y,
            s(COMPACT_WIDTH),
            s(58),
            s(18),
            yellow,
            alpha,
        );
        draw_disc(
            canvas,
            w,
            h,
            s(DISC_CENTER_X),
            s(DISC_CENTER_Y),
            s(DISC_SIZE / 2),
            alpha,
        );
        let track_x = s(COMPACT_X + 13);
        let track_y = s(COMPACT_Y + 51);
        let track_w = s(COMPACT_WIDTH - 26);
        rounded(
            canvas,
            w,
            h,
            track_x,
            track_y,
            track_w,
            s(4),
            s(2),
            [255, 241, 180],
            alpha,
        );
        rounded(
            canvas,
            w,
            h,
            track_x,
            track_y,
            s((f64::from(COMPACT_WIDTH - 26) * progress).round() as i32),
            s(4),
            s(2),
            red,
            alpha,
        );
        let controls_y = s(COMPACT_Y + 79);
        chevron(
            canvas,
            w,
            h,
            s(COMPACT_X + 40),
            controls_y,
            false,
            control_ink,
            alpha,
        );
        draw_play_pause(
            canvas,
            w,
            h,
            s(COMPACT_X + 80),
            controls_y,
            playing,
            play_color,
            alpha,
        );
        chevron(
            canvas,
            w,
            h,
            s(COMPACT_X + 120),
            controls_y,
            true,
            control_ink,
            alpha,
        );
    }
}

fn draw_disc(canvas: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, r: i32, alpha: f32) {
    circle(canvas, w, h, cx, cy, r, [24, 23, 30], alpha);
    for n in 1..=7 {
        let ring_r = r - n * 7;
        if ring_r > r / 2 {
            circle_ring(
                canvas,
                w,
                h,
                cx,
                cy,
                ring_r,
                [
                    58 + (n as u8 % 2) * 8,
                    56 + (n as u8 % 2) * 8,
                    65 + (n as u8 % 2) * 8,
                ],
                alpha * 0.85,
            );
        }
    }
    let hub_r = (r / 2).max(1);
    circle(canvas, w, h, cx, cy, hub_r, [203, 213, 225], alpha);
    circle(canvas, w, h, cx, cy, hub_r - 3, [226, 232, 240], alpha);
    circle(
        canvas,
        w,
        h,
        cx,
        cy,
        (hub_r / 2).max(1),
        [248, 250, 252],
        alpha,
    );
    circle_ring(canvas, w, h, cx, cy, hub_r, [100, 116, 139], alpha);
}

fn draw_play_pause(
    c: &mut [u8],
    w: i32,
    h: i32,
    cx: i32,
    cy: i32,
    playing: bool,
    color: [u8; 3],
    alpha: f32,
) {
    if playing {
        rect(c, w, h, cx - 5, cy - 7, 3, 14, color, alpha);
        rect(c, w, h, cx + 2, cy - 7, 3, 14, color, alpha);
    } else {
        line(c, w, h, cx - 4, cy - 7, cx + 7, cy, color, alpha);
        line(c, w, h, cx + 7, cy, cx - 4, cy + 7, color, alpha);
        line(c, w, h, cx - 4, cy + 7, cx - 4, cy - 7, color, alpha);
    }
}

fn draw_shuffle(c: &mut [u8], w: i32, h: i32, x: i32, y: i32, color: [u8; 3], a: f32) {
    line(c, w, h, x - 8, y - 5, x + 7, y + 5, color, a);
    line(c, w, h, x - 8, y + 5, x + 7, y - 5, color, a);
    line(c, w, h, x + 3, y - 5, x + 7, y - 5, color, a);
    line(c, w, h, x + 7, y - 5, x + 7, y - 1, color, a);
    line(c, w, h, x + 3, y + 5, x + 7, y + 5, color, a);
    line(c, w, h, x + 7, y + 1, x + 7, y + 5, color, a);
}

fn draw_repeat(c: &mut [u8], w: i32, h: i32, x: i32, y: i32, color: [u8; 3], a: f32) {
    line(c, w, h, x - 7, y - 4, x + 6, y - 4, color, a);
    line(c, w, h, x + 6, y - 4, x + 6, y, color, a);
    line(c, w, h, x + 6, y + 4, x - 6, y + 4, color, a);
    line(c, w, h, x - 6, y + 4, x - 6, y, color, a);
    line(c, w, h, x + 3, y - 7, x + 6, y - 4, color, a);
    line(c, w, h, x + 6, y - 4, x + 3, y - 1, color, a);
    line(c, w, h, x - 3, y + 1, x - 6, y + 4, color, a);
    line(c, w, h, x - 6, y + 4, x - 3, y + 7, color, a);
}

fn draw_speaker(c: &mut [u8], w: i32, h: i32, x: i32, y: i32, color: [u8; 3], a: f32) {
    rect(c, w, h, x, y - 3, 3, 6, color, a);
    line(c, w, h, x + 3, y - 3, x + 8, y - 7, color, a);
    line(c, w, h, x + 8, y - 7, x + 8, y + 7, color, a);
    line(c, w, h, x + 8, y + 7, x + 3, y + 3, color, a);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_player_sits_at_the_right_edge_of_its_left_side_surface() {
        assert_eq!(PLAYER_WIDTH, 320);
        assert_eq!(PLAYER_HEIGHT, 222);
        assert_eq!(COMPACT_X + COMPACT_WIDTH, PLAYER_WIDTH);
        assert_eq!(DISC_CENTER_X, COMPACT_X + COMPACT_WIDTH / 2);
        assert!(compact_region_contains(DISC_CENTER_X, DISC_CENTER_Y));
        assert!(!compact_region_contains(COMPACT_X - 1, DISC_CENTER_Y));
    }

    #[test]
    fn compact_and_expanded_transport_hitboxes_match_the_drawn_controls() {
        assert_eq!(
            control_at(COMPACT_X + 40, COMPACT_Y + 79, false),
            Some(MediaControl::Previous)
        );
        assert_eq!(
            control_at(COMPACT_X + 80, COMPACT_Y + 79, false),
            Some(MediaControl::PlayPause)
        );
        assert_eq!(
            control_at(COMPACT_X + 120, COMPACT_Y + 79, false),
            Some(MediaControl::Next)
        );
        assert_eq!(control_at(108, 139, true), Some(MediaControl::Previous));
        assert_eq!(control_at(160, 139, true), Some(MediaControl::PlayPause));
        assert_eq!(control_at(212, 139, true), Some(MediaControl::Next));
    }
}

fn truncate(value: &str, max: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max {
        value.to_string()
    } else {
        format!(
            "{}…",
            chars
                .iter()
                .take(max.saturating_sub(1))
                .copied()
                .collect::<String>()
        )
    }
}
fn time_label(seconds: f64) -> String {
    format!("{}:{:02}", seconds as u64 / 60, seconds as u64 % 60)
}
fn index(w: i32, x: i32, y: i32) -> usize {
    ((y * w + x) * 4) as usize
}
fn blend(c: &mut [u8], w: i32, h: i32, x: i32, y: i32, rgb: [u8; 3], alpha: f32) {
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let i = index(w, x, y);
    if i + 3 >= c.len() {
        return;
    }
    let a = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    c[i] = (rgb[2] as f32 * a + c[i] as f32 * inv) as u8;
    c[i + 1] = (rgb[1] as f32 * a + c[i + 1] as f32 * inv) as u8;
    c[i + 2] = (rgb[0] as f32 * a + c[i + 2] as f32 * inv) as u8;
    c[i + 3] = (255.0 * a + c[i + 3] as f32 * inv) as u8;
}
fn rect(c: &mut [u8], w: i32, h: i32, x: i32, y: i32, rw: i32, rh: i32, rgb: [u8; 3], a: f32) {
    for py in y..y + rh {
        for px in x..x + rw {
            blend(c, w, h, px, py, rgb, a);
        }
    }
}
fn line(c: &mut [u8], w: i32, h: i32, x0: i32, y0: i32, x1: i32, y1: i32, rgb: [u8; 3], a: f32) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    for n in 0..=steps {
        let t = n as f32 / steps as f32;
        blend(
            c,
            w,
            h,
            (x0 as f32 + (x1 - x0) as f32 * t).round() as i32,
            (y0 as f32 + (y1 - y0) as f32 * t).round() as i32,
            rgb,
            a,
        );
    }
}
fn circle(c: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, r: i32, rgb: [u8; 3], a: f32) {
    for y in -r..=r {
        for x in -r..=r {
            if x * x + y * y <= r * r {
                blend(c, w, h, cx + x, cy + y, rgb, a);
            }
        }
    }
}
fn circle_ring(c: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, r: i32, rgb: [u8; 3], a: f32) {
    let inner = (r - 1).max(0);
    for y in -r..=r {
        for x in -r..=r {
            let d = x * x + y * y;
            if d <= r * r && d >= inner * inner {
                blend(c, w, h, cx + x, cy + y, rgb, a);
            }
        }
    }
}
fn rounded(
    c: &mut [u8],
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    rw: i32,
    rh: i32,
    r: i32,
    rgb: [u8; 3],
    a: f32,
) {
    for py in 0..rh {
        for px in 0..rw {
            let cx = if px < r {
                r
            } else if px >= rw - r {
                rw - r - 1
            } else {
                px
            };
            let cy = if py < r {
                r
            } else if py >= rh - r {
                rh - r - 1
            } else {
                py
            };
            let dx = px - cx;
            let dy = py - cy;
            if (px < r || px >= rw - r) && (py < r || py >= rh - r) && dx * dx + dy * dy > r * r {
                continue;
            }
            blend(c, w, h, x + px, y + py, rgb, a);
        }
    }
}
fn chevron(c: &mut [u8], w: i32, h: i32, x: i32, y: i32, next: bool, rgb: [u8; 3], a: f32) {
    let (x1, x2) = if next { (x - 4, x + 4) } else { (x + 4, x - 4) };
    line(c, w, h, x1, y - 5, x2, y, rgb, a);
    line(c, w, h, x2, y, x1, y + 5, rgb, a);
}
