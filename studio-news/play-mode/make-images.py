"""Illustrations for the "Play mode" and "News" Studios articles.

Same style and helpers as client-installations/make-images.py (drawn SVG,
no screenshots or real data). Usage:
    python make-images.py <studio-news folder>
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_src = open(os.path.join(HERE, '..', 'client-installations', 'make-images.py'), encoding='utf-8').read()
_helpers = _src[: _src.index('# ---------- 1. Create instance ----------')]
sys.argv = [sys.argv[0], sys.argv[1] if len(sys.argv) > 1 else '.']
exec(_helpers)  # noqa: S102 - our own helper code

ROOT = sys.argv[1]
W, H = 1920, 1280

ICONS.update({
    'star': ['M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z'],
    'chev_down': ['m6 9 6 6 6-6'],
    'gear': ['M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z', 'M12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6'],
    'globe': ['M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20', 'M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20', 'M2 12h20'],
    'server': ['M4 2h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z', 'M4 14h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2z', 'M6 6h.01', 'M6 18h.01'],
    'ext': ['M15 3h6v6', 'M10 14 21 3', 'M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h3'],
    'github': ['M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4', 'M9 18c-4.51 2-5-2-7-2'],
})


def window(x, y, w, h, title='Modrinth Studios'):
    b = f'<g filter="url(#shadow)">{rect(x, y, w, h, 26, C["win"], C["border"], 2)}</g>'
    for i, c in enumerate(['#ff5f57', '#febc2e', '#28c840']):
        b += f'<circle cx="{x + 34 + i * 26}" cy="{y + 32}" r="9" fill="{c}"/>'
    b += t(x + 130, y + 41, title, 24, 600, C['sec'])
    return b


def terrain(x, y, w, h, gid, sky=('#7fd3ff', '#3c7bd6'), cell=40, seed=3):
    """A blocky, Minecraft-like landscape: sky, hills of grass and dirt, water."""
    d = (f'<linearGradient id="{gid}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{sky[0]}"/>'
         f'<stop offset="1" stop-color="{sky[1]}"/></linearGradient>'
         f'<clipPath id="{gid}c"><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="0"/></clipPath>')
    b = f'<g clip-path="url(#{gid}c)"><rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{gid})"/>'
    # sun + clouds
    b += f'<rect x="{x + w * 0.78}" y="{y + h * 0.12}" width="{cell * 2}" height="{cell * 2}" fill="#fff6c8"/>'
    for cx, cy, n in [(0.12, 0.16, 5), (0.45, 0.1, 4), (0.62, 0.24, 3)]:
        b += f'<rect x="{x + w * cx}" y="{y + h * cy}" width="{cell * n}" height="{cell * 0.8}" fill="#ffffff" opacity="0.85"/>'
    cols = int(w / cell) + 2
    import math as _m
    for i in range(cols):
        hx = x + i * cell
        top = h * (0.55 + 0.12 * _m.sin(i * 0.33 + seed) + 0.06 * _m.sin(i * 0.9 + seed * 2))
        top = round(top / cell) * cell
        far = h * (0.42 + 0.1 * _m.sin(i * 0.21 + seed + 1))
        far = round(far / cell) * cell
        b += f'<rect x="{hx}" y="{y + far}" width="{cell}" height="{h}" fill="#3f7f6a" opacity="0.55"/>'
        b += f'<rect x="{hx}" y="{y + top}" width="{cell}" height="{cell * 0.5}" fill="#5fbf3f"/>'
        b += f'<rect x="{hx}" y="{y + top + cell * 0.5}" width="{cell}" height="{h}" fill="#7a5233"/>'
        b += f'<rect x="{hx}" y="{y + top + cell * 0.5}" width="{cell}" height="{cell * 0.5}" fill="#4f9a34"/>'
        if (i * 7 + seed) % 9 == 0:  # a tree
            b += f'<rect x="{hx + cell * 0.35}" y="{y + top - cell * 2}" width="{cell * 0.3}" height="{cell * 2}" fill="#5b3a20"/>'
            b += f'<rect x="{hx - cell * 0.5}" y="{y + top - cell * 3.2}" width="{cell * 2}" height="{cell * 1.4}" fill="#2f7d32"/>'
    b += f'<rect x="{x}" y="{y + h * 0.86}" width="{w}" height="{h}" fill="#2f6fd0" opacity="0.85"/>'
    b += '</g>'
    return d, b


def avatar(x, y, size, c1, c2, gid, letter=None):
    d = (f'<linearGradient id="{gid}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{c1}"/>'
         f'<stop offset="1" stop-color="{c2}"/></linearGradient>')
    b = rect(x, y, size, size, size * 0.24, f'url(#{gid})')
    # a little blocky landscape glyph instead of a real instance icon
    s = size / 8
    b += f'<rect x="{x + s}" y="{y + 5 * s}" width="{6 * s}" height="{2 * s}" fill="#ffffff" opacity="0.35"/>'
    b += f'<rect x="{x + 2 * s}" y="{y + 3 * s}" width="{2 * s}" height="{2 * s}" fill="#ffffff" opacity="0.5"/>'
    b += f'<rect x="{x + 5 * s}" y="{y + 2 * s}" width="{s}" height="{s}" fill="#ffffff" opacity="0.6"/>'
    if letter:
        b += t(x + size / 2, y + size * 0.62, letter, size * 0.42, 800, '#fff', 'middle')
    return d, b


INSTANCES = [
    ('Survival', 'Fabric 1.21.11', '#43d17a', '#126b3c'),
    ('Create: Above & Beyond', 'Forge 1.20.1', '#ffb04d', '#8a4a12'),
    ('Skyblock', 'Fabric 1.21.10', '#5aa9ff', '#1a4a8a'),
    ('Creative builds', 'Vanilla 1.21.11', '#b48bff', '#4a2a8a'),
    ('Hardcore', 'Fabric 1.21.8', '#ff6b6b', '#8a1f1f'),
    ('Lunar Client 1.8.9', 'Lunar 1.8.9', '#6b5cff', '#2a2268'),
]


PALETTES = {
    # sky top, sky mid, horizon, mountain layers far -> near, water
    'dusk': ('#1b2a5e', '#5b6fb5', '#f6b878', ['#9a93c9', '#6f72a8', '#3f5a74', '#24473a', '#14291f'], '#3d5d9c', '#ffd27a'),
    'night': ('#070b22', '#1a2156', '#3b3f86', ['#2c3070', '#222660', '#191c48', '#111432', '#0a0c20'], '#1d2358', '#eef2ff'),
    'snow': ('#6fa3e0', '#a9cdf3', '#e9f3fd', ['#cfdcec', '#b4c6dc', '#90a8c2', '#e8eef6', '#c3d1e0'], '#7fa6d6', '#fffbe8'),
    'desert': ('#e9895a', '#f6b77c', '#fde3b4', ['#e7b27a', '#d79a62', '#c07d48', '#a8653a', '#8a4f2c'], '#d98f5a', '#fff2c4'),
    'nether': ('#1f0606', '#4a0f0c', '#8c2a1a', ['#6e1c14', '#55140f', '#3f0e0b', '#2b0907', '#1a0504'], '#c2410c', '#ffb347'),
    'noon': ('#2f7fd6', '#6fb3f2', '#cfe9ff', ['#9cc1de', '#6f9fc0', '#4f8a5a', '#3a7a3c', '#2a5e2c'], '#2f6fd0', '#fff8d6'),
}


def scene(x, y, w, h, gid, pal='dusk', cell=None, seed=1, sun=(0.72, 0.62), blur=True):
    """A blocky landscape drawn like a screenshot: layered ridges with depth."""
    import math as _m
    top, mid, hor, layers, water, glow = PALETTES[pal]
    cell = cell or max(6, w / 64)
    d = (f'<linearGradient id="{gid}k" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{top}"/>'
         f'<stop offset="0.55" stop-color="{mid}"/><stop offset="1" stop-color="{hor}"/></linearGradient>'
         f'<radialGradient id="{gid}g" cx="{sun[0]}" cy="{sun[1]}" r="0.45"><stop offset="0" stop-color="{glow}" stop-opacity="0.85"/>'
         f'<stop offset="1" stop-color="{glow}" stop-opacity="0"/></radialGradient>'
         f'<linearGradient id="{gid}w" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{hor}" stop-opacity="0.9"/>'
         f'<stop offset="1" stop-color="{water}"/></linearGradient>'
         f'<clipPath id="{gid}c"><rect x="{x}" y="{y}" width="{w}" height="{h}"/></clipPath>'
         f'<filter id="{gid}f" x="-5%" y="-5%" width="110%" height="110%"><feGaussianBlur stdDeviation="{cell * 0.25}"/></filter>')
    b = f'<g clip-path="url(#{gid}c)"><rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{gid}k)"/>'
    b += f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{gid}g)"/>'
    s = cell * 3
    b += f'<rect x="{x + w * sun[0] - s / 2}" y="{y + h * sun[1] - s / 2}" width="{s}" height="{s}" fill="{glow}"/>'
    # clouds
    for i, (cx, cy, n) in enumerate([(0.08, 0.14, 7), (0.38, 0.08, 5), (0.58, 0.22, 4), (0.86, 0.12, 6)]):
        cx = (cx + seed * 0.13) % 1
        b += f'<rect x="{x + w * cx}" y="{y + h * cy}" width="{cell * n}" height="{cell}" fill="#ffffff" opacity="0.18"/>'
        b += f'<rect x="{x + w * cx + cell}" y="{y + h * cy - cell * 0.6}" width="{cell * (n - 3)}" height="{cell * 0.6}" fill="#ffffff" opacity="0.14"/>'
    water_y = y + h * 0.82
    n = len(layers)
    for li, col in enumerate(layers):
        base = 0.42 + li * 0.085
        amp = 0.16 - li * 0.02
        step = cell * (1 + li * 0.5)
        pts = [f'M{x} {water_y}']
        xi = x
        k = 0
        while xi < x + w + step:
            v = (_m.sin(k * 0.21 + seed * 1.7 + li * 2.1) * 0.6
                 + _m.sin(k * 0.53 + seed + li) * 0.3 + _m.sin(k * 1.31 + li * 3) * 0.1)
            hy = y + h * (base - amp * v)
            hy = round((hy - y) / step) * step + y
            pts.append(f'L{xi} {hy} L{xi + step} {hy}')
            xi += step
            k += 1
        pts.append(f'L{x + w + step} {water_y} Z')
        far = li < n - 2
        filt = f' filter="url(#{gid}f)"' if blur and far else ''
        b += f'<path d="{" ".join(pts)}" fill="{col}"{filt}/>'
        if li == n - 1 or li == n - 2:  # grass caps + trees on the near ridges
            pass
    b += f'<rect x="{x}" y="{water_y}" width="{w}" height="{h}" fill="url(#{gid}w)"/>'
    for i in range(int(w / (cell * 4))):  # light streaks on the water
        if (i * 7 + seed) % 5 == 0:
            b += f'<rect x="{x + i * cell * 4}" y="{water_y + cell * (1 + i % 3)}" width="{cell * 2}" height="{cell * 0.35}" fill="#ffffff" opacity="0.18"/>'
    b += '</g>'
    return d, b


SB = 92  # width of the app's left sidebar

ICONS.update({
    'library': ['M3 3h7v7H3z', 'M14 3h7v7h-7z', 'M14 14h7v7h-7z', 'M3 14h7v7H3z'],
    'video': ['m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5', 'M4 6h10a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z'],
    'tag': ['M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z', 'M7.5 7.5h.01'],
    'timer': ['M10 2h4', 'M12 14l3-3', 'M12 6a8 8 0 1 0 0 16 8 8 0 1 0 0-16'],
    'clock': ['M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20', 'M12 6v6l4 2'],
    'more': ['M12 11a1 1 0 1 0 0 2 1 1 0 1 0 0-2', 'M19 11a1 1 0 1 0 0 2 1 1 0 1 0 0-2', 'M5 11a1 1 0 1 0 0 2 1 1 0 1 0 0-2'],
})


def play_mode_frame(defs, body, x0, y0, w0, h0, gid):
    """The app with Play mode open: sidebar, artwork, centred tab bar."""
    # app sidebar
    body += rect(x0 + 2, y0 + 64, SB - 2, h0 - 66, 0, C['win'])
    body += f'<line x1="{x0 + SB}" y1="{y0 + 64}" x2="{x0 + SB}" y2="{y0 + h0 - 2}" stroke="{C["border"]}" stroke-width="2"/>'
    nav = ['play', 'library', 'compass', 'server', 'video']
    for i, ic in enumerate(nav):
        cy = y0 + 92 + i * 72
        if i == 0:
            body += rect(x0 + 18, cy - 4, 56, 56, 16, C['brandhl'])
        body += icon(ic, x0 + 32, cy + 10, 28, C['brand'] if i == 0 else C['sec'], 2.2)
    sep = y0 + 92 + 5 * 72
    body += f'<line x1="{x0 + 26}" y1="{sep}" x2="{x0 + SB - 26}" y2="{sep}" stroke="{C["border"]}" stroke-width="2"/>'
    body += rect(x0 + 18, sep + 12, 56, 56, 16, C['surf3'])
    body += icon('plus', x0 + 32, sep + 26, 28, C['prim'], 2.2)
    body += icon('gear', x0 + 32, y0 + h0 - 76, 28, C['sec'], 2)
    # account in the title bar (generic placeholder, not anyone's skin)
    ax = x0 + w0 - 210
    body += rect(ax, y0 + 14, 180, 38, 19, C['surf3'])
    body += rect(ax + 6, y0 + 20, 26, 26, 6, '#6b5a43')
    body += rect(ax + 6, y0 + 20, 26, 9, 3, '#3b2f24')
    body += t(ax + 44, y0 + 41, 'Player', 19, 600, C['prim'])
    body += icon('chev_down', ax + 146, y0 + 22, 22, C['sec'], 2.2)

    # artwork + the same shades the page uses
    bx, by, bw, bh = x0 + SB + 2, y0 + 64, w0 - SB - 4, h0 - 66
    d, b = scene(bx, by, bw, bh, gid, 'dusk', cell=24, seed=2, sun=(0.66, 0.52))
    defs += d
    defs += (f'<linearGradient id="{gid}s" x1="0" y1="0" x2="0" y2="1">'
             f'<stop offset="0" stop-color="#16181c" stop-opacity="0.55"/><stop offset="0.14" stop-color="#16181c" stop-opacity="0"/>'
             f'<stop offset="0.40" stop-color="#16181c" stop-opacity="0"/><stop offset="0.62" stop-color="#16181c" stop-opacity="0.55"/>'
             f'<stop offset="0.80" stop-color="#16181c" stop-opacity="0.92"/><stop offset="0.95" stop-color="#16181c" stop-opacity="1"/></linearGradient>'
             f'<linearGradient id="{gid}l" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#16181c" stop-opacity="0.7"/>'
             f'<stop offset="0.4" stop-color="#16181c" stop-opacity="0.15"/><stop offset="0.6" stop-color="#16181c" stop-opacity="0"/>'
             f'<stop offset="0.75" stop-color="#16181c" stop-opacity="0.2"/><stop offset="1" stop-color="#16181c" stop-opacity="0.75"/></linearGradient>')
    body += b
    body += f'<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" fill="url(#{gid}s)"/>'
    body += f'<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" fill="url(#{gid}l)"/>'

    # top row: instance picker | tabs
    px = bx + 34
    d, b = avatar(px, y0 + 96, 44, '#43d17a', '#126b3c', gid + 'a')
    defs += d
    body += b
    body += t(px + 58, y0 + 127, 'Survival', 26, 700)
    body += icon('chev_down', px + 58 + text_w('Survival', 26, 700) + 8, y0 + 106, 24, C['prim'], 2.6)
    tabs = ['Play', 'Content', 'Files', 'Screenshots', 'Worlds', 'Logs']
    tw = sum(text_w(tb, 21) + 40 for tb in tabs) + 12
    tx = bx + bw / 2 - tw / 2
    body += rect(tx, y0 + 92, tw, 52, 26, 'rgba(22,24,28,0.72)', 'rgba(255,255,255,0.10)', 1.5)
    cx = tx + 6
    for i, tb in enumerate(tabs):
        ww = text_w(tb, 21) + 40
        if i == 0:
            body += rect(cx, y0 + 98, ww, 40, 20, C['brandhl'])
        body += t(cx + 20, y0 + 125, tb, 21, 600, C['brand'] if i == 0 else C['prim'])
        cx += ww
    return defs, body


def play_home():
    defs, bg = gradient_bg(W, H, [(0, '#16337a'), (0.6, '#0e6f73'), (1, '#29c7a0')])
    defs += SHADOW
    x0, y0, w0, h0 = 110, 90, W - 220, H - 180
    body = bg + window(x0, y0, w0, h0)
    defs, body = play_mode_frame(defs, body, x0, y0, w0, h0, 'pm')
    left = x0 + SB + 36
    right = x0 + w0 - 36

    # bottom: Jump back in (frosted panel with world rows)
    jh = 236
    jy = y0 + h0 - 36 - jh
    body += rect(left, jy, right - left, jh, 30, 'rgba(22,24,28,0.72)', C['border'], 2)
    body += t(left + 28, jy + 44, 'JUMP BACK IN', 17, 700, C['sec'])
    rows = [('Main world', 'Singleplayer', 'Survival mode · Played 2 hours ago', 'noon', 6, False),
            ('Friends SMP', 'Server', '12 / 40 online · Played yesterday', 'dusk', 8, True)]
    rw = (right - left - 56 - 16) / 2
    for i, (name, kind, sub, pal, sd, server) in enumerate(rows):
        rx, ry = left + 28 + i * (rw + 16), jy + 66
        body += rect(rx, ry, rw, 142, 20, C['surf3'], C['surf4'], 1.5)
        d, b = scene(rx + 16, ry + 16, 110, 110, f'jb{i}', pal, cell=6, seed=sd, blur=False)
        defs += d + f'<clipPath id="jc{i}"><rect x="{rx + 16}" y="{ry + 16}" width="110" height="110" rx="14"/></clipPath>'
        body += f'<g clip-path="url(#jc{i})">{b}</g>'
        body += t(rx + 148, ry + 52, name, 26, 700)
        kw = text_w(kind, 16, 600) + 44
        body += rect(rx + 148, ry + 66, kw, 28, 14, C['surf4'])
        body += icon('server' if server else 'globe', rx + 156, ry + 71, 18, C['prim'], 2)
        body += t(rx + 180, ry + 86, kind, 16, 600, C['prim'])
        body += t(rx + 148, ry + 122, sub, 19, 400, C['sec'])
        bw_ = 120
        body += rect(rx + rw - bw_ - 22, ry + 47, bw_, 48, 14, C['brand'])
        body += icon('play', rx + rw - bw_ - 2, ry + 59, 24, '#0b1a10', 2.6)
        body += t(rx + rw - bw_ + 30, ry + 79, 'Play', 22, 700, '#0b1a10')

    # hero (bottom left, above the panel)
    base = jy - 32
    ay = base - 300
    d, b = avatar(left, ay, 128, '#43d17a', '#126b3c', 'hero')
    defs += d
    body += b
    body += t(left + 156, ay + 100, 'Survival', 112, 800)
    mx, my = left, ay + 176
    for ic, label in [('tag', 'Fabric 1.21.11'), ('timer', '126 hours'), ('clock', 'Played yesterday')]:
        body += icon(ic, mx, my - 20, 24, C['sec'], 2)
        body += t(mx + 34, my, label, 22, 600, C['prim'])
        mx += 34 + text_w(label, 22, 600) + 30
    by_ = base - 76
    body += rect(left, by_, 230, 76, 22, C['brand'])
    body += icon('play', left + 52, by_ + 22, 32, '#0b1a10', 2.8)
    body += t(left + 100, by_ + 50, 'Play', 30, 700, '#0b1a10')
    for j, ic in enumerate(['gear', 'more']):
        gx = left + 246 + j * 92
        body += rect(gx, by_, 76, 76, 22, 'rgba(46,48,54,0.85)', C['surf5'], 1.5)
        body += icon(ic, gx + 22, by_ + 22, 32, C['text'], 2.2)

    # right of the hero: highlights above the screenshots, right aligned
    gw = 580
    gx0 = right - gw
    gap = 12
    tw_ = (gw - 2 * gap) / 3
    th_ = tw_ * 9 / 16
    gy0 = base - (2 * th_ + gap)
    body += t(gx0, gy0 - 20, 'SCREENSHOTS', 17, 700, C['prim'])
    body += t(right, gy0 - 20, 'View all 24', 18, 600, C['prim'], 'end')
    shots = [('night', 3), ('snow', 5), ('desert', 7), ('noon', 2), ('nether', 4), ('dusk', 9)]
    for i, (pal, sd) in enumerate(shots):
        sx = gx0 + (i % 3) * (tw_ + gap)
        sy = gy0 + (i // 3) * (th_ + gap)
        d, b = scene(sx, sy, tw_, th_, f'shot{i}', pal, cell=6, seed=sd, sun=(0.3 + 0.1 * i, 0.5), blur=False)
        defs += d + f'<clipPath id="sc{i}"><rect x="{sx}" y="{sy}" width="{tw_}" height="{th_}" rx="14"/></clipPath>'
        body += f'<g clip-path="url(#sc{i})">{b}</g>'
        body += rect(sx, sy, tw_, th_, 14, 'none', 'rgba(255,255,255,0.12)', 1.5)
    hy = gy0 - 70
    hx = right
    for num, lab in [('126', 'HOURS PLAYED'), ('7', 'WORLDS'), ('48', 'MODS')]:
        body += t(hx, hy - 34, num, 76, 800, '#fff', 'end')
        body += t(hx, hy, lab, 17, 700, C['prim'], 'end')
        hx -= max(text_w(lab, 17, 700), text_w(num, 76, 800)) + 44
    save(svg(W, H, body, defs), os.path.join('play-mode', 'play-home.webp'), W)


def instance_picker():
    defs, bg = gradient_bg(W, H, [(0, '#16337a'), (0.6, '#0e6f73'), (1, '#29c7a0')])
    defs += SHADOW
    x0, y0, w0, h0 = 110, 90, W - 220, H - 180
    body = bg + window(x0, y0, w0, h0)
    defs, body = play_mode_frame(defs, body, x0, y0, w0, h0, 'pp')
    body += rect(x0 + SB + 2, y0 + 160, w0 - SB - 4, h0 - 162, 0, '#000', extra='opacity="0.35"')
    # popover
    px, py, pw = x0 + SB + 36, y0 + 160, 640
    ph = h0 - 200
    body += f'<g filter="url(#shadow)">{rect(px, py, pw, ph, 24, C["win2"], C["border"], 2)}</g>'
    body += rect(px + 24, py + 24, pw - 48, 62, 16, C['surf3'])
    body += icon('search', px + 44, py + 41, 28, C['sec'], 2.4)
    body += t(px + 88, py + 64, 'Search instances', 24, 400, C['sec'])
    # favourites
    fy = py + 124
    body += icon('star', px + 28, fy - 18, 20, C['sec'], 2.4)
    body += t(px + 56, fy, 'FAVORITES', 18, 700, C['sec'])
    tw = (pw - 48 - 24) / 3
    for i, (name, sub, c1, c2) in enumerate(INSTANCES[:3]):
        tx = px + 24 + i * (tw + 12)
        sel = i == 0
        body += rect(tx, fy + 20, tw, 200, 22, C['brandhl'] if sel else C['surf4'], C['brand'] if sel else C['surf4'], 3 if sel else 2)
        d, b = avatar(tx + tw / 2 - 40, fy + 44, 80, c1, c2, f'fav{i}')
        defs += d
        body += b
        label = name if text_w(name, 22, 700) < tw - 24 else name.split(':')[0]
        body += t(tx + tw / 2, fy + 160, label, 22, 700, C['text'], 'middle')
        body += t(tx + tw / 2, fy + 190, sub.split(' ')[-1], 18, 400, C['sec'], 'middle')
        if i == 1:  # hovered tile shows its star
            body += icon('star', tx + tw - 40, fy + 32, 24, '#ffa347', 2)
            body += f'<g transform="translate({tx + tw - 40},{fy + 32}) scale(1)"></g>'
    # recent
    ry = fy + 270
    body += t(px + 28, ry, 'RECENT', 18, 700, C['sec'])
    for i, (name, sub, c1, c2) in enumerate(INSTANCES[3:]):
        rowy = ry + 18 + i * 92
        hover = i == 1
        if hover:
            body += rect(px + 16, rowy, pw - 32, 80, 16, C['surf4'])
        d, b = avatar(px + 32, rowy + 14, 52, c1, c2, f'rec{i}')
        defs += d
        body += b
        body += t(px + 102, rowy + 38, name, 24, 700)
        body += t(px + 102, rowy + 66, f'{sub} · {["3 days ago", "last week", "2 weeks ago"][i]}', 19, 400, C['sec'])
        if hover:
            body += f'<circle cx="{px + pw - 56}" cy="{rowy + 40}" r="22" fill="{C["surf5"]}"/>'
            body += icon('star', px + pw - 70, rowy + 26, 28, '#ffa347', 2.2)
    save(svg(W, H, body, defs), os.path.join('play-mode', 'instance-picker.webp'), W)


# ---------- News ----------
WARM = [(0, '#7a2e1e'), (0.55, '#8a3a5c'), (1, '#4a2a7a')]


def mini_banner(x, y, w, h, gid, c1, c2, ic):
    d = (f'<linearGradient id="{gid}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{c1}"/>'
         f'<stop offset="1" stop-color="{c2}"/></linearGradient><clipPath id="{gid}c"><rect x="{x}" y="{y}" width="{w}" height="{h}" rx="18"/></clipPath>')
    b = f'<g clip-path="url(#{gid}c)"><rect x="{x}" y="{y}" width="{w}" height="{h}" fill="url(#{gid})"/>'
    s = h / 5
    for r in range(-1, 7):
        for c in range(-1, int(w / s) + 2):
            cx = x + c * s + (r % 2) * s / 2
            cy = y + r * s * 0.58
            b += f'<path d="M{cx} {cy} l{s/2} {s*0.29} l{-s/2} {s*0.29} l{-s/2} {-s*0.29}Z" fill="#fff" opacity="{0.03 + ((r + c) % 3) * 0.02}"/>'
    r = h * 0.28
    b += f'<circle cx="{x + w / 2}" cy="{y + h / 2}" r="{r}" fill="rgba(255,255,255,0.16)" stroke="rgba(255,255,255,0.35)" stroke-width="2"/>'
    b += icon(ic, x + w / 2 - r * 0.6, y + h / 2 - r * 0.6, r * 1.2, '#ffffff', 2)
    b += '</g>'
    return d, b


def news_feed():
    defs, bg = gradient_bg(W, H, WARM)
    defs += SHADOW
    x0, y0, w0, h0 = 110, 90, W - 220, H - 180
    body = bg + window(x0, y0, w0, h0)
    # sidebar icons
    for i in range(7):
        body += rect(x0 + 22, y0 + 92 + i * 64, 40, 40, 12, C['surf3'])
    # library, dimmed
    lx = x0 + 100
    body += t(lx, y0 + 130, 'Library', 34, 700, C['prim'], opacity=0.6)
    for i in range(6):
        cx = lx + (i % 3) * 300
        cy = y0 + 170 + (i // 3) * 330
        d, b = avatar(cx, cy, 260, INSTANCES[i][2], INSTANCES[i][3], f'lib{i}')
        defs += d
        body += f'<g opacity="0.45">{b}</g>'
        body += t(cx, cy + 296, INSTANCES[i][0][:18], 24, 700, C['prim'], opacity=0.5)
    # news sidebar
    sx = x0 + w0 - 520
    body += rect(sx, y0 + 64, 518, h0 - 66, 0, '#141619')
    body += f'<line x1="{sx}" y1="{y0 + 64}" x2="{sx}" y2="{y0 + h0}" stroke="{C["border"]}" stroke-width="2"/>'
    body += t(sx + 36, y0 + 122, 'News', 28, 600, C['prim'])
    cards = [
        ('studios', 'Client installations', 'Play Lunar Client, LabyMod, NoRisk Client and Horizon straight from Modrinth Studios.', 'November 23, 2026', '#3b2a8a', '#1a1240', 'rocket'),
        ('modrinth', 'Joining your server just got easier', 'Share your server with friends directly in the app.', 'November 2, 2026', '#2b1d3a', '#5a2a52', 'server'),
    ]
    cy = y0 + 156
    for i, (src, title, summ, date, c1, c2, ic) in enumerate(cards):
        d, b = mini_banner(sx + 36, cy, 446, 250, f'nb{i}', c1, c2, ic)
        defs += d
        body += b
        if src == 'studios':
            body += rect(sx + 52, cy + 16, 110, 38, 19, C['brand'])
            body += t(sx + 107, cy + 42, 'Studios', 20, 800, '#0b1a10', 'middle')
        body += t(sx + 36, cy + 296, title, 26, 700)
        for j, line in enumerate(wrap(summ, 21, 446)):
            body += t(sx + 36, cy + 332 + j * 28, line, 21, 400, C['prim'])
        body += t(sx + 36, cy + 340 + 28 * len(wrap(summ, 21, 446)) + 8, date, 20, 400, C['sec'])
        cy += 470
    # click cursor hint on the studios card
    body += f'<path d="M{sx + 330} {y0 + 330} l0 46 l12 -12 l9 20 l9 -4 l-9 -20 l17 0 z" fill="#fff" stroke="#000" stroke-width="2"/>'
    save(svg(W, H, body, defs), os.path.join('news', 'news-feed.webp'), W)


def article_reader():
    defs, bg = gradient_bg(W, H, WARM)
    defs += SHADOW
    x0, y0, w0, h0 = 110, 90, W - 220, H - 180
    body = bg + window(x0, y0, w0, h0)
    for i in range(7):
        body += rect(x0 + 22, y0 + 92 + i * 64, 40, 40, 12, C['surf3'])
    body += rect(x0, y0, w0, h0, 26, '#000', extra='opacity="0.5"')
    mx, my, mw, mh = 430, 130, 1060, 1030
    body += modal(mx, my, mw, mh, 'Modrinth Studios news')
    iy = my + 140
    d, b = mini_banner(mx + 40, iy, mw - 80, 330, 'ab', '#3b2a8a', '#1a1240', 'rocket')
    defs += d
    body += b
    body += t(mx + 40, iy + 400, 'Client installations', 48, 800)
    body += t(mx + 40, iy + 448, 'Play Lunar Client, LabyMod, NoRisk Client and Horizon straight from Studios.', 25, 400, C['sec'])
    bx = mx + 40
    body += rect(bx, iy + 474, text_w('Modrinth Studios', 18) + 28, 34, 17, C['brandhl'])
    body += t(bx + 14, iy + 497, 'Modrinth Studios', 18, 700, C['brand'])
    body += t(bx + text_w('Modrinth Studios', 18) + 44, iy + 497, 'November 23, 2026  ·  Version 1.4.0', 20, 400, C['sec'])
    body += f'<line x1="{mx + 40}" y1="{iy + 536}" x2="{mx + mw - 40}" y2="{iy + 536}" stroke="{C["border"]}" stroke-width="2"/>'
    body += t(mx + 40, iy + 584, 'Hey everyone,', 24, 400, C['prim'])
    para = ('This update lets you play other Minecraft clients straight from Modrinth Studios. '
            'They install like any other instance, show up in your library, and each one gets its own pages.')
    for j, line in enumerate(wrap(para, 24, mw - 80)):
        body += t(mx + 40, iy + 630 + j * 36, line, 24, 400, C['prim'])
    # footer
    fy = my + mh - 96
    bw = text_w('View on GitHub', 24) + 90
    body += rect(mx + mw - 40 - bw, fy, bw, 60, 16, 'none', C['surf5'], 2)
    body += icon('ext', mx + mw - 40 - bw + 22, fy + 17, 26, C['text'], 2.2)
    body += t(mx + mw - 40 - bw + 60, fy + 39, 'View on GitHub', 24, 600)
    save(svg(W, H, body, defs), os.path.join('news', 'article-reader.webp'), W)


OUT = ROOT
for f in (play_home, instance_picker, news_feed, article_reader):
    f()
