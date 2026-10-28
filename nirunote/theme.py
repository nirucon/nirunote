from __future__ import annotations

import os
import shutil
import tomllib
from pathlib import Path
from typing import Any

DARK = {
    'bg': '#090909', 'fg': '#ededed', 'muted': '#858585', 'panel': '#141414',
    'border': '#292929', 'accent': '#bdbdbd', 'select': '#333333',
    'line': '#0b0b0b', 'md': '#6f6f6f',
}


def config_home() -> Path:
    return Path(os.environ.get('XDG_CONFIG_HOME') or Path.home() / '.config')


def theme_paths() -> dict[str, Path]:
    config = config_home()
    return {
        'appearance': config / 'niruwm' / 'appearance.toml',
        'config': config / 'niruwm' / 'config.toml',
        'kitty': config / 'nirux' / 'theme' / 'kitty.conf',
    }


def nirux_available() -> bool:
    paths = theme_paths()
    return bool(
        shutil.which('niructl')
        or paths['appearance'].exists()
        or paths['config'].exists()
        or paths['kitty'].exists()
        or (config_home() / 'nirux').is_dir()
    )


def _hex(value: Any, fallback: str) -> str:
    if not isinstance(value, str):
        return fallback
    value = value.strip().strip('"\'')
    if not value:
        return fallback
    if not value.startswith('#'):
        value = '#' + value
    if len(value) == 7 and all(c in '0123456789abcdefABCDEF' for c in value[1:]):
        return value.lower()
    if len(value) == 4 and all(c in '0123456789abcdefABCDEF' for c in value[1:]):
        return '#' + ''.join(c * 2 for c in value[1:]).lower()
    return fallback


def _rgb(value: str) -> tuple[int, int, int]:
    value = _hex(value, '#000000')
    return tuple(int(value[i:i + 2], 16) for i in (1, 3, 5))


def _hex_rgb(rgb) -> str:
    return '#' + ''.join(f'{max(0, min(255, round(v))):02x}' for v in rgb)


def mix(first: str, second: str, weight: float) -> str:
    weight = max(0.0, min(1.0, weight))
    a, b = _rgb(first), _rgb(second)
    return _hex_rgb(tuple(a[i] * (1 - weight) + b[i] * weight for i in range(3)))


def luminance(value: str) -> float:
    def channel(v: int) -> float:
        v /= 255
        return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4
    r, g, b = _rgb(value)
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)


def contrast(first: str, second: str) -> float:
    high, low = sorted((luminance(first), luminance(second)), reverse=True)
    return (high + 0.05) / (low + 0.05)


def readable_on(background: str) -> str:
    return max(('#ffffff', '#000000'), key=lambda color: contrast(background, color))


def _appearance(path: Path) -> dict[str, Any]:
    try:
        with path.open('rb') as handle:
            data = tomllib.load(handle)
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


def _kitty(path: Path) -> dict[str, str]:
    colors: dict[str, str] = {}
    try:
        lines = path.read_text(errors='replace').splitlines()
    except Exception:
        return colors
    for line in lines:
        line = line.strip()
        if not line or line.startswith('#') or len(line.split()) < 2:
            continue
        key, value = line.split(None, 1)
        if key in {'background', 'foreground', 'color0', 'color1', 'color4', 'color8', 'selection_background'}:
            colors[key] = value.strip()
    return colors


def load_nirux_palette(paths: dict[str, Path] | None = None) -> dict[str, str]:
    paths = paths or theme_paths()
    appearance = _appearance(paths['appearance'])
    kitty = _kitty(paths['kitty'])
    colors = appearance.get('colors') if isinstance(appearance.get('colors'), dict) else {}
    palette = dict(DARK)
    if kitty:
        bg = _hex(kitty.get('background'), palette['bg'])
        fg = _hex(kitty.get('foreground'), palette['fg'])
        muted = _hex(kitty.get('color8'), palette['muted'])
        accent = _hex(kitty.get('color4'), palette['accent'])
        palette.update({
            'bg': bg,
            'fg': fg,
            'muted': muted,
            'border': mix(_hex(kitty.get('color0'), bg), muted, 0.55),
            'accent': accent,
            'select': _hex(kitty.get('selection_background'), mix(bg, accent, 0.25)),
            'line': mix(bg, fg, 0.06),
            'md': mix(muted, accent, 0.20),
            'panel': mix(bg, fg, 0.035),
        })
    if colors:
        palette.update({
            'bg': _hex(colors.get('background'), palette['bg']),
            'fg': _hex(colors.get('foreground'), palette['fg']),
            'muted': _hex(colors.get('muted'), palette['muted']),
            'border': _hex(colors.get('border'), palette['border']),
            'accent': _hex(colors.get('focus') or colors.get('accent'), palette['accent']),
            'select': _hex(colors.get('selected'), palette['select']),
            'panel': _hex(colors.get('surface'), palette['panel']),
        })
    if contrast(palette['bg'], palette['fg']) < 4.5:
        palette['fg'] = readable_on(palette['bg'])
    if contrast(palette['bg'], palette['muted']) < 3.0:
        palette['muted'] = mix(palette['fg'], palette['bg'], 0.45)
    if contrast(palette['panel'], palette['fg']) < 3.0:
        palette['panel'] = mix(palette['bg'], palette['fg'], 0.08)
    if contrast(palette['select'], palette['fg']) < 3.0:
        palette['select'] = mix(palette['bg'], palette['accent'], 0.30)
    palette['md'] = mix(palette['muted'], palette['accent'], 0.20)
    palette['line'] = mix(palette['bg'], palette['fg'], 0.06)
    return palette
