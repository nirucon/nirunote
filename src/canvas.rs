//! A client-side framebuffer (0x00RRGGBB per pixel) with the few drawing
//! operations a file manager needs. Everything is drawn here and sent to the
//! window in one copy (MIT-SHM when the server offers it), so a redraw never
//! flickers and costs one memory copy instead of many X requests.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        Rect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }
    pub fn inset(&self, d: i32) -> Rect {
        Rect::new(
            self.x + d,
            self.y + d,
            (self.w - 2 * d).max(0),
            (self.h - 2 * d).max(0),
        )
    }
}

/// Premultiplied RGBA image (as tiny-skia produces it), row-major.
pub struct Image {
    pub w: i32,
    pub h: i32,
    pub data: Vec<u8>,
}

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
    clip: Rect,
}

impl Canvas {
    pub fn new(w: i32, h: i32) -> Self {
        let w = w.max(1);
        let h = h.max(1);
        Self {
            w,
            h,
            px: vec![0; (w * h) as usize],
            clip: Rect::new(0, 0, w, h),
        }
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        let w = w.max(1);
        let h = h.max(1);
        if w != self.w || h != self.h {
            self.w = w;
            self.h = h;
            self.px = vec![0; (w * h) as usize];
        }
        self.clip = Rect::new(0, 0, w, h);
    }

    pub fn set_clip(&mut self, r: Rect) {
        self.clip = r.intersect(&Rect::new(0, 0, self.w, self.h));
    }

    pub fn reset_clip(&mut self) {
        self.clip = Rect::new(0, 0, self.w, self.h);
    }

    pub fn fill(&mut self, r: Rect, color: u32) {
        let r = r.intersect(&self.clip);
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        for y in r.y..r.y + r.h {
            let row = (y * self.w) as usize;
            self.px[row + r.x as usize..row + (r.x + r.w) as usize].fill(color);
        }
    }

    /// A one pixel frame inside `r`.
    pub fn frame(&mut self, r: Rect, color: u32) {
        self.fill(Rect::new(r.x, r.y, r.w, 1), color);
        self.fill(Rect::new(r.x, r.y + r.h - 1, r.w, 1), color);
        self.fill(Rect::new(r.x, r.y, 1, r.h), color);
        self.fill(Rect::new(r.x + r.w - 1, r.y, 1, r.h), color);
    }

    /// Blend `color` with coverage 0..=255 at one pixel (already clipped).
    #[inline]
    fn blend_at(&mut self, i: usize, color: u32, a: u32) {
        if a == 0 {
            return;
        }
        if a >= 255 {
            self.px[i] = color;
            return;
        }
        let d = self.px[i];
        let inv = 255 - a;
        let ch = |s: u32, d: u32| (s * a + d * inv + 127) / 255;
        self.px[i] = (ch((color >> 16) & 0xff, (d >> 16) & 0xff) << 16)
            | (ch((color >> 8) & 0xff, (d >> 8) & 0xff) << 8)
            | ch(color & 0xff, d & 0xff);
    }

    /// Draw a coverage mask (one byte per pixel) in `color` at (x, y).
    pub fn mask(&mut self, x: i32, y: i32, w: i32, h: i32, coverage: &[u8], color: u32) {
        let area = Rect::new(x, y, w, h).intersect(&self.clip);
        for py in area.y..area.y + area.h {
            let src_row = ((py - y) * w) as usize;
            let dst_row = (py * self.w) as usize;
            for px in area.x..area.x + area.w {
                let a = coverage[src_row + (px - x) as usize] as u32;
                self.blend_at(dst_row + px as usize, color, a);
            }
        }
    }

    /// Draw a premultiplied RGBA image at (x, y), scaled by `alpha` (0..=255).
    pub fn image(&mut self, x: i32, y: i32, img: &Image, alpha: u32) {
        let area = Rect::new(x, y, img.w, img.h).intersect(&self.clip);
        for py in area.y..area.y + area.h {
            let src_row = ((py - y) * img.w) as usize * 4;
            let dst_row = (py * self.w) as usize;
            for px in area.x..area.x + area.w {
                let s = src_row + (px - x) as usize * 4;
                let (r, g, b, a) = (
                    img.data[s] as u32,
                    img.data[s + 1] as u32,
                    img.data[s + 2] as u32,
                    img.data[s + 3] as u32,
                );
                let (r, g, b, a) = if alpha >= 255 {
                    (r, g, b, a)
                } else {
                    (
                        r * alpha / 255,
                        g * alpha / 255,
                        b * alpha / 255,
                        a * alpha / 255,
                    )
                };
                if a == 0 {
                    continue;
                }
                let i = dst_row + px as usize;
                let d = self.px[i];
                let inv = 255 - a;
                let ch = |s: u32, d: u32| (s + (d * inv + 127) / 255).min(255);
                self.px[i] = (ch(r, (d >> 16) & 0xff) << 16)
                    | (ch(g, (d >> 8) & 0xff) << 8)
                    | ch(b, d & 0xff);
            }
        }
    }

    /// Blend a translucent rectangle (selection rubber band, hover).
    pub fn tint(&mut self, r: Rect, color: u32, a: u32) {
        let r = r.intersect(&self.clip);
        for py in r.y..r.y + r.h {
            let row = (py * self.w) as usize;
            for px in r.x..r.x + r.w {
                self.blend_at(row + px as usize, color, a);
            }
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: u32 has no padding; the slice covers exactly the pixels.
        unsafe { std::slice::from_raw_parts(self.px.as_ptr() as *const u8, self.px.len() * 4) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_is_clipped_to_the_canvas_and_the_clip_rect() {
        let mut c = Canvas::new(10, 10);
        c.fill(Rect::new(-5, -5, 8, 8), 0xffffff);
        assert_eq!(c.px[0], 0xffffff);
        assert_eq!(c.px[3], 0);
        c.set_clip(Rect::new(5, 5, 5, 5));
        c.fill(Rect::new(0, 0, 10, 10), 0x123456);
        assert_eq!(c.px[4 * 10 + 4], 0);
        assert_eq!(c.px[5 * 10 + 5], 0x123456);
    }

    #[test]
    fn half_coverage_blends_halfway() {
        let mut c = Canvas::new(1, 1);
        c.mask(0, 0, 1, 1, &[128], 0xffffff);
        let v = c.px[0] & 0xff;
        assert!((126..=130).contains(&v), "{v}");
    }
}
