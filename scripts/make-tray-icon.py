"""Render the menu-bar glyph.

A macOS template image carries no colour — only coverage — so the app icon's
orange ticks and white bars all flatten to one block. This draws the same
motif (three items, two done, one still to go) as pure alpha instead.
"""
import math, zlib, struct

UNITS = 18.0          # tray-icon scales the image to 18pt tall
PX = 36               # 2x, for retina
SS = 4                # supersampling per axis

def dist_seg(px, py, x0, y0, x1, y1):
    dx, dy = x1 - x0, y1 - y0
    l2 = dx * dx + dy * dy
    t = 0.0 if l2 == 0 else max(0.0, min(1.0, ((px - x0) * dx + (py - y0) * dy) / l2))
    return math.hypot(px - (x0 + t * dx), py - (y0 + t * dy))

def polyline(pts, w):
    half = w / 2.0
    def f(x, y):
        return min(dist_seg(x, y, *pts[i], *pts[i + 1]) for i in range(len(pts) - 1)) <= half
    return f

def ring(cx, cy, r, w):
    half = w / 2.0
    return lambda x, y: abs(math.hypot(x - cx, y - cy) - r) <= half

def rounded_rect(x0, y0, x1, y1, r):
    def f(x, y):
        qx = max(x0 + r - x, 0.0, x - (x1 - r))
        qy = max(y0 + r - y, 0.0, y - (y1 - r))
        return math.hypot(qx, qy) <= r and x0 <= x <= x1 and y0 <= y <= y1
    return f

# --- the mark -------------------------------------------------------------
# Ticks rather than filled discs: at 18pt a disc with a knocked-out check is
# mush, whereas two strokes stay legible.
ROWS = (3.7, 9.0, 14.3)
BAR_X0, BAR_X1, BAR_H = 7.6, 16.6, 1.9
TICK = 1.4
shapes = []
for i, y in enumerate(ROWS):
    shapes.append(rounded_rect(BAR_X0, y - BAR_H / 2, BAR_X1, y + BAR_H / 2, BAR_H / 2))
    if i < 2:
        shapes.append(polyline([(1.8, y + 0.1), (3.2, y + 1.4), (5.8, y - 1.6)], TICK))
    else:
        shapes.append(ring(3.8, y, 1.75, 1.3))

def coverage(px, py):
    hits = 0
    for sy in range(SS):
        for sx in range(SS):
            x = (px + (sx + 0.5) / SS) * UNITS / PX
            y = (py + (sy + 0.5) / SS) * UNITS / PX
            if any(s(x, y) for s in shapes):
                hits += 1
    return hits / (SS * SS)

rows = []
for py in range(PX):
    row = bytearray([0])                      # filter: none
    for px in range(PX):
        a = int(round(coverage(px, py) * 255))
        row += bytes((0, 0, 0, a))            # black; macOS tints the template
    rows.append(bytes(row))

def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", PX, PX, 8, 6, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(b"".join(rows), 9))
       + chunk(b"IEND", b""))
open("src-tauri/icons/tray.png", "wb").write(png)
print(f"wrote {PX}x{PX} template icon, {len(png)} bytes")
