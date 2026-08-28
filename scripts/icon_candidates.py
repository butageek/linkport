#!/usr/bin/env python3
"""Render Linkport app-icon candidates to dist/icon-candidates/ for review.

Each candidate follows the Windows 11 Fluent guidance: one literal
metaphor, white rounded glyph on an indigo rounded square with a subtle
same-hue diagonal gradient, legible when small. Pure-stdlib renderer with
supersampling. Not part of the build; the winner gets folded into
scripts/gen_icon.py.

Run: python3 scripts/icon_candidates.py
"""

import math
import os
import struct
import zlib

OUT = 256      # output size
SS = 3         # supersample factor
S = OUT * SS   # render size

# 32-unit reference grid (Fluent uses 48; ours is historic 32) -> scale.
G = S / 32.0

INDIGO_LIGHT = (99, 102, 241)    # indigo-500, top-left
INDIGO_DARK = (67, 56, 202)      # indigo-700, bottom-right
WHITE = (255, 255, 255, 255)


# ---------- png ----------

def write_png(path, w, h, rgba):
    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    raw = b"".join(b"\x00" + rgba[y * w * 4:(y + 1) * w * 4] for y in range(h))
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
                + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


# ---------- shapes (all in 32-grid floats) ----------

def dist_seg(px, py, ax, ay, bx, by):
    ax, ay, bx, by = ax * G, ay * G, bx * G, by * G
    dx, dy = bx - ax, by - ay
    if dx == dy == 0:
        return math.hypot(px - ax, py - ay)
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def capsule(px, py, ax, ay, bx, by, r):
    return dist_seg(px, py, ax, ay, bx, by) <= r * G


def disc(px, py, cx, cy, r):
    return math.hypot(px - cx * G, py - cy * G) <= r * G


def ring(px, py, cx, cy, radius, w):
    return abs(math.hypot(px - cx * G, py - cy * G) - radius * G) <= w * G / 2


def in_triangle(px, py, a, b, c):
    def side(x1, y1, x2, y2):
        return (px - x1 * G) * (y2 - y1) * G - (py - y1 * G) * (x2 - x1) * G
    d1, d2, d3 = side(*a, *b), side(*b, *c), side(*c, *a)
    neg = d1 < 0 or d2 < 0 or d3 < 0
    pos = d1 > 0 or d2 > 0 or d3 > 0
    return not (neg and pos)


def in_trapezoid(px, py, xtl, xtr, xbl, xbr, yt, yb):
    x, y = px / G, py / G
    if not (yt <= y <= yb):
        return False
    t = (y - yt) / (yb - yt) if yb > yt else 0
    xl = xtl + (xbl - xtl) * t
    xr = xtr + (xbr - xtr) * t
    return xl <= x <= xr


def rounded_square(px, py):
    # 1.5..30.5 with 7-unit corner radius (Fluent-ish soft corners)
    x, y = px / G, py / G
    x0, y0, x1, y1, r = 1.5, 1.5, 30.5, 30.5, 7.0
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


# ---------- candidates ----------

def glyph_fork(px, py):
    g = (capsule(px, py, 4.5, 16, 14, 16, 2.3)
         or capsule(px, py, 13.5, 15.5, 23.5, 8, 2.3)
         or capsule(px, py, 13.5, 16.5, 23.5, 24, 2.3))
    g = g or disc(px, py, 25.5, 6.7, 2.0) or disc(px, py, 25.5, 25.3, 2.0)
    return g


def glyph_link_arrow(px, py):
    g = ring(px, py, 12, 16, 4.6, 2.3)
    g = g or capsule(px, py, 4.5, 16, 20.5, 16, 1.7)
    g = g or in_triangle(px, py, (20, 12.6), (20, 19.4), (26, 16))
    return g


def glyph_interchange(px, py):
    g = (capsule(px, py, 5.5, 10.5, 14, 10.5, 2.3)
         or capsule(px, py, 14, 10.5, 26.5, 21.5, 2.3))
    g = g or capsule(px, py, 5.5, 21.5, 14, 21.5, 2.3)
    g = g or capsule(px, py, 14, 21.5, 26.5, 10.5, 2.3)
    return g


def glyph_lighthouse(px, py):
    g = in_trapezoid(px, py, 14.6, 17.4, 13.2, 18.8, 11.5, 25.5)  # tower
    g = g or capsule(px, py, 11.5, 25.5, 20.5, 25.5, 1.6)          # base
    g = g or capsule(px, py, 14, 11.5, 18, 11.5, 1.6)              # gallery
    g = g or disc(px, py, 16, 9.2, 1.9)                            # lamp
    g = g or in_triangle(px, py, (17.5, 8.2), (27, 4.5), (27, 9.4))
    g = g or in_triangle(px, py, (17.5, 10.2), (27, 9.0), (27, 14.0))
    return g


def glyph_current(px, py):
    return ring(px, py, 11, 16, 4.25, 2.0) or ring(px, py, 21, 16, 4.25, 2.0)


def glyph_current_grad(px, py):
    # same geometry as the deployed icon, but on the gradient tile
    return glyph_current(px, py)


def glyph_chain2(px, py):
    in_l = ring(px, py, 12, 16, 4.25, 2.0)
    in_r = ring(px, py, 20, 16, 4.25, 2.0)
    cut_r = disc(px, py, 16, 12.6, 2.1)   # gap in right ring (left passes over)
    cut_l = disc(px, py, 16, 19.4, 2.1)   # gap in left ring (right passes over)
    if in_r and cut_r:
        return False
    if in_l and cut_l:
        return False
    return in_l or in_r


CANDIDATES = [
    ("a-fork", glyph_fork),
    ("b-link-arrow", glyph_link_arrow),
    ("e-current", glyph_current),
    ("f-current-gradient", glyph_current_grad),
    ("g-chain-weave", glyph_chain2),
]


def render(glyph, size):
    """Render at `size` (multiple of 4); returns RGBA bytes."""
    global G
    old_g, old_ss = G, SS
    # reuse module S/G by scaling: render at size*SS then downsample
    G = (size * SS) / 32.0
    s = size * SS
    hi = bytearray()
    for y in range(s):
        for x in range(s):
            if not rounded_square(x + 0.5, y + 0.5):
                hi += b"\x00\x00\x00\x00"
                continue
            t = (x + y) / (2 * s)  # subtle diagonal gradient, light top-left
            r = INDIGO_LIGHT[0] + (INDIGO_DARK[0] - INDIGO_LIGHT[0]) * t
            g = INDIGO_LIGHT[1] + (INDIGO_DARK[1] - INDIGO_LIGHT[1]) * t
            b = INDIGO_LIGHT[2] + (INDIGO_DARK[2] - INDIGO_LIGHT[2]) * t
            if glyph(x + 0.5, y + 0.5):
                hi += bytes(WHITE)
            else:
                hi += bytes((int(r), int(g), int(b), 255))
    out = bytearray()
    n = SS * SS
    for y in range(size):
        for x in range(size):
            r = g = b = a = 0
            for dy in range(SS):
                for dx in range(SS):
                    i = ((y * SS + dy) * s + x * SS + dx) * 4
                    r += hi[i]
                    g += hi[i + 1]
                    b += hi[i + 2]
                    a += hi[i + 3]
            out += bytes((r // n, g // n, b // n, a // n))
    G = old_g
    return bytes(out)


def downsample(rgba, size, factor):
    out = bytearray()
    n = factor * factor
    for y in range(size):
        for x in range(size):
            r = g = b = a = 0
            for dy in range(factor):
                for dx in range(factor):
                    i = ((y * factor + dy) * size * factor + x * factor + dx) * 4
                    r += rgba[i]
                    g += rgba[i + 1]
                    b += rgba[i + 2]
                    a += rgba[i + 3]
            out += bytes((r // n, g // n, b // n, a // n))
    return bytes(out), size // factor


def main():
    here = os.path.dirname(__file__)
    out_dir = os.path.normpath(os.path.join(here, "..", "dist", "icon-candidates"))
    os.makedirs(out_dir, exist_ok=True)

    renders = []
    for name, glyph in CANDIDATES:
        rgba = render(glyph, 256)
        p = os.path.join(out_dir, f"{name}.png")
        write_png(p, 256, 256, rgba)
        print("wrote", p)
        renders.append((name, rgba))

    # Contact sheet: each candidate at 160px + 32px + 16px for size legibility.
    cell, pad, gap = 176, 8, 28
    label_h = 44
    sheet_w = pad + len(renders) * (cell + gap)
    sheet_h = pad * 2 + cell + label_h
    sheet = bytearray(b"\xe5\xe7\xeb\xff" * sheet_w * sheet_h)

    def blit(img, size, ox, oy):
        for y in range(size):
            for x in range(size):
                i = (y * size + x) * 4
                if img[i + 3] == 0:
                    continue
                j = ((oy + y) * sheet_w + ox + x) * 4
                sheet[j:j + 4] = img[i:i + 4]

    for idx, (name, rgba) in enumerate(renders):
        big, big_s = downsample(rgba, 256, 256 // 160)
        blit(big, big_s, pad + idx * (cell + gap) + 8, pad + 8)
        small32, s32 = downsample(rgba, 32, 8)
        small16, s16 = downsample(small32, 16, 2)
        blit(small32, s32, pad + idx * (cell + gap) + 8, pad + cell - 36)
        blit(small16, s16, pad + idx * (cell + gap) + 52, pad + cell - 36)

    p = os.path.join(out_dir, "contact-sheet.png")
    write_png(p, sheet_w, sheet_h, bytes(sheet))
    print("wrote", p)


if __name__ == "__main__":
    main()
