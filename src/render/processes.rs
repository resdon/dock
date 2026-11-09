//! Process panel renderer.
//!
//! Renders the paginated process list with CPU and memory columns.

use crate::models::{ProcessSnapshot, ProcessSort};
use crate::render::font::FontManager;
use crate::render::text::draw_text;
use crate::render::theme;

pub const PROCESS_POPUP_WIDTH: i32 = 300;
pub const PROCESS_POPUP_HEIGHT: i32 = 280;
pub const HEADER_MINIMIZE_X: i32 = 272;
pub const HEADER_MINIMIZE_Y: i32 = 4;
pub const HEADER_MINIMIZE_W: i32 = 20;
pub const HEADER_MINIMIZE_H: i32 = 16;
pub const ROW_START_Y: i32 = 57;
pub const ROW_HEIGHT: i32 = 28;
pub const ROWS_PER_PAGE: usize = 6;
pub const CPU_SORT_HITBOX: (i32, i32, i32, i32) = (192, 36, 43, 17);
pub const MEM_SORT_HITBOX: (i32, i32, i32, i32) = (238, 36, 48, 17);
pub const PREV_BUTTON: (i32, i32, i32, i32) = (226, 254, 30, 17);
pub const NEXT_BUTTON: (i32, i32, i32, i32) = (260, 254, 34, 17);

fn rect(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: [u8; 4],
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(width);
    let y1 = (y + h).min(height);
    for py in y0..y1 {
        for px in x0..x1 {
            let index = ((py * width + px) * 4) as usize;
            if index + 3 < canvas.len() {
                canvas[index..index + 4].copy_from_slice(&color);
            }
        }
    }
}
fn label(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    font: &mut FontManager,
    text: &str,
    size: i32,
    x: i32,
    baseline: i32,
    color: (u8, u8, u8),
) {
    draw_text(
        canvas,
        width,
        height,
        font,
        text,
        size.max(7),
        x,
        baseline,
        color,
    );
}
fn short(value: &str, max: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max {
        value.to_string()
    } else {
        format!(
            "{}…",
            chars.iter().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}
fn format_bytes(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = 0;
    let units = ["B", "K", "M", "G", "T"];
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}{}", value as u64, units[unit])
    } else {
        format!("{:.1}{}", value, units[unit])
    }
}
fn button(canvas: &mut [u8], width: i32, height: i32, hitbox: (i32, i32, i32, i32), active: bool) {
    theme::fill_rounded(
        canvas,
        width,
        height,
        hitbox.0,
        hitbox.1,
        hitbox.2,
        hitbox.3,
        7,
        if active { theme::YELLOW_ARGB } else { theme::INSET_ARGB },
    );
    theme::stroke_rounded(
        canvas,
        width,
        height,
        hitbox.0,
        hitbox.1,
        hitbox.2,
        hitbox.3,
        7,
        1,
        if active {
            theme::YELLOW_SOFT_ARGB
        } else {
            theme::BORDER_ARGB
        },
    );
}

fn dot(canvas: &mut [u8], width: i32, height: i32, cx: i32, cy: i32, radius: i32, color: [u8; 4]) {
    for y in -radius..=radius {
        for x in -radius..=radius {
            if x * x + y * y <= radius * radius {
                let px = cx + x;
                let py = cy + y;
                if px >= 0 && py >= 0 && px < width && py < height {
                    let i = ((py * width + px) * 4) as usize;
                    if i + 3 < canvas.len() {
                        canvas[i..i + 4].copy_from_slice(&color);
                    }
                }
            }
        }
    }
}

pub fn render_process_surface(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    scale: f32,
    snapshot: &ProcessSnapshot,
    sort: ProcessSort,
    page: usize,
    font: &mut FontManager,
) {
    let w = width as i32;
    let h = height as i32;
    if w <= 0 || h <= 0 || canvas.len() < width as usize * height as usize * 4 {
        return;
    }
    canvas.fill(0);
    let s = |v: f32| (v * scale).round() as i32;
    theme::fill_rounded(canvas, w, h, 0, 0, w, h, s(14.0), theme::PANEL_ARGB);
    theme::stroke_rounded(
        canvas,
        w,
        h,
        0,
        0,
        w,
        h,
        s(14.0),
        s(1.0).max(1),
        theme::BORDER_ARGB,
    );
    rect(canvas, w, h, s(8.0), s(7.0), s(24.0), s(3.0), theme::YELLOW_ARGB);
    label(
        canvas,
        w,
        h,
        font,
        "Running processes",
        s(12.0),
        s(8.0),
        s(20.0),
        theme::TEXT,
    );
    theme::fill_rounded(
        canvas,
        w,
        h,
        s(HEADER_MINIMIZE_X as f32),
        s(HEADER_MINIMIZE_Y as f32),
        s(HEADER_MINIMIZE_W as f32),
        s(HEADER_MINIMIZE_H as f32),
        s(6.0),
        theme::RAISED_ARGB,
    );
    rect(
        canvas,
        w,
        h,
        s((HEADER_MINIMIZE_X + 6) as f32),
        s((HEADER_MINIMIZE_Y + 8) as f32),
        s(8.0),
        s(2.0),
        theme::YELLOW_ARGB,
    );
    label(
        canvas,
        w,
        h,
        font,
        &format!("{} processes", snapshot.total_processes),
        s(7.0),
        s(8.0),
        s(34.0),
        theme::MUTED,
    );
    rect(
        canvas,
        w,
        h,
        s(8.0),
        s(42.0),
        s(284.0),
        s(1.0).max(1),
        theme::HAIRLINE_ARGB,
    );

    let cpu_hit = (
        s(CPU_SORT_HITBOX.0 as f32),
        s(CPU_SORT_HITBOX.1 as f32),
        s(CPU_SORT_HITBOX.2 as f32),
        s(CPU_SORT_HITBOX.3 as f32),
    );
    let mem_hit = (
        s(MEM_SORT_HITBOX.0 as f32),
        s(MEM_SORT_HITBOX.1 as f32),
        s(MEM_SORT_HITBOX.2 as f32),
        s(MEM_SORT_HITBOX.3 as f32),
    );
    button(canvas, w, h, cpu_hit, sort == ProcessSort::Cpu);
    button(canvas, w, h, mem_hit, sort == ProcessSort::Memory);
    dot(
        canvas,
        w,
        h,
        cpu_hit.0 + s(8.0),
        cpu_hit.1 + cpu_hit.3 / 2,
        s(4.0).max(2),
        if sort == ProcessSort::Cpu {
            theme::RED_ARGB
        } else {
            theme::BORDER_ARGB
        },
    );
    dot(
        canvas,
        w,
        h,
        mem_hit.0 + s(8.0),
        mem_hit.1 + mem_hit.3 / 2,
        s(4.0).max(2),
        if sort == ProcessSort::Memory {
            theme::RED_ARGB
        } else {
            theme::BORDER_ARGB
        },
    );
    label(
        canvas,
        w,
        h,
        font,
        "CPU",
        s(7.0),
        s(203.0),
        s(48.0),
        if sort == ProcessSort::Cpu {
            theme::INK
        } else {
            theme::TEXT
        },
    );
    label(
        canvas,
        w,
        h,
        font,
        "RAM",
        s(7.0),
        s(249.0),
        s(48.0),
        if sort == ProcessSort::Memory {
            theme::INK
        } else {
            theme::TEXT
        },
    );

    let mut processes = snapshot.processes.clone();
    match sort {
        ProcessSort::Cpu => processes.sort_by(|a, b| {
            b.cpu_percent
                .total_cmp(&a.cpu_percent)
                .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
        }),
        ProcessSort::Memory => processes.sort_by(|a, b| {
            b.memory_bytes
                .cmp(&a.memory_bytes)
                .then_with(|| b.cpu_percent.total_cmp(&a.cpu_percent))
        }),
    }
    let page_count = ((processes.len() + ROWS_PER_PAGE - 1) / ROWS_PER_PAGE).max(1);
    let page = page.min(page_count - 1);
    let start = page * ROWS_PER_PAGE;
    for (index, process) in processes.iter().skip(start).take(ROWS_PER_PAGE).enumerate() {
        let y = ROW_START_Y + index as i32 * ROW_HEIGHT;
        theme::fill_rounded(
            canvas,
            w,
            h,
            s(6.0),
            s(y as f32),
            s(288.0),
            s(25.0),
            s(7.0),
            theme::INSET_ARGB,
        );
        label(
            canvas,
            w,
            h,
            font,
            &short(&process.name, 22),
            s(8.0),
            s(10.0),
            s((y + 10) as f32),
            theme::TEXT,
        );
        label(
            canvas,
            w,
            h,
            font,
            &format!("PID {}", process.pid),
            s(6.0),
            s(10.0),
            s((y + 21) as f32),
            theme::MUTED,
        );
        label(
            canvas,
            w,
            h,
            font,
            &format!("{:.1}%", process.cpu_percent),
            s(7.0),
            s(198.0),
            s((y + 16) as f32),
            (247, 208, 95),
        );
        label(
            canvas,
            w,
            h,
            font,
            &format_bytes(process.memory_bytes),
            s(7.0),
            s(244.0),
            s((y + 16) as f32),
            (226, 218, 183),
        );
    }
    if snapshot.total_processes == 0 {
        label(
            canvas,
            w,
            h,
            font,
            "No process data available",
            s(8.0),
            s(10.0),
            s(80.0),
            (170, 180, 192),
        );
    }
    label(
        canvas,
        w,
        h,
        font,
        &format!("Page {}/{}", page + 1, page_count),
        s(7.0),
        s(8.0),
        s(267.0),
        (132, 142, 156),
    );
    if page > 0 {
        button(
            canvas,
            w,
            h,
            (
                s(PREV_BUTTON.0 as f32),
                s(PREV_BUTTON.1 as f32),
                s(PREV_BUTTON.2 as f32),
                s(PREV_BUTTON.3 as f32),
            ),
            true,
        );
    }
    if page + 1 < page_count {
        button(
            canvas,
            w,
            h,
            (
                s(NEXT_BUTTON.0 as f32),
                s(NEXT_BUTTON.1 as f32),
                s(NEXT_BUTTON.2 as f32),
                s(NEXT_BUTTON.3 as f32),
            ),
            true,
        );
    }
    label(
        canvas,
        w,
        h,
        font,
        "<",
        s(8.0),
        s(237.0),
        s(267.0),
        (224, 232, 241),
    );
    label(
        canvas,
        w,
        h,
        font,
        ">",
        s(8.0),
        s(273.0),
        s(267.0),
        (224, 232, 241),
    );
}

pub fn minimize_button_at(x: f64, y: f64) -> bool {
    let x = x.round() as i32;
    let y = y.round() as i32;
    (HEADER_MINIMIZE_X..HEADER_MINIMIZE_X + HEADER_MINIMIZE_W).contains(&x)
        && (HEADER_MINIMIZE_Y..HEADER_MINIMIZE_Y + HEADER_MINIMIZE_H).contains(&y)
}
pub fn page_delta_at(x: f64, y: f64) -> Option<i32> {
    let x = x.round() as i32;
    let y = y.round() as i32;
    if (PREV_BUTTON.1..PREV_BUTTON.1 + PREV_BUTTON.3).contains(&y)
        && (PREV_BUTTON.0..PREV_BUTTON.0 + PREV_BUTTON.2).contains(&x)
    {
        Some(-1)
    } else if (NEXT_BUTTON.1..NEXT_BUTTON.1 + NEXT_BUTTON.3).contains(&y)
        && (NEXT_BUTTON.0..NEXT_BUTTON.0 + NEXT_BUTTON.2).contains(&x)
    {
        Some(1)
    } else {
        None
    }
}
pub fn sort_at(x: f64, y: f64) -> Option<ProcessSort> {
    let x = x.round() as i32;
    let y = y.round() as i32;
    if (CPU_SORT_HITBOX.1..CPU_SORT_HITBOX.1 + CPU_SORT_HITBOX.3).contains(&y)
        && (CPU_SORT_HITBOX.0..CPU_SORT_HITBOX.0 + CPU_SORT_HITBOX.2).contains(&x)
    {
        Some(ProcessSort::Cpu)
    } else if (MEM_SORT_HITBOX.1..MEM_SORT_HITBOX.1 + MEM_SORT_HITBOX.3).contains(&y)
        && (MEM_SORT_HITBOX.0..MEM_SORT_HITBOX.0 + MEM_SORT_HITBOX.2).contains(&x)
    {
        Some(ProcessSort::Memory)
    } else {
        None
    }
}
