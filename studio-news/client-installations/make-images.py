"""Illustrations for the "Client installations" Studios article.

Drawn as SVG (no screenshots, no real user data) in the style of Modrinth's
news images: an app window on a coloured gradient, and a thumbnail with a
block pattern and a glossy icon. Rendered with cairosvg + Inter.
"""
import io
import math
import os
import sys

import cairosvg
from PIL import Image, ImageFont

OUT = sys.argv[1]
FONTS = os.path.expanduser('~/.fonts')
_font_cache = {}


def text_w(s, size, weight=600):
    key = (size, weight)
    if key not in _font_cache:
        _font_cache[key] = ImageFont.truetype(f'{FONTS}/Inter-{weight}.ttf', size)
    return _font_cache[key].getlength(s)


FAMILY = {400: 'Inter', 500: 'Inter Medium', 600: 'Inter SemiBold', 700: 'Inter', 800: 'Inter ExtraBold'}

C = {
    'win': '#16181c', 'win2': '#1d1f23', 'surf3': '#25272c', 'surf4': '#2e3036', 'surf5': '#3a3d44',
    'border': '#30333a', 'text': '#ffffff', 'prim': '#c8ccd2', 'sec': '#9196a0',
    'brand': '#1bd96a', 'brandhl': 'rgba(27,217,106,0.15)', 'blue': '#4f9cff', 'purple': '#a678ff',
}


def t(x, y, s, size=28, weight=600, fill=C['text'], anchor='start', opacity=1):
    s = s.replace('&', '&amp;').replace('<', '&lt;')
    fw = 700 if weight == 700 else 400
    return (f'<text x="{x}" y="{y}" font-family="{FAMILY[weight]}" font-weight="{fw}" '
            f'font-size="{size}" fill="{fill}" text-anchor="{anchor}" opacity="{opacity}">{s}</text>')


def rect(x, y, w, h, r=16, fill=C['surf3'], stroke=None, sw=2, extra=''):
    st = f' stroke="{stroke}" stroke-width="{sw}"' if stroke else ''
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{fill}"{st} {extra}/>'


# Lucide icons (24x24 viewBox), drawn as strokes.
ICONS = {
    'rocket': ['M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z',
               'm12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z',
               'M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0', 'M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5'],
    'check': ['M20 6 9 17l-5-5'],
    'x': ['M18 6 6 18', 'm6 6 12 12'],
    'boxes': ['M2.97 12.92A2 2 0 0 0 2 14.63v3.24a2 2 0 0 0 .97 1.71l3 1.8a2 2 0 0 0 2.06 0L12 19v-5.5l-5-3-4.03 2.42Z',
              'm7 16.5-4.74-2.85', 'm7 16.5 5-3', 'M7 16.5v5.17',
              'M12 13.5V19l3.97 2.38a2 2 0 0 0 2.06 0l3-1.8a2 2 0 0 0 .97-1.71v-3.24a2 2 0 0 0-.97-1.71L17 10.5l-5 3Z',
              'm17 16.5-5-3', 'm17 16.5 4.74-2.85', 'M17 16.5v5.17',
              'M7.97 4.42A2 2 0 0 0 7 6.13v4.37l5 3 5-3V6.13a2 2 0 0 0-.97-1.71l-3-1.8a2 2 0 0 0-2.06 0l-3 1.8Z',
              'M12 8 7.26 5.15', 'm12 8 4.74-2.85', 'M12 13.5V8'],
    'compass': ['M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20', 'm16.24 7.76-1.804 5.411a2 2 0 0 1-1.265 1.265L7.76 16.24l1.804-5.411a2 2 0 0 1 1.265-1.265z'],
    'upload': ['M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4', 'm17 8-5-5-5 5', 'M12 3v12'],
    'import': ['M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z', 'm3.3 7 8.7 5 8.7-5', 'M12 22V12'],
    'search': ['M11 3a8 8 0 1 0 0 16 8 8 0 1 0 0-16', 'm21 21-4.3-4.3'],
    'chev_left': ['m15 18-6-6 6-6'],
    'plus': ['M5 12h14', 'M12 5v14'],
    'help': ['M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20', 'M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3', 'M12 17h.01'],
    'play': ['M6 3 20 12 6 21Z'],
    'news': ['M4 22h16a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v16a2 2 0 0 1-2 2Zm0 0a2 2 0 0 1-2-2v-9c0-1.1.9-2 2-2h2', 'M18 14h-8', 'M15 18h-5', 'M10 6h8v4h-8V6Z'],
    'folder': ['M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z'],
    'link': ['M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71', 'M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71'],
    'copy': ['M8 8h12a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2Z', 'M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2'],
    'trash': ['M3 6h18', 'M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6', 'M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2'],
}


def icon(name, x, y, size=28, color=C['text'], sw=2):
    s = size / 24
    paths = ''.join(f'<path d="{d}"/>' for d in ICONS[name])
    return (f'<g transform="translate({x},{y}) scale({s})" fill="none" stroke="{color}" '
            f'stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round">{paths}</g>')


def svg(w, h, body, defs=''):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">'
            f'<defs>{defs}</defs>{body}</svg>')


def gradient_bg(w, h, stops, gid='bg'):
    s = ''.join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in stops)
    return (f'<linearGradient id="{gid}" x1="0" y1="0" x2="1" y2="1">{s}</linearGradient>',
            f'<rect width="{w}" height="{h}" fill="url(#{gid})"/>')


SHADOW = ('<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">'
          '<feDropShadow dx="0" dy="24" stdDeviation="30" flood-color="#000" flood-opacity="0.45"/></filter>')


def app_window(x, y, w, h, title='Modrinth Studios'):
    """A dimmed app window (behind a modal), like Modrinth's article shots."""
    b = f'<g filter="url(#shadow)">{rect(x, y, w, h, 26, C["win"], C["border"], 2)}</g>'
    for i, c in enumerate(['#ff5f57', '#febc2e', '#28c840']):
        b += f'<circle cx="{x + 34 + i * 26}" cy="{y + 32}" r="9" fill="{c}"/>'
    b += t(x + 130, y + 41, title, 24, 600, C['sec'])
    # sidebar + library cards, as quiet shapes
    for i in range(7):
        b += rect(x + 22, y + 92 + i * 64, 40, 40, 12, C['surf3'])
    for row in range(2):
        for col in range(4):
            cx, cy = x + 110 + col * ((w - 160) / 4), y + 140 + row * 300
            b += rect(cx, cy, (w - 200) / 4, 260, 20, C['win2'])
            b += rect(cx + 20, cy + 20, 120, 120, 18, ['#3b2f6e', '#24563f', '#5a3a1f', '#1f4060'][(row + col) % 4])
            b += rect(cx + 20, cy + 162, 180, 18, 9, C['surf4'])
            b += rect(cx + 20, cy + 194, 120, 14, 7, C['surf3'])
    b += rect(x, y, w, h, 26, '#000', extra='opacity="0.5"')
    return b


def modal(x, y, w, h, title):
    b = f'<g filter="url(#shadow)">{rect(x, y, w, h, 28, C["win2"], C["border"], 2)}</g>'
    b += t(x + 40, y + 70, title, 36, 700)
    b += f'<circle cx="{x + w - 58}" cy="{y + 58}" r="26" fill="{C["surf4"]}"/>'
    b += icon('x', x + w - 72, y + 44, 28, C['prim'], 2.4)
    b += f'<line x1="{x}" y1="{y + 112}" x2="{x + w}" y2="{y + 112}" stroke="{C["border"]}" stroke-width="2"/>'
    return b


def chip(x, y, label, selected=False, size=26, pad=26, h=60):
    w = text_w(label, size) + pad * 2 + (34 if selected else 0)
    if selected:
        b = rect(x, y, w, h, 16, C['brandhl'], C['brand'], 2.5)
        b += icon('check', x + pad - 4, y + h / 2 - 13, 26, C['brand'], 3)
        b += t(x + pad + 30, y + h / 2 + size * 0.36, label, size, 600, C['brand'])
    else:
        b = rect(x, y, w, h, 16, C['surf4'], C['surf5'], 2)
        b += t(x + pad, y + h / 2 + size * 0.36, label, size, 600, C['text'])
    return b, w


def monogram(x, y, size, letters, c1, c2, gid):
    d = (f'<linearGradient id="{gid}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{c1}"/>'
         f'<stop offset="1" stop-color="{c2}"/></linearGradient>')
    b = rect(x, y, size, size, size * 0.26, f'url(#{gid})')
    b += t(x + size / 2, y + size / 2 + size * 0.16, letters, size * 0.42, 800, '#fff', 'middle')
    return d, b


def save(svg_str, name, w):
    png = cairosvg.svg2png(bytestring=svg_str.encode(), output_width=w)
    Image.open(io.BytesIO(png)).convert('RGB').save(os.path.join(OUT, name), 'WEBP', quality=90)
    print('wrote', name)


CLIENTS = [
    ('Lunar Client', 'PvP client with built-in mods, from 1.8.9 to the newest versions.', 'LC', '#6b5cff', '#2a2268'),
    ('LabyMod', 'LabyMod 4 with its addons, cosmetics and menus.', 'LM', '#3aa0ff', '#174a8a'),
    ('NoRisk Client', 'Fabric-based client with cosmetics, replays and friends.', 'NR', '#ff4d6a', '#7a1730'),
    ('Horizon', 'Minecraft 26.3 rewritten in Rust, with its own mods and shaders.', 'H', '#9d7bff', '#3b2a7a'),
]

W, H = 1920, 1280
PURPLE = [(0, '#7b5cff'), (0.55, '#5b8cff'), (1, '#9be7ff')]
GREEN = [(0, '#0f6e45'), (0.5, '#1bd96a'), (1, '#b9ffd6')]
PINK = [(0, '#ff7aa8'), (0.5, '#a879ff'), (1, '#6fd3ff')]
BLUE = [(0, '#1b3c8f'), (0.5, '#3f7bff'), (1, '#7fe3ff')]


def wrap(text, size, width, weight=400):
    words, lines, cur = text.split(), [], ''
    for wd in words:
        nxt = (cur + ' ' + wd).strip()
        if text_w(nxt, size, weight) > width and cur:
            lines.append(cur)
            cur = wd
        else:
            cur = nxt
    if cur:
        lines.append(cur)
    return lines


# ---------- 1. Create instance ----------
def create_instance():
    gd, gb = gradient_bg(W, H, PURPLE)
    body = gb + app_window(110, 90, W - 220, H - 180)
    mx, my, mw, mh = 500, 150, 920, 1000
    body += modal(mx, my, mw, mh, 'Create instance')
    body += t(mx + 40, my + 168, 'Choose instance type', 28, 600)
    opts = [('boxes', 'Custom setup', 'Start from scratch by picking a loader and game version.'),
            ('compass', 'Start from a mod or modpack', 'Choose a project and we’ll use its latest version.'),
            ('upload', 'Upload a modpack', 'Install a modpack from an .mrpack file on your device.'),
            ('import', 'Import instance', 'Import an instance from Prism, CurseForge, or similar.'),
            ('rocket', 'Client installation', 'Play Lunar, LabyMod, NoRisk or Horizon from here.')]
    for i, (ic, title, desc) in enumerate(opts):
        oy = my + 200 + i * 148
        hl = ic == 'rocket'
        body += rect(mx + 40, oy, mw - 80, 128, 22, C['surf3'], C['brand'] if hl else C['surf4'], 3 if hl else 2)
        body += rect(mx + 64, oy + 26, 76, 76, 18, C['win2'], C['surf5'], 2)
        body += icon(ic, mx + 84, oy + 46, 36, C['brand'] if hl else C['prim'], 2)
        body += t(mx + 166, oy + 58, title, 30, 700)
        body += t(mx + 166, oy + 98, desc, 24, 400, C['sec'])
        if hl:
            body += icon('help', mx + 166 + text_w(title, 30, 700) + 14, oy + 35, 30, C['sec'], 2)
    save(svg(W, H, body, gd + SHADOW), 'create-instance.webp', W)


# ---------- 2. Choose a client ----------
def choose_client():
    gd, gb = gradient_bg(W, H, PINK)
    defs = gd + SHADOW
    body = gb + app_window(110, 90, W - 220, H - 180)
    mx, my, mw, mh = 440, 190, 1040, 920
    body += modal(mx, my, mw, mh, 'Client installation')
    body += t(mx + 40, my + 172, 'Choose a client', 28, 600)
    cw, ch = (mw - 104) / 2, 320
    for i, (name, tag, letters, c1, c2) in enumerate(CLIENTS):
        cx = mx + 40 + (i % 2) * (cw + 24)
        cy = my + 208 + (i // 2) * (ch + 24)
        sel = i == 0
        body += rect(cx, cy, cw, ch, 26, C['surf3'], C['brand'] if sel else C['surf4'], 3 if sel else 2)
        d, b = monogram(cx + 32, cy + 32, 96, letters, c1, c2, f'm{i}')
        defs += d
        body += b
        body += icon('help', cx + cw - 64, cy + 34, 32, C['sec'], 2)
        body += t(cx + 32, cy + 188, name, 34, 700)
        for j, line in enumerate(wrap(tag, 25, cw - 64)):
            body += t(cx + 32, cy + 232 + j * 34, line, 25, 400, C['sec'])
    save(svg(W, H, body, defs), 'choose-client.webp', W)


# ---------- 3. Version and module ----------
def client_setup():
    gd, gb = gradient_bg(W, H, BLUE)
    defs = gd + SHADOW
    body = gb + app_window(110, 90, W - 220, H - 180)
    mx, my, mw, mh = 520, 130, 880, 1030
    body += modal(mx, my, mw, mh, 'Client installation')
    # picked client card
    cy = my + 148
    body += rect(mx + 40, cy, mw - 80, 128, 24, C['win'])
    d, b = monogram(mx + 64, cy + 20, 88, 'LC', '#6b5cff', '#2a2268', 'ms')
    defs += d
    body += b
    body += t(mx + 176, cy + 58, 'Lunar Client', 30, 700)
    body += t(mx + 176, cy + 96, 'PvP client with built-in mods.', 24, 400, C['sec'])
    body += icon('chev_left', mx + mw - 210, cy + 50, 28, C['prim'], 2.4)
    body += t(mx + mw - 176, cy + 74, 'Change', 26, 600, C['prim'])
    # name
    y = cy + 180
    body += t(mx + 40, y, 'Name', 28, 600)
    body += rect(mx + 40, y + 20, mw - 80, 70, 16, C['surf3'])
    body += t(mx + 66, y + 66, 'Lunar Client 1.21.10', 26, 400, C['sec'])
    # versions
    y += 150
    body += t(mx + 40, y, 'Game version', 28, 600)
    body += t(mx + mw - 40, y, 'All 34 versions', 24, 600, C['brand'], 'end')
    x, row = mx + 40, y + 24
    for v, sel in [('1.21.10 · Recommended', True), ('1.21.8', False), ('1.21.5', False), ('1.21.1', False),
                   ('1.20.4', False), ('1.8.9 · Recommended', False)]:
        cb, cwid = chip(x, row, v, sel)
        if x + cwid > mx + mw - 40:
            x, row = mx + 40, row + 76
            cb, cwid = chip(x, row, v, sel)
        body += cb
        x += cwid + 16
    # module
    y = row + 130
    body += t(mx + 40, y, 'Module', 28, 600)
    body += icon('help', mx + 40 + text_w('Module', 28) + 12, y - 25, 30, C['sec'], 2)
    x = mx + 40
    for v, sel in [('Lunar', False), ('Lunar + Fabric', True)]:
        cb, cwid = chip(x, y + 24, v, sel)
        body += cb
        x += cwid + 16
    # footer
    fy = my + mh - 110
    body += rect(mx + 40, fy, 150, 66, 16, 'none', C['surf5'], 2)
    body += icon('chev_left', mx + 60, fy + 19, 28, C['text'], 2.4)
    body += t(mx + 98, fy + 43, 'Back', 26, 600)
    bw = text_w('Create instance', 26) + 96
    body += rect(mx + mw - 40 - bw, fy, bw, 66, 16, C['brand'])
    body += icon('plus', mx + mw - 40 - bw + 24, fy + 19, 28, '#0b1a10', 3)
    body += t(mx + mw - 40 - bw + 64, fy + 43, 'Create instance', 26, 700, '#0b1a10')
    save(svg(W, H, body, defs), 'client-setup.webp', W)


# ---------- 4. Copy from the client's launcher ----------
def copy_from():
    gd, gb = gradient_bg(W, H, GREEN)
    body = gb + app_window(110, 90, W - 220, H - 180)
    mx, my, mw, mh = 470, 230, 980, 820
    body += modal(mx, my, mw, mh, 'Client installation')
    by = my + 150
    body += rect(mx + 40, by, mw - 80, mh - 200, 26, C['win'])
    body += t(mx + 76, by + 66, 'Copy from Lunar Client', 32, 700)
    for j, line in enumerate(wrap('Bring servers, settings and packs over from Lunar Client’s own launcher.', 24, mw - 330)):
        body += t(mx + 76, by + 108 + j * 32, line, 24, 400, C['sec'])
    # toggle (on)
    tx = mx + mw - 160
    body += rect(tx, by + 46, 84, 46, 23, C['brand'])
    body += f'<circle cx="{tx + 61}" cy="{by + 69}" r="17" fill="#0b1a10"/>'
    # select
    body += rect(mx + 76, by + 170, mw - 152, 70, 16, C['surf3'])
    body += t(mx + 104, by + 216, 'Lunar profile 1.21', 26, 600)
    body += f'<path d="M{mx + mw - 120} {by + 198} l12 12 12-12" fill="none" stroke="{C["prim"]}" stroke-width="3" stroke-linecap="round"/>'
    items = [('Servers', True), ('Settings and keybinds', True), ('Resource packs (12)', True),
             ('Shader packs (3)', True), ('Screenshots (148)', False), ('Worlds (6)', False), ('Fabric mods (8)', True)]
    for i, (lab, on) in enumerate(items):
        ix = mx + 76 + (i % 2) * ((mw - 152) / 2)
        iy = by + 290 + (i // 2) * 76
        if on:
            body += rect(ix, iy, 40, 40, 10, C['brand'])
            body += icon('check', ix + 6, iy + 6, 28, '#0b1a10', 3.4)
        else:
            body += rect(ix, iy, 40, 40, 10, C['surf3'], C['surf5'], 2)
        body += t(ix + 58, iy + 30, lab, 26, 500 if not on else 600, C['text'] if on else C['prim'])
    save(svg(W, H, body, gd + SHADOW), 'copy-from-lunar.webp', W)


# ---------- 5. Mods next to Lunar ----------
def lunar_content():
    gd, gb = gradient_bg(W, H, PURPLE)
    defs = gd + SHADOW
    x0, y0, w0, h0 = 110, 90, W - 220, H - 180
    body = gb + f'<g filter="url(#shadow)">{rect(x0, y0, w0, h0, 26, C["win"], C["border"], 2)}</g>'
    for i, c in enumerate(['#ff5f57', '#febc2e', '#28c840']):
        body += f'<circle cx="{x0 + 34 + i * 26}" cy="{y0 + 32}" r="9" fill="{c}"/>'
    body += t(x0 + 130, y0 + 41, 'Modrinth Studios', 24, 600, C['sec'])
    for i in range(7):
        body += rect(x0 + 22, y0 + 92 + i * 64, 40, 40, 12, C['surf3'])
    cx = x0 + 110
    # header
    d, b = monogram(cx, y0 + 90, 104, 'LC', '#6b5cff', '#2a2268', 'mh')
    defs += d
    body += b
    body += t(cx + 132, y0 + 136, 'Lunar Client 1.21.10', 40, 700)
    tw = text_w('Lunar Client 1.21.10', 40, 700)
    body += rect(cx + 150 + tw, y0 + 104, text_w('Lunar Client', 22) + 32, 42, 21, 'rgba(166,120,255,0.2)', C['purple'], 2)
    body += t(cx + 166 + tw, y0 + 133, 'Lunar Client', 22, 600, C['purple'])
    body += t(cx + 132, y0 + 178, 'Lunar + Fabric  ·  Fabric 1.21.10', 24, 500, C['sec'])
    pw = text_w('Play', 28) + 90
    body += rect(x0 + w0 - 60 - pw, y0 + 106, pw, 70, 18, C['brand'])
    body += icon('play', x0 + w0 - 60 - pw + 26, y0 + 127, 28, '#0b1a10', 2.6)
    body += t(x0 + w0 - 60 - pw + 64, y0 + 151, 'Play', 28, 700, '#0b1a10')
    # tabs
    ty = y0 + 240
    tabs = [('News', False), ('Content', True), ('Files', False), ('Screenshots', False), ('Worlds', False), ('Logs', False)]
    tx = cx
    body += rect(cx - 10, ty - 10, 860, 76, 24, C['win2'])
    for lab, on in tabs:
        tw = text_w(lab, 24) + 48
        if on:
            body += rect(tx, ty, tw, 56, 18, C['brandhl'])
        body += t(tx + 24, ty + 37, lab, 24, 600, C['brand'] if on else C['prim'])
        tx += tw + 14
    # table
    ry = ty + 110
    tw = w0 - 170
    body += rect(cx, ry, tw, 70, 16, C['surf3'])
    body += icon('search', cx + 22, ry + 21, 28, C['sec'], 2.4)
    body += t(cx + 66, ry + 45, 'Search 5 projects…', 24, 400, C['sec'])
    bw = text_w('Browse content', 24) + 60
    body += rect(cx + tw - bw, ry, bw, 70, 16, C['brand'])
    body += t(cx + tw - bw + 30, ry + 44, 'Browse content', 24, 700, '#0b1a10')
    hy = ry + 100
    body += rect(cx, hy, tw, h0 - (hy - y0) - 40, 22, C['win2'], C['surf3'], 2)
    body += t(cx + 30, hy + 50, 'Project', 22, 600, C['sec'])
    body += t(cx + tw * 0.52, hy + 50, 'Version', 22, 600, C['sec'])
    body += t(cx + tw - 30, hy + 50, 'Actions', 22, 600, C['sec'], 'end')
    mods = [('Sodium', 'CaffeineMC', 'mc1.21.10-0.7.3', '#2bd47a', '#0d6b3a'),
            ('Iris Shaders', 'coderbot', '1.9.7', '#7e5bff', '#2e1f73'),
            ('Mod Menu', 'Prospector', '16.0.0', '#ffb84d', '#7a4a0d'),
            ('AppleSkin', 'squeek502', '3.0.10', '#ff5b5b', '#7a1c1c'),
            ('Fabric API', 'modmuss50', '0.138.4', '#d9c39b', '#6b5a3a')]
    for i, (name, author, ver, c1, c2) in enumerate(mods):
        yy = hy + 80 + i * 106
        body += f'<line x1="{cx}" y1="{yy}" x2="{cx + tw}" y2="{yy}" stroke="{C["surf3"]}" stroke-width="2"/>'
        d, b = monogram(cx + 30, yy + 18, 70, name[0], c1, c2, f'mm{i}')
        defs += d
        body += b
        body += t(cx + 124, yy + 50, name, 26, 700)
        body += t(cx + 124, yy + 84, author, 22, 400, C['sec'])
        body += t(cx + tw * 0.52, yy + 62, ver, 24, 600, C['prim'])
        # toggle on + trash
        tgx = cx + tw - 170
        body += rect(tgx, yy + 32, 76, 42, 21, C['brand'])
        body += f'<circle cx="{tgx + 55}" cy="{yy + 53}" r="15" fill="#0b1a10"/>'
        body += icon('trash', cx + tw - 70, yy + 40, 28, C['sec'], 2.2)
    save(svg(W, H, body, defs), 'lunar-content.webp', W)


# ---------- Thumbnail / banner ----------
def thumbnail():
    w, h = 1920, 1080
    defs = ('<linearGradient id="tb" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#2a1d5e"/>'
            '<stop offset="1" stop-color="#140f2e"/></linearGradient>'
            '<linearGradient id="gloss" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="0.22"/>'
            '<stop offset="1" stop-color="#ffffff" stop-opacity="0.04"/></linearGradient>'
            '<radialGradient id="glow" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#1bd96a" stop-opacity="0.45"/>'
            '<stop offset="1" stop-color="#1bd96a" stop-opacity="0"/></radialGradient>' + SHADOW)
    body = f'<rect width="{w}" height="{h}" fill="url(#tb)"/>'
    # isometric block pattern, like Modrinth's thumbnails
    s = 120
    for row in range(-2, 18):
        for col in range(-2, 20):
            cx = col * s * 1.0 + (row % 2) * s * 0.5
            cy = row * s * 0.58
            top = f'M{cx} {cy} l{s/2} {s*0.29} l{-s/2} {s*0.29} l{-s/2} {-s*0.29}Z'
            left = f'M{cx - s/2} {cy + s*0.29} l{s/2} {s*0.29} v{s*0.58} l{-s/2} {-s*0.29}Z'
            right = f'M{cx} {cy + s*0.58} l{s/2} {-s*0.29} v{s*0.58} l{-s/2} {s*0.29}Z'
            k = (row * 7 + col * 3) % 5
            body += f'<path d="{top}" fill="#ffffff" opacity="{0.035 + k*0.008}"/>'
            body += f'<path d="{left}" fill="#000000" opacity="{0.10 + k*0.01}"/>'
            body += f'<path d="{right}" fill="#000000" opacity="{0.18 + k*0.01}"/>'
    # three instance cards on the left (one per client, monograms only)
    for i, (name, _, letters, c1, c2) in enumerate(CLIENTS[:3]):
        y = 300 + i * 170
        x = 330 + i * 40
        card = rect(x, y, 640, 130, 32, '#ffffff', extra='opacity="0.10"')
        body += f'<g filter="url(#shadow)">{card}</g>'
        body += rect(x, y, 640, 130, 32, 'url(#gloss)', 'rgba(255,255,255,0.25)', 2)
        d, b = monogram(x + 26, y + 21, 88, letters, c1, c2, f'tm{i}')
        defs += d
        body += b
        body += rect(x + 140, y + 38, 300, 22, 11, '#ffffff', extra='opacity="0.75"')
        body += rect(x + 140, y + 76, 200, 16, 8, '#ffffff', extra='opacity="0.35"')
        body += rect(x + 520, y + 38, 90, 54, 18, '#1bd96a')
        body += icon('play', x + 551, y + 51, 28, '#0b1a10', 2.6)
    # glossy rocket circle on the right
    rx, ry, r = 1440, 540, 170
    body += f'<circle cx="{rx}" cy="{ry}" r="{r*1.9}" fill="url(#glow)"/>'
    body += f'<g filter="url(#shadow)"><circle cx="{rx}" cy="{ry}" r="{r}" fill="#123b2a"/></g>'
    body += f'<circle cx="{rx}" cy="{ry}" r="{r}" fill="url(#gloss)" stroke="rgba(255,255,255,0.25)" stroke-width="3"/>'
    body += icon('rocket', rx - 100, ry - 100, 200, '#ffffff', 1.6)
    save(svg(w, h, body, defs), 'banner.webp', w)


for f in (thumbnail, create_instance, choose_client, client_setup, copy_from, lunar_content):
    f()
