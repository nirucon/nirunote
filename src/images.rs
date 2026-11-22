//! Images of the Preview: local files (PNG, JPEG with EXIF orientation, GIF,
//! WebP, SVG) decoded when the Preview lays them out, at most 32 MB, kept
//! at most 1600 pixels wide; remote http(s) images fetched in a worker
//! thread with `curl` (or `wget`), at most 8 MB and 10 s, only while the
//! Preview shows them and only when "Remote images in Preview" is on.
use crate::canvas::Image;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};

pub const MAX_LOCAL_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_REMOTE_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const KEEP_WIDTH: u32 = 1600;
const CACHE_ENTRIES: usize = 64;

pub enum Slot {
    Ready(std::sync::Arc<Image>),
    Failed,
    Pending,
}

pub struct Images {
    map: HashMap<String, Slot>,
    order: Vec<String>,
    scaled: HashMap<(String, i32), std::sync::Arc<Image>>,
    pending: HashSet<String>,
    tx: Sender<(String, Option<Image>)>,
    rx: Receiver<(String, Option<Image>)>,
    pub wake: i32,
    pub remote: bool,
}

impl Images {
    pub fn new(wake: i32, remote: bool) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Images {
            map: HashMap::new(),
            order: Vec::new(),
            scaled: HashMap::new(),
            pending: HashSet::new(),
            tx,
            rx,
            wake,
            remote,
        }
    }

    pub fn reset(&mut self) {
        self.map.clear();
        self.order.clear();
        self.scaled.clear();
    }

    fn put(&mut self, key: String, slot: Slot) {
        if !self.map.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.map.insert(key, slot);
        while self.order.len() > CACHE_ENTRIES {
            let old = self.order.remove(0);
            self.map.remove(&old);
            self.scaled.retain(|k, _| k.0 != old);
        }
    }

    /// Images that finished loading in the background; true if any.
    pub fn collect(&mut self) -> bool {
        let mut any = false;
        while let Ok((key, img)) = self.rx.try_recv() {
            self.pending.remove(&key);
            let slot = match img {
                Some(i) => Slot::Ready(std::sync::Arc::new(i)),
                None => Slot::Failed,
            };
            self.put(key, slot);
            any = true;
        }
        any
    }

    /// The image for `src` (relative to `base`), loading a local one now
    /// and starting the download of a remote one.
    pub fn get(&mut self, src: &str, base: Option<&Path>) -> &Slot {
        if !self.map.contains_key(src) {
            let lower = src.to_ascii_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                if self.remote {
                    if !self.pending.contains(src) {
                        self.pending.insert(src.to_string());
                        let (tx, key, wake) = (self.tx.clone(), src.to_string(), self.wake);
                        std::thread::spawn(move || {
                            let img = fetch(&key).and_then(|d| decode(&d, &key));
                            let _ = tx.send((key, img));
                            wake_up(wake);
                        });
                    }
                    return &Slot::Pending;
                }
                self.put(src.to_string(), Slot::Failed);
            } else {
                let img = local_path(src, base).and_then(|p| {
                    let meta = std::fs::metadata(&p).ok()?;
                    if !meta.is_file() || meta.len() > MAX_LOCAL_IMAGE_BYTES {
                        return None;
                    }
                    let data = std::fs::read(&p).ok()?;
                    decode(&data, &p.to_string_lossy())
                });
                let slot = match img {
                    Some(i) => Slot::Ready(std::sync::Arc::new(i)),
                    None => Slot::Failed,
                };
                self.put(src.to_string(), slot);
            }
        }
        self.map.get(src).unwrap_or(&Slot::Failed)
    }

    /// The image scaled to `w` pixels wide (cached).
    pub fn scaled(&mut self, src: &str, w: i32) -> Option<std::sync::Arc<Image>> {
        let Some(Slot::Ready(img)) = self.map.get(src) else {
            return None;
        };
        if img.w == w {
            return Some(img.clone());
        }
        let key = (src.to_string(), w);
        if let Some(s) = self.scaled.get(&key) {
            return Some(s.clone());
        }
        let h = ((img.h as i64 * w as i64) / img.w.max(1) as i64).max(1) as i32;
        let s = std::sync::Arc::new(resize(img, w, h));
        if self.scaled.len() > 64 {
            self.scaled.clear();
        }
        self.scaled.insert(key, s.clone());
        Some(s)
    }
}

pub fn wake_up(fd: i32) {
    if fd >= 0 {
        let v: u64 = 1;
        unsafe { libc::write(fd, &v as *const u64 as *const libc::c_void, 8) };
    }
}

fn local_path(src: &str, base: Option<&Path>) -> Option<PathBuf> {
    let s = src.strip_prefix("file://").unwrap_or(src);
    let s = s.split(['?', '#']).next().unwrap_or(s);
    let p = crate::x::decode_path(s);
    if p.is_absolute() {
        Some(p)
    } else {
        base.map(|b| b.join(p))
    }
}

/// Download with a size and time limit (curl, else wget).
fn fetch(url: &str) -> Option<Vec<u8>> {
    let max = MAX_REMOTE_IMAGE_BYTES.to_string();
    let out = std::process::Command::new("curl")
        .args([
            "-sSfL",
            "--proto",
            "=http,https",
            "--max-time",
            "10",
            "--max-filesize",
            &max,
            "-A",
            "NIRUNOTE preview",
            "--",
            url,
        ])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| o.stdout)
        .or_else(|| {
            std::process::Command::new("wget")
                .args([
                    "-q",
                    "-T",
                    "10",
                    "-U",
                    "NIRUNOTE preview",
                    "-O",
                    "-",
                    "--",
                    url,
                ])
                .stderr(std::process::Stdio::null())
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| o.stdout)
        })?;
    (!out.is_empty() && out.len() as u64 <= MAX_REMOTE_IMAGE_BYTES).then_some(out)
}

fn looks_like_svg(data: &[u8], name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.ends_with(".svg") || n.ends_with(".svgz") {
        return true;
    }
    let head = String::from_utf8_lossy(&data[..data.len().min(2048)]).to_lowercase();
    let t = head.trim_start();
    t.starts_with("<svg") || (t.starts_with("<?xml") && head.contains("<svg"))
}

/// Decode any supported image to premultiplied RGBA, at most 1600 wide.
pub fn decode(data: &[u8], name: &str) -> Option<Image> {
    let img = if data.starts_with(b"\x89PNG") {
        decode_png(data)
    } else if data.starts_with(&[0xff, 0xd8]) {
        decode_jpeg(data)
    } else if data.starts_with(b"GIF8") {
        decode_gif(data)
    } else if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        decode_webp(data)
    } else if looks_like_svg(data, name) {
        decode_svg(data)
    } else {
        None
    }?;
    if img.w as u32 > KEEP_WIDTH {
        let h = ((img.h as i64 * KEEP_WIDTH as i64) / img.w as i64).max(1) as i32;
        return Some(resize(&img, KEEP_WIDTH as i32, h));
    }
    Some(img)
}

fn premultiply(w: u32, h: u32, mut rgba: Vec<u8>) -> Image {
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a < 255 {
            px[0] = ((px[0] as u32 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u32 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u32 * a + 127) / 255) as u8;
        }
    }
    Image {
        w: w as i32,
        h: h as i32,
        data: rgba,
    }
}

fn decode_png(data: &[u8]) -> Option<Image> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let px = (w * h) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..px * 4].to_vec(),
        png::ColorType::Rgb => buf[..px * 3]
            .chunks_exact(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf[..px].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..px * 2]
            .chunks_exact(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        _ => return None,
    };
    Some(premultiply(w, h, rgba))
}

fn decode_jpeg(data: &[u8]) -> Option<Image> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(data, opts);
    let px = d.decode().ok()?;
    let (w, h) = d.dimensions()?;
    let img = Image {
        w: w as i32,
        h: h as i32,
        data: px,
    };
    Some(orient(img, exif_orientation(data)))
}

fn decode_gif(data: &[u8]) -> Option<Image> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut d = opts.read_info(std::io::Cursor::new(data)).ok()?;
    let (w, h) = (d.width() as u32, d.height() as u32);
    let mut canvas = vec![0u8; (w * h * 4) as usize];
    let f = d.read_next_frame().ok()??;
    for y in 0..f.height as u32 {
        for x in 0..f.width as u32 {
            let (cx, cy) = (x + f.left as u32, y + f.top as u32);
            if cx >= w || cy >= h {
                continue;
            }
            let s = ((y * f.width as u32 + x) * 4) as usize;
            let t = ((cy * w + cx) * 4) as usize;
            if f.buffer[s + 3] > 0 {
                canvas[t..t + 4].copy_from_slice(&f.buffer[s..s + 4]);
            }
        }
    }
    Some(premultiply(w, h, canvas))
}

fn decode_webp(data: &[u8]) -> Option<Image> {
    let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(data)).ok()?;
    let (w, h) = d.dimensions();
    let alpha = d.has_alpha();
    let mut buf = vec![0u8; d.output_buffer_size()?];
    d.read_image(&mut buf).ok()?;
    let rgba = if alpha {
        buf
    } else {
        buf.chunks_exact(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect()
    };
    Some(premultiply(w, h, rgba))
}

fn decode_svg(data: &[u8]) -> Option<Image> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(data, &opt).ok()?;
    let size = tree.size();
    let (mut w, mut h) = (size.width(), size.height());
    if w > KEEP_WIDTH as f32 {
        h = h * KEEP_WIDTH as f32 / w;
        w = KEEP_WIDTH as f32;
    }
    let (wi, hi) = (w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32);
    let mut pm = resvg::tiny_skia::Pixmap::new(wi, hi)?;
    let t = resvg::tiny_skia::Transform::from_scale(w / size.width(), h / size.height());
    resvg::render(&tree, t, &mut pm.as_mut());
    Some(Image {
        w: wi as i32,
        h: hi as i32,
        data: pm.take(),
    })
}

/// The EXIF orientation (1-8) of a JPEG, 1 when there is none.
fn exif_orientation(d: &[u8]) -> u16 {
    let mut i = 2;
    while i + 4 < d.len() {
        if d[i] != 0xff {
            return 1;
        }
        let marker = d[i + 1];
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        if marker == 0xe1 && i + 10 < d.len() && &d[i + 4..i + 10] == b"Exif\0\0" {
            let t = &d[i + 10..(i + 2 + len).min(d.len())];
            if t.len() < 8 {
                return 1;
            }
            let le = &t[0..2] == b"II";
            let u16_at = |o: usize| -> u16 {
                if o + 2 > t.len() {
                    return 0;
                }
                if le {
                    u16::from_le_bytes([t[o], t[o + 1]])
                } else {
                    u16::from_be_bytes([t[o], t[o + 1]])
                }
            };
            let u32_at = |o: usize| -> u32 {
                if o + 4 > t.len() {
                    return 0;
                }
                let b = [t[o], t[o + 1], t[o + 2], t[o + 3]];
                if le {
                    u32::from_le_bytes(b)
                } else {
                    u32::from_be_bytes(b)
                }
            };
            let ifd = u32_at(4) as usize;
            let n = u16_at(ifd) as usize;
            for k in 0..n {
                let e = ifd + 2 + k * 12;
                if u16_at(e) == 0x0112 {
                    let v = u16_at(e + 8);
                    return if (1..=8).contains(&v) { v } else { 1 };
                }
            }
            return 1;
        }
        if marker == 0xda {
            return 1;
        }
        i += 2 + len;
    }
    1
}

fn orient(img: Image, o: u16) -> Image {
    if o <= 1 {
        return img;
    }
    let (w, h) = (img.w as usize, img.h as usize);
    let swap = o >= 5;
    let (nw, nh) = if swap { (h, w) } else { (w, h) };
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match o {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (h - 1 - y, x),
                7 => (h - 1 - y, w - 1 - x),
                8 => (y, w - 1 - x),
                _ => (x, y),
            };
            let s = (y * w + x) * 4;
            let t = (ny * nw + nx) * 4;
            out[t..t + 4].copy_from_slice(&img.data[s..s + 4]);
        }
    }
    Image {
        w: nw as i32,
        h: nh as i32,
        data: out,
    }
}

/// Area-averaging resize (good for the usual downscale), bilinear up.
pub fn resize(img: &Image, w: i32, h: i32) -> Image {
    let (sw, sh) = (img.w.max(1) as usize, img.h.max(1) as usize);
    let (dw, dh) = (w.max(1) as usize, h.max(1) as usize);
    let mut out = vec![0u8; dw * dh * 4];
    let fx = sw as f32 / dw as f32;
    let fy = sh as f32 / dh as f32;
    for y in 0..dh {
        let y0 = (y as f32 * fy) as usize;
        let y1 = (((y + 1) as f32 * fy).ceil() as usize).clamp(y0 + 1, sh);
        for x in 0..dw {
            let x0 = (x as f32 * fx) as usize;
            let x1 = (((x + 1) as f32 * fx).ceil() as usize).clamp(x0 + 1, sw);
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for yy in y0.min(sh - 1)..y1 {
                for xx in x0.min(sw - 1)..x1 {
                    let s = (yy * sw + xx) * 4;
                    for (c, v) in acc.iter_mut().enumerate() {
                        *v += img.data[s + c] as u32;
                    }
                    n += 1;
                }
            }
            let t = (y * dw + x) * 4;
            for c in 0..4 {
                out[t + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    Image {
        w: dw as i32,
        h: dh as i32,
        data: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_and_unknown_data() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#f00"/></svg>"##;
        let i = decode(svg, "x.svg").unwrap();
        assert_eq!((i.w, i.h), (40, 20));
        assert_eq!(&i.data[0..4], &[255, 0, 0, 255]);
        assert!(decode(b"not an image", "x.bin").is_none());
        let r = resize(&i, 20, 10);
        assert_eq!((r.w, r.h), (20, 10));
    }
}
