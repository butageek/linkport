#!/usr/bin/env python3
"""Generate resources/icon32.rgba — 32x32 RGBA tray icon for Linkport.

Chain-link rings on an indigo rounded square. No dependencies; run from the
repo root: python3 scripts/gen_icon.py
"""

import math
import os

W = 32          # output size
SS = 4          # supersampling factor
S = W * SS      # render size

INDIGO = (79, 70, 229, 255)
WHITE = (255, 255, 255, 255)
TRANSPARENT = (0, 0, 0, 0)

px = [[TRANSPARENT] * S for _ in range(S)]


def in_rounded_rect(x, y, x0, y0, x1, y1, r):
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def on_ring(x, y, cx, cy, radius, thickness):
    return abs(math.hypot(x - cx, y - cy) - radius) <= thickness


for y in range(S):
    for x in range(S):
        if in_rounded_rect(x, y, 6, 6, S - 6, S - 6, 18):
            px[y][x] = INDIGO

# Two interlocked rings (a chain link).
for y in range(S):
    for x in range(S):
        if on_ring(x, y, 44, 64, 17, 4) or on_ring(x, y, 84, 64, 17, 4):
            px[y][x] = WHITE

out = bytearray()
for y in range(W):
    for x in range(W):
        r = g = b = a = 0
        for dy in range(SS):
            for dx in range(SS):
                c = px[y * SS + dy][x * SS + dx]
                r += c[0]
                g += c[1]
                b += c[2]
                a += c[3]
        n = SS * SS
        out += bytes((r // n, g // n, b // n, a // n))

path = os.path.join(os.path.dirname(__file__), "..", "resources", "icon32.rgba")
os.makedirs(os.path.dirname(path), exist_ok=True)
with open(path, "wb") as f:
    f.write(out)
print(f"wrote {os.path.normpath(path)} ({len(out)} bytes)")
