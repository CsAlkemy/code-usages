// Tray ring rendering: replaces the Electron app's offscreen-canvas window.
// Draws at 2x (44px) for a crisp 22pt menu-bar icon.
use tiny_skia::{Color, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub const SIZE: u32 = 44;

fn color_for(pct: i64, dark: bool) -> Color {
    // Same thresholds as the Electron config.js.
    let (light, dk) = if pct < 60 {
        ((0x0F, 0x9D, 0x6C), (0x33, 0xC8, 0x9A))
    } else if pct < 85 {
        ((0xC9, 0x82, 0x0F), (0xE6, 0xA9, 0x3B))
    } else {
        ((0xD6, 0x45, 0x45), (0xF0, 0x6B, 0x6B))
    };
    let (r, g, b) = if dark { dk } else { light };
    Color::from_rgba8(r, g, b, 255)
}

fn arc_path(cx: f32, cy: f32, r: f32, from: f32, to: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    let steps = 64;
    for i in 0..=steps {
        let t = from + (to - from) * (i as f32) / (steps as f32);
        let (x, y) = (cx + r * t.cos(), cy + r * t.sin());
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    pb.finish()
}

// Returns un-premultiplied RGBA bytes, SIZE x SIZE.
pub fn draw(pct: Option<i64>, dark: bool) -> Vec<u8> {
    let mut pixmap = Pixmap::new(SIZE, SIZE).expect("pixmap");
    let s = SIZE as f32;
    let lw = 6.5_f32;
    let r = s / 2.0 - lw / 2.0 - 2.0;
    let (cx, cy) = (s / 2.0, s / 2.0);
    let tau = std::f32::consts::TAU;

    let stroke = Stroke { width: lw, line_cap: LineCap::Round, ..Stroke::default() };

    let mut track = Paint::default();
    track.set_color(Color::from_rgba8(140, 140, 140, 128));
    track.anti_alias = true;
    if let Some(path) = arc_path(cx, cy, r, 0.0, tau) {
        pixmap.stroke_path(&path, &track, &stroke, Transform::identity(), None);
    }

    let p = (pct.unwrap_or(0).clamp(0, 100) as f32) / 100.0;
    if p > 0.0 {
        let mut fill = Paint::default();
        fill.set_color(color_for(pct.unwrap_or(0), dark));
        fill.anti_alias = true;
        let start = -tau / 4.0; // 12 o'clock
        if let Some(path) = arc_path(cx, cy, r, start, start + p * tau) {
            pixmap.stroke_path(&path, &fill, &stroke, Transform::identity(), None);
        }
    }

    // tiny-skia stores premultiplied alpha; tray images want straight RGBA.
    pixmap
        .pixels()
        .iter()
        .flat_map(|px| {
            let c = px.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}
