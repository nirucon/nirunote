//! Colours: the built-in Dark, Light and Satie palettes and "Follow NIRUX",
//! which reads the NIRUX terminal theme (`~/.config/nirux/theme/kitty.conf`)
//! and the optional `[colors]` of `~/.config/niruwm/appearance.toml`, then
//! repairs contrast, exactly as NIRUNOTE 0.7 did.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: u32,
    pub fg: u32,
    pub muted: u32,
    pub panel: u32,
    pub border: u32,
    pub accent: u32,
    pub select: u32,
    pub line: u32,
    pub md: u32,
    pub red: u32,
    pub green: u32,
    pub yellow: u32,
    pub blue: u32,
    pub magenta: u32,
    pub cyan: u32,
}

pub const DARK: Palette = Palette {
    bg: 0x090909,
    fg: 0xededed,
    muted: 0x858585,
    panel: 0x141414,
    border: 0x292929,
    accent: 0xbdbdbd,
    select: 0x333333,
    line: 0x0b0b0b,
    md: 0x6f6f6f,
    red: 0xc97a7a,
    green: 0x8fb08f,
    yellow: 0xc6b37f,
    blue: 0x84a2c0,
    magenta: 0xad94b8,
    cyan: 0x84afad,
};

pub const LIGHT: Palette = Palette {
    bg: 0xf7f7f4,
    fg: 0x181818,
    muted: 0x777777,
    panel: 0xecece8,
    border: 0xd2d2cc,
    accent: 0x444444,
    select: 0xdcdcd6,
    line: 0xf6f6f2,
    md: 0x9a9a92,
    red: 0xa33b3b,
    green: 0x3d7a46,
    yellow: 0x86650c,
    blue: 0x2f5f8f,
    magenta: 0x7a4a8f,
    cyan: 0x2f7470,
};

pub const SATIE: Palette = Palette {
    bg: 0xddd5c4,
    fg: 0x292b29,
    muted: 0x737168,
    panel: 0xcbc2af,
    border: 0xaaa18f,
    accent: 0x4f4c45,
    select: 0xbbb6a9,
    line: 0xddd6c8,
    md: 0x8c887d,
    red: 0x8f3f35,
    green: 0x4d6b3f,
    yellow: 0x755a18,
    blue: 0x3f5a75,
    magenta: 0x6b4a6b,
    cyan: 0x3d6b66,
};

pub const FOLLOW: &str = "Follow NIRUX";

pub fn palette_for(theme: &str) -> Palette {
    match theme {
        "light" => LIGHT,
        "satie" => SATIE,
        _ => DARK,
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home().join(".config"))
}

pub struct ThemePaths {
    pub appearance: PathBuf,
    pub config: PathBuf,
    pub kitty: PathBuf,
}

pub fn theme_paths() -> ThemePaths {
    let c = config_home();
    ThemePaths {
        appearance: c.join("niruwm/appearance.toml"),
        config: c.join("niruwm/config.toml"),
        kitty: c.join("nirux/theme/kitty.conf"),
    }
}

pub fn in_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| {
        std::env::split_paths(&p).any(|d| {
            let f = d.join(name);
            f.is_file()
        })
    })
}

/// NIRUX is installed: `niructl`, a NIRUWM or NIRUX configuration.
pub fn nirux_available() -> bool {
    let p = theme_paths();
    in_path("niructl")
        || p.appearance.exists()
        || p.config.exists()
        || p.kitty.exists()
        || config_home().join("nirux").is_dir()
}

pub fn parse_hex(value: &str) -> Option<u32> {
    let v = value.trim().trim_matches(|c| c == '"' || c == '\'');
    let v = v.strip_prefix('#').unwrap_or(v);
    if !v.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match v.len() {
        6 => u32::from_str_radix(v, 16).ok(),
        3 => {
            let d: Vec<u32> = v.chars().filter_map(|c| c.to_digit(16)).collect();
            Some(((d[0] * 17) << 16) | ((d[1] * 17) << 8) | (d[2] * 17))
        }
        _ => None,
    }
}

/// `first` moved towards `second` by `weight` (0..1), rounded like Python.
pub fn mix(first: u32, second: u32, weight: f64) -> u32 {
    let w = weight.clamp(0.0, 1.0);
    let ch = |shift: u32| {
        let a = ((first >> shift) & 0xff) as f64;
        let b = ((second >> shift) & 0xff) as f64;
        let v = a * (1.0 - w) + b * w;
        // Python's round() rounds half to even.
        let r = v.round();
        let r = if (v - v.trunc() - 0.5).abs() < 1e-9 && (r as i64) % 2 != 0 {
            r - 1.0
        } else {
            r
        };
        (r.clamp(0.0, 255.0) as u32) << shift
    };
    ch(16) | ch(8) | ch(0)
}

pub fn luminance(c: u32) -> f64 {
    let ch = |v: u32| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch((c >> 16) & 0xff) + 0.7152 * ch((c >> 8) & 0xff) + 0.0722 * ch(c & 0xff)
}

pub fn contrast(a: u32, b: u32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

pub fn readable_on(bg: u32) -> u32 {
    if contrast(bg, 0xffffff) >= contrast(bg, 0x000000) {
        0xffffff
    } else {
        0x000000
    }
}

fn read_kitty(path: &Path) -> std::collections::HashMap<String, String> {
    let mut colors = std::collections::HashMap::new();
    let Ok(text) = std::fs::read(path) else {
        return colors;
    };
    let text = String::from_utf8_lossy(&text);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.splitn(2, char::is_whitespace);
        let (Some(k), Some(v)) = (it.next(), it.next()) else {
            continue;
        };
        if matches!(
            k,
            "background"
                | "foreground"
                | "color0"
                | "color1"
                | "color2"
                | "color3"
                | "color4"
                | "color5"
                | "color6"
                | "color8"
                | "selection_background"
        ) {
            colors.insert(k.to_string(), v.trim().to_string());
        }
    }
    colors
}

/// The "Follow NIRUX" palette.
pub fn load_nirux_palette(paths: &ThemePaths) -> Palette {
    let kitty = read_kitty(&paths.kitty);
    let appearance = std::fs::read_to_string(&paths.appearance)
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok());
    let colors = appearance
        .as_ref()
        .and_then(|a| a.get("colors"))
        .and_then(|c| c.as_table())
        .cloned()
        .unwrap_or_default();
    let mut p = DARK;
    let k = |key: &str| kitty.get(key).and_then(|v| parse_hex(v));
    if !kitty.is_empty() {
        let bg = k("background").unwrap_or(p.bg);
        let fg = k("foreground").unwrap_or(p.fg);
        let muted = k("color8").unwrap_or(p.muted);
        let accent = k("color4").unwrap_or(p.accent);
        p.bg = bg;
        p.fg = fg;
        p.muted = muted;
        p.border = mix(k("color0").unwrap_or(bg), muted, 0.55);
        p.accent = accent;
        p.select = k("selection_background").unwrap_or_else(|| mix(bg, accent, 0.25));
        p.panel = mix(bg, fg, 0.035);
    }
    if let Some(v) = k("color1") {
        p.red = v;
    }
    if let Some(v) = k("color2") {
        p.green = v;
    }
    if let Some(v) = k("color3") {
        p.yellow = v;
    }
    if let Some(v) = k("color4") {
        p.blue = v;
    }
    if let Some(v) = k("color5") {
        p.magenta = v;
    }
    if let Some(v) = k("color6") {
        p.cyan = v;
    }
    if !colors.is_empty() {
        let c = |key: &str| colors.get(key).and_then(|v| v.as_str()).and_then(parse_hex);
        p.bg = c("background").unwrap_or(p.bg);
        p.fg = c("foreground").unwrap_or(p.fg);
        p.muted = c("muted").unwrap_or(p.muted);
        p.border = c("border").unwrap_or(p.border);
        p.accent = c("focus").or_else(|| c("accent")).unwrap_or(p.accent);
        p.select = c("selected").unwrap_or(p.select);
        p.panel = c("surface").unwrap_or(p.panel);
    }
    if contrast(p.bg, p.fg) < 4.5 {
        p.fg = readable_on(p.bg);
    }
    if contrast(p.bg, p.muted) < 3.0 {
        p.muted = mix(p.fg, p.bg, 0.45);
    }
    if contrast(p.panel, p.fg) < 3.0 {
        p.panel = mix(p.bg, p.fg, 0.08);
    }
    if contrast(p.select, p.fg) < 3.0 {
        p.select = mix(p.bg, p.accent, 0.30);
    }
    for c in [
        &mut p.red,
        &mut p.green,
        &mut p.yellow,
        &mut p.blue,
        &mut p.magenta,
        &mut p.cyan,
    ] {
        if contrast(p.bg, *c) < 3.0 {
            *c = mix(*c, p.fg, 0.45);
        }
    }
    p.md = mix(p.muted, p.accent, 0.20);
    p.line = mix(p.bg, p.fg, 0.06);
    p
}

/// The palette of a theme name ("dark", "light", "satie", "Follow NIRUX").
pub fn resolve(theme: &str) -> Palette {
    if theme == FOLLOW {
        load_nirux_palette(&theme_paths())
    } else {
        palette_for(theme)
    }
}

/// Derived colours of the Preview (the old style sheet).
pub struct PreviewColors {
    pub code_bg: u32,
    pub quote_fg: u32,
    pub odd: u32,
}

pub fn preview_colors(p: &Palette) -> PreviewColors {
    PreviewColors {
        code_bg: if p.panel != p.bg {
            p.panel
        } else {
            mix(p.bg, p.fg, 0.05)
        },
        quote_fg: mix(p.fg, p.muted, 0.45),
        odd: mix(p.bg, p.fg, 0.025),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nirux_palette_follows_kitty_and_survives_a_broken_appearance_file() {
        let dir = std::env::temp_dir().join(format!("nirunote-theme-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("nirux/theme")).unwrap();
        std::fs::create_dir_all(dir.join("niruwm")).unwrap();
        std::fs::write(
            dir.join("nirux/theme/kitty.conf"),
            "background #101010\nforeground #eeeeee\ncolor4 #88aacc\ncolor8 #888888\nselection_background #333333\n",
        )
        .unwrap();
        std::fs::write(dir.join("niruwm/appearance.toml"), "[font]\nsize = 14\n").unwrap();
        let paths = ThemePaths {
            appearance: dir.join("niruwm/appearance.toml"),
            config: dir.join("niruwm/config.toml"),
            kitty: dir.join("nirux/theme/kitty.conf"),
        };
        assert_eq!(load_nirux_palette(&paths).bg, 0x101010);
        std::fs::write(dir.join("niruwm/appearance.toml"), "not valid = [").unwrap();
        assert_eq!(load_nirux_palette(&paths).bg, 0x101010);
        std::fs::write(
            dir.join("niruwm/appearance.toml"),
            "[colors]\nbackground = \"203040\"\n",
        )
        .unwrap();
        assert_eq!(load_nirux_palette(&paths).bg, 0x203040);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unknown_themes_fall_back_to_dark_and_mix_rounds_like_python() {
        assert_eq!(palette_for("unknown"), DARK);
        assert_eq!(mix(0x000000, 0xffffff, 0.5), 0x808080);
        assert_eq!(parse_hex("#abc"), Some(0xaabbcc));
        assert_eq!(parse_hex("zz"), None);
    }
}
