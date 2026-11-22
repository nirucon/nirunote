//! Header icons drawn as pixels (menu, copy, Preview, focus, save, a check
//! mark for feedback, the unsaved dot) and the window icon from the
//! NIRUNOTE SVG.
use crate::canvas::{Canvas, Rect};

fn line(cv: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, t: i32, color: u32) {
    let n = ((x1 - x0).abs().max((y1 - y0).abs()) as i32).max(1);
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let x = x0 + (x1 - x0) * f;
        let y = y0 + (y1 - y0) * f;
        cv.fill(Rect::new(x.round() as i32, y.round() as i32, t, t), color);
    }
}

pub fn dot(cv: &mut Canvas, cx: i32, cy: i32, r: i32, color: u32) {
    for dy in -r..=r {
        for dx in -r..=r {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d <= r as f32 + 0.5 {
                let a = ((r as f32 + 0.5 - d).clamp(0.0, 1.0) * 255.0) as u32;
                cv.tint(Rect::new(cx + dx, cy + dy, 1, 1), color, a);
            }
        }
    }
}

/// 0 menu, 1 copy, 2 Preview, 3 focus, 4 save, 9 check mark.
pub fn icon(cv: &mut Canvas, kind: u8, r: Rect, color: u32, scale: f32) {
    let u = (scale * 1.0).max(1.0);
    let t = u.round() as i32;
    let cx = r.x as f32 + r.w as f32 / 2.0;
    let cy = r.y as f32 + r.h as f32 / 2.0;
    let s = 7.0 * u;
    match kind {
        0 => {
            for k in -1..=1 {
                let y = (cy + k as f32 * 4.0 * u).round() as i32;
                cv.fill(
                    Rect::new((cx - s).round() as i32, y, (2.0 * s) as i32, t),
                    color,
                );
            }
        }
        1 => {
            let a = Rect::new(
                (cx - s + 3.0 * u) as i32,
                (cy - s) as i32,
                (s * 1.5) as i32,
                (s * 1.5) as i32,
            );
            let b = Rect::new(
                (cx - s) as i32,
                (cy - s + 3.0 * u) as i32,
                (s * 1.5) as i32,
                (s * 1.5) as i32,
            );
            cv.frame(a, color);
            cv.frame(b, color);
        }
        2 => {
            for i in 0..40 {
                let a = i as f32 / 40.0 * std::f32::consts::TAU;
                let x = cx + a.cos() * s * 0.85;
                let y = cy + a.sin() * s * 0.55;
                cv.fill(Rect::new(x.round() as i32, y.round() as i32, t, t), color);
            }
            dot(cv, cx as i32, cy as i32, (2.0 * u) as i32, color);
        }
        3 => {
            let (x0, y0, x1, y1) = (cx - s, cy - s, cx + s, cy + s);
            let l = 4.0 * u;
            for (ax, ay, dx, dy) in [
                (x0, y0, 1.0, 1.0),
                (x1, y0, -1.0, 1.0),
                (x0, y1, 1.0, -1.0),
                (x1, y1, -1.0, -1.0),
            ] {
                line(cv, ax, ay, ax + dx * l, ay, t, color);
                line(cv, ax, ay, ax, ay + dy * l, t, color);
            }
        }
        4 => {
            let b = Rect::new(
                (cx - s) as i32,
                (cy - s) as i32,
                (2.0 * s) as i32,
                (2.0 * s) as i32,
            );
            cv.frame(b, color);
            cv.frame(
                Rect::new(
                    b.x + (3.0 * u) as i32,
                    b.y,
                    b.w - (6.0 * u) as i32,
                    (5.0 * u) as i32,
                ),
                color,
            );
            cv.fill(
                Rect::new(
                    b.x + (4.0 * u) as i32,
                    b.y + b.h - (5.0 * u) as i32,
                    b.w - (8.0 * u) as i32,
                    t,
                ),
                color,
            );
        }
        _ => {
            line(
                cv,
                cx - s * 0.8,
                cy,
                cx - s * 0.2,
                cy + s * 0.6,
                t + 1,
                color,
            );
            line(
                cv,
                cx - s * 0.2,
                cy + s * 0.6,
                cx + s * 0.9,
                cy - s * 0.6,
                t + 1,
                color,
            );
        }
    }
}

const SVG: &[u8] = include_bytes!("../assets/nirunote.svg");

/// ARGB window icons rendered from the NIRUNOTE SVG.
pub fn window_icons() -> Vec<(u32, u32, Vec<u32>)> {
    let opt = resvg::usvg::Options::default();
    let Ok(tree) = resvg::usvg::Tree::from_data(SVG, &opt) else {
        return Vec::new();
    };
    [16u32, 32, 48]
        .iter()
        .filter_map(|&s| {
            let mut pm = resvg::tiny_skia::Pixmap::new(s, s)?;
            let sz = tree.size();
            let t = resvg::tiny_skia::Transform::from_scale(
                s as f32 / sz.width(),
                s as f32 / sz.height(),
            );
            resvg::render(&tree, t, &mut pm.as_mut());
            let px = pm
                .data()
                .chunks_exact(4)
                .map(|c| {
                    let a = c[3] as u32;
                    let un = |v: u8| (v as u32 * 255).checked_div(a).unwrap_or(0).min(255);
                    (a << 24) | (un(c[0]) << 16) | (un(c[1]) << 8) | un(c[2])
                })
                .collect();
            Some((s, s, px))
        })
        .collect()
}
