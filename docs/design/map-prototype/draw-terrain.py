# SPDX-License-Identifier: MPL-2.0
"""Draw original, deterministic review terrain. Coordinates are synthetic blocks."""
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

OUT = Path(__file__).parent
N = 512
rng = np.random.default_rng(1841)
z, x = np.mgrid[:N, :N]


def noise(grid):
    points = (rng.random((grid, grid)) * 255).astype("uint8")
    image = Image.fromarray(points).resize((N, N), Image.Resampling.BICUBIC)
    return np.asarray(image, dtype=float) / 255 - 0.5


landform = noise(6) * 30 + noise(18) * 13 + noise(48) * 4
height = 35 + landform + 44 * np.exp(-((x - 410) / 110)**2 - ((z - 88) / 180)**2)
river = 177 + 0.16 * z + 38 * np.sin(z / 61) - 20 * np.cos(z / 31)
distance = np.abs(x - river + noise(58) * 3)
water = distance < 13 + 4 * np.cos(z / 42)
bank = distance < 18 + 5 * np.cos(z / 42)
moist = noise(12) * 14 + noise(37) * 6
base = np.zeros((N, N, 3), dtype=float)
base[:] = [122, 141, 89]
base[moist > 0] = [103, 129, 83]
base[moist > 2] = [79, 105, 77]
base[height > 53] = [124, 128, 108]
base[height > 63] = [154, 156, 140]
base[height > 71] = [195, 199, 183]
base[bank] = [178, 170, 119]
base[water] = [65, 112, 125]
base[water & (distance < 8)] = [56, 95, 116]
relief = np.floor(height / 1.7)
shade = np.clip((relief - np.roll(relief, 1, axis=0)) * 14 +
                (relief - np.roll(relief, 1, axis=1)) * 9, -28, 26)
texture = rng.normal(0, 3.4, (N, N)) + noise(120) * 7
pixels = np.clip(base + (shade + texture)[:, :, None], 0, 255).astype("uint8")
image = Image.fromarray(pixels)
draw = ImageDraw.Draw(image)
# Pixel-scale tree crowns cast short southeast shadows.
for _ in range(14500):
    xx, zz = rng.integers(4, N - 5, 2)
    if bank[zz, xx] or moist[zz, xx] < 0.3 or height[zz, xx] > 63:
        continue
    green = int(rng.integers(-8, 10))
    draw.rectangle((xx, zz, xx + 3, zz + 3), fill=(46 + green, 69 + green, 56 + green))
    draw.rectangle((xx - 1, zz - 1, xx + 1, zz + 1), fill=(70 + green, 95 + green, 68 + green))
    draw.point((xx - 1, zz - 1), fill=(97 + green, 119 + green, 79 + green))
# Roads, a river crossing, gardens and roof ridges give the crop readable landmarks.
for path in [[(146, 253), (193, 246), (228, 250), (261, 243), (302, 250), (344, 228)],
             [(260, 213), (268, 230), (261, 243), (274, 263), (286, 287)]]:
    draw.line(path, fill=(111, 100, 75), width=4)
    draw.line(path, fill=(186, 167, 125), width=2)
for xx, zz in [(245,226),(261,217),(271,231),(239,241),(251,249),(275,249),(285,256),(265,266),(280,272)]:
    draw.rectangle((xx+1,zz+1,xx+10,zz+7),fill=(72,72,59))
    draw.rectangle((xx,zz,xx+8,zz+5),fill=(139,89,58))
    draw.line((xx,zz,xx+8,zz),fill=(186,132,86))
    draw.line((xx,zz+3,xx+8,zz+3),fill=(117,78,54))
for xx,zz in [(287,229),(301,232),(245,258)]:
    draw.rectangle((xx,zz,xx+10,zz+13),fill=(105,111,66))
    for dx in range(1,10,2):
        draw.line((xx+dx,zz+1,xx+dx,zz+12),fill=(183,161,80))
image.save(OUT / "alder-reach.png", optimize=True)

lava = noise(9) * 28 + noise(33) * 8 + np.sin(x/27 + np.cos(z/54)) * 5
rock = np.zeros_like(base)
rock[:] = [106, 65, 65]
rock[lava < -3] = [66, 56, 60]
rock[(lava > 6) & (lava < 8.6)] = [213, 114, 57]
rock[(lava >= 7.5) & (lava < 8.2)] = [235, 154, 66]
nether = np.clip(rock + (texture + shade * 0.5)[:, :, None],0,255).astype("uint8")
Image.fromarray(nether).save(OUT / "ember-vault.png", optimize=True)
