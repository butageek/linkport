#!/usr/bin/env python3
"""Generate Linkport's icon assets in resources/.

Interlocked chain rings (over-under weave) on an indigo rounded square
with a subtle diagonal gradient, following the Windows 11 Fluent icon
guidance (single metaphor, legible silhouette, soft corners, one-hue
gradient). No dependencies; run from the repo root:
python3 scripts/gen_icon.py

Outputs:
- icon32.rgba      — 32x32 raw RGBA (tray icon, include_bytes'd by tray.rs)
- icon32-grey.rgba — desaturated variant for the paused-routing tray state
- icon.ico         — multi-size ICO (16/24/32/48/64 as DIB + 256 as PNG),
  embedded into the Windows exes by build.rs (winresource) and referenced
  by the DefaultIcon registry values written at registration.

The portal's inline SVG mark (frontend/src/components/logo.tsx) mirrors
this geometry — keep the two in sync.
"""

import math
import os
import struct
import zlib

SS = 4  # supersampling factor

INDIGO_LIGHT = (99, 102, 241)  # indigo-500 — gradient top-left
INDIGO_DARK = (67, 56, 202)    # indigo-700 — gradient bottom-right
WHITE = (255, 255, 255, 255)

# Geometry in 32-unit reference space.
RRECT = (1.5, 1.5, 30.5, 30.5, 7.0)  # x0, y0, x1, y1, corner radius
RING_A = (12, 16)                    # left ring center
RING_B = (20, 16)                    # right ring center
RING_R = 4.25                        # stroke centerline radius
RING_W = 2.0                         # stroke width
CUT_B = (16, 12.6, 2.1)              # gap in right ring: left passes over
CUT_A = (16, 19.4, 2.1)              # gap in left ring: right passes over


def render_rgba(w):
    """RGBA rows (top-down) for a w x w icon, supersampled SS x."""
    s = w * SS
    g = s / 32.0
    x0, y0, x1, y1, rad = (c * g for c in RRECT)

    def in_tile(px, py):
        cx = min(max(px, x0 + rad), x1 - rad)
        cy = min(max(py, y0 + rad), y1 - rad)
        return (px - cx) ** 2 + (py - cy) ** 2 <= rad * rad

    def in_ring(px, py, c):
        return abs(math.hypot(px - c[0] * g, py - c[1] * g) - RING_R * g) <= RING_W * g / 2

    def in_disc(px, py, d):
        return math.hypot(px - d[0] * g, py - d[1] * g) <= d[2] * g

    def glyph(px, py):
        # Over-under weave: each ring's stroke breaks where the other
        # passes over it (see CUT_A / CUT_B).
        if in_ring(px, py, RING_B) and in_disc(px, py, CUT_B):
            return False
        if in_ring(px, py, RING_A) and in_disc(px, py, CUT_A):
            return False
        return in_ring(px, py, RING_A) or in_ring(px, py, RING_B)

    out = bytearray()
    n = SS * SS
    for y in range(w):
        for x in range(w):
            r_ = g_ = b_ = a_ = 0
            for dy in range(SS):
                for dx in range(SS):
                    px, py = x * SS + dx + 0.5, y * SS + dy + 0.5
                    if not in_tile(px, py):
                        continue
                    a_ += 255
                    if glyph(px, py):
                        r_ += WHITE[0]
                        g_ += WHITE[1]
                        b_ += WHITE[2]
                    else:
                        # subtle diagonal gradient, light from top-left
                        t = (px + py) / (2 * s)
                        r_ += INDIGO_LIGHT[0] + (INDIGO_DARK[0] - INDIGO_LIGHT[0]) * t
                        g_ += INDIGO_LIGHT[1] + (INDIGO_DARK[1] - INDIGO_LIGHT[1]) * t
                        b_ += INDIGO_LIGHT[2] + (INDIGO_DARK[2] - INDIGO_LIGHT[2]) * t
            out += bytes((int(r_ // n), int(g_ // n), int(b_ // n), a_ // n))
    return bytes(out)


def greyify(rgba, brightness=0.55):
    """Desaturate for the paused-state tray icon (keeps alpha)."""
    out = bytearray()
    for i in range(0, len(rgba), 4):
        r, g, b, a = rgba[i:i + 4]
        lum = int((r * 299 + g * 587 + b * 114) / 1000 * brightness)
        out += bytes((lum, lum, lum, a))
    return bytes(out)


def png(w, rgba):
    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    raw = b"".join(b"\x00" + rgba[y * w * 4:(y + 1) * w * 4] for y in range(w))
    ihdr = struct.pack(">IIBBBBB", w, w, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def dib(w, rgba):
    """ICO image entry as a 32bpp bottom-up DIB with (empty) AND mask."""
    hdr = struct.pack("<IiiHHIiiII", 40, w, w * 2, 1, 32, 0, 0, 0, 0, 0)
    rows = []
    for y in range(w - 1, -1, -1):  # bottom-up, BGRA
        row = bytearray()
        for x in range(w):
            r_, g_, b_, a_ = rgba[(y * w + x) * 4:(y * w + x) * 4 + 4]
            row += bytes((b_, g_, r_, a_))
        rows.append(bytes(row))
    mask_row = b"\x00" * (((w + 31) // 32) * 4)
    return hdr + b"".join(rows) + mask_row * w


def write_ico(path, sizes_dib, sizes_png):
    images = []
    for w in sizes_dib:
        images.append((w, dib(w, render_rgba(w))))
    for w in sizes_png:
        images.append((w, png(w, render_rgba(w))))

    entries = bytearray()
    offset = 6 + 16 * len(images)
    blob = bytearray()
    for w, data in images:
        entries += struct.pack("<BBBBHHII", w % 256, w % 256, 0, 0, 1, 32,
                               len(data), offset)
        blob += data
        offset += len(data)
    with open(path, "wb") as f:
        f.write(struct.pack("<HHH", 0, 1, len(images)) + entries + blob)


def main():
    here = os.path.dirname(__file__)
    res = os.path.normpath(os.path.join(here, "..", "resources"))

    rgba = render_rgba(32)
    p = os.path.join(res, "icon32.rgba")
    with open(p, "wb") as f:
        f.write(rgba)
    print(f"wrote {p} ({len(rgba)} bytes)")

    p = os.path.join(res, "icon32-grey.rgba")
    with open(p, "wb") as f:
        f.write(greyify(rgba))
    print(f"wrote {p} ({os.path.getsize(p)} bytes)")

    p = os.path.join(res, "icon.ico")
    write_ico(p, sizes_dib=(16, 24, 32, 48, 64), sizes_png=(256,))
    print(f"wrote {p} ({os.path.getsize(p)} bytes)")


if __name__ == "__main__":
    main()
