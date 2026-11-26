# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Generate two original 64x64 Minecraft Java skins and raw RGBA fixtures.

The output is deterministic.  It is written next to this script:

    alder.png / alder.rgba    classic 4-pixel arms, brown adventurer
    ember.png / ember.rgba    slim 3-pixel arms, dark curls + denim jacket

No external data, randomness, network access, or files outside this folder are
used.  Only Pillow is required.
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image


# ---------------------------------------------------------------------------
# small deterministic pixel helpers
# ---------------------------------------------------------------------------


def h01(x: int, y: int, salt: int) -> float:
    """Deterministic [0,1) hash, stable across Python runs."""
    n = (x * 374761393 + y * 668265263 + salt * 69069 + 0x9E3779B9) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    n ^= n >> 16
    return (n & 0xFFFF) / 65536.0


def shade(c: tuple[int, int, int], f: float) -> tuple[int, int, int]:
    return tuple(max(0, min(255, int(v * f + 0.5))) for v in c)


def mix(a: tuple[int, int, int], b: tuple[int, int, int], t: float) -> tuple[int, int, int]:
    return tuple(int(a[i] * (1.0 - t) + b[i] * t + 0.5) for i in range(3))


class Face:
    def __init__(self, x: int, y: int, w: int, h: int, name: str):
        self.x = x
        self.y = y
        self.w = w
        self.h = h
        self.name = name

    def put(self, skin: "Skin", lx: int, ly: int, c: tuple[int, int, int], a: int = 255) -> None:
        if 0 <= lx < self.w and 0 <= ly < self.h:
            skin.put(self.x + lx, self.y + ly, c, a)

    def get(self, skin: "Skin", lx: int, ly: int) -> tuple[int, int, int, int]:
        return skin.get(self.x + lx, self.y + ly)

    def fill(self, skin: "Skin", c: tuple[int, int, int], a: int = 255) -> None:
        for ly in range(self.h):
            for lx in range(self.w):
                self.put(skin, lx, ly, c, a)

    def front_distance(self, lx: int, ly: int) -> int:
        """Distance from the front edge on a side/top/bottom face."""
        if self.name == "nx":
            return self.w - 1 - lx
        if self.name == "px":
            return lx
        if self.name == "top":
            return self.h - 1 - ly
        if self.name == "bottom":
            return ly
        return 0


FACE_SALT = {"nx": 1, "front": 2, "px": 3, "back": 4, "top": 5, "bottom": 6}


def box_faces(u: int, v: int, w: int, h: int, d: int) -> dict[str, Face]:
    """The standard Java skin cube net, using the same names as bust.rs."""
    return {
        "nx": Face(u, v + d, d, h, "nx"),
        "front": Face(u + d, v + d, w, h, "front"),
        "px": Face(u + d + w, v + d, d, h, "px"),
        "back": Face(u + 2 * d + w, v + d, w, h, "back"),
        "top": Face(u + d, v, w, d, "top"),
        "bottom": Face(u + d + w, v, w, d, "bottom"),
    }


class Skin:
    def __init__(self) -> None:
        self.px = [[(0, 0, 0, 0) for _ in range(64)] for _ in range(64)]

    def put(self, x: int, y: int, c: tuple[int, int, int], a: int = 255) -> None:
        if 0 <= x < 64 and 0 <= y < 64:
            self.px[y][x] = (c[0], c[1], c[2], a)

    def get(self, x: int, y: int) -> tuple[int, int, int, int]:
        return self.px[y][x]

    def blend(self, x: int, y: int, c: tuple[int, int, int], a: int) -> None:
        if not (0 <= x < 64 and 0 <= y < 64):
            return
        dr, dg, db, da = self.px[y][x]
        sa = a / 255.0
        da = da / 255.0
        oa = sa + da * (1.0 - sa)
        if oa <= 1e-6:
            self.px[y][x] = (0, 0, 0, 0)
            return
        r = int((c[0] * sa + dr * da * (1.0 - sa)) / oa + 0.5)
        g = int((c[1] * sa + dg * da * (1.0 - sa)) / oa + 0.5)
        b = int((c[2] * sa + db * da * (1.0 - sa)) / oa + 0.5)
        self.px[y][x] = (r, g, b, int(oa * 255 + 0.5))

    def image(self) -> Image.Image:
        img = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
        p = img.load()
        for y in range(64):
            for x in range(64):
                p[x, y] = self.px[y][x]
        return img


def field(
    face: Face,
    skin: Skin,
    pal: tuple[tuple[int, int, int], tuple[int, int, int], tuple[int, int, int]],
    salt: int,
    light: float = 0.17,
    dark: float = 0.17,
    a: int = 255,
) -> None:
    base, lt, dk = pal
    for ly in range(face.h):
        for lx in range(face.w):
            n = h01(face.x + lx, face.y + ly, salt)
            c = base
            if n < light:
                c = lt
            elif n > 1.0 - dark:
                c = dk
            face.put(skin, lx, ly, c, a)


def rect(face: Face, skin: Skin, x0: int, y0: int, x1: int, y1: int, c, a=255) -> None:
    for y in range(y0, y1 + 1):
        for x in range(x0, x1 + 1):
            face.put(skin, x, y, c, a)


def line(face: Face, skin: Skin, x0: int, y0: int, x1: int, y1: int, c, a=255) -> None:
    dx = abs(x1 - x0)
    dy = -abs(y1 - y0)
    sx = 1 if x0 < x1 else -1
    sy = 1 if y0 < y1 else -1
    err = dx + dy
    x, y = x0, y0
    while True:
        face.put(skin, x, y, c, a)
        if x == x1 and y == y1:
            break
        e2 = 2 * err
        if e2 >= dy:
            err += dy
            x += sx
        if e2 <= dx:
            err += dx
            y += sy


def hair_field(face: Face, skin: Skin, salt: int, a: int = 255) -> None:
    """Dark, slightly curly hair with small deterministic highlight arcs."""
    hair = (32, 23, 23)
    dark = (20, 14, 15)
    light = (58, 42, 38)
    high = (84, 60, 52)
    for ly in range(face.h):
        for lx in range(face.w):
            n = h01(lx // 2, ly // 2, salt)
            m = h01(lx, ly, salt + 5)
            c = hair
            if n < 0.26:
                c = dark
            elif n > 0.71:
                c = light
            if m > 0.86:
                c = high
            face.put(skin, lx, ly, c, a)

    # A few deliberate curl highlights / dark separations.
    curls = [
        (1, 1, 2, 1),
        (5, 1, 2, 1),
        (2, 4, 2, 1),
        (6, 4, 1, 2),
        (3, 6, 2, 1),
    ]
    for cx, cy, cw, ch in curls:
        for y in range(cy, min(face.h, cy + ch)):
            for x in range(cx, min(face.w, cx + cw)):
                if h01(x, y, salt + 11) > 0.22:
                    face.put(skin, x, y, high if y == cy else light)
        if cy + ch < face.h:
            for x in range(cx, min(face.w, cx + cw)):
                face.put(skin, x, cy + ch, dark)


# ---------------------------------------------------------------------------
# Alder: brown adventurer, classic arms
# ---------------------------------------------------------------------------

A_SKIN = (201, 141, 100)
A_SKIN_L = (222, 167, 124)
A_SKIN_D = (166, 109, 74)
A_SKIN_SH = (140, 88, 60)
A_CHEEK = (190, 112, 83)
A_HAIR = (58, 37, 24)
A_HAIR_L = (82, 54, 35)
A_HAIR_D = (38, 23, 15)
A_BEARD = (74, 48, 31)
A_EYE_W = (238, 232, 215)
A_IRIS = (47, 92, 70)
A_BROW = (60, 38, 23)
A_MOUTH = (116, 65, 52)
A_CAP = (91, 60, 37)
A_CAP_L = (124, 86, 53)
A_CAP_D = (59, 37, 21)
A_BRASS = (198, 154, 74)
A_BRASS_L = (231, 190, 104)
A_BRASS_D = (126, 89, 38)
A_SCARF = (166, 66, 45)
A_SCARF_L = (199, 96, 62)
A_SCARF_D = (111, 39, 28)
A_COAT = (104, 66, 41)
A_COAT_L = (133, 90, 55)
A_COAT_D = (67, 40, 24)
A_SHIRT = (214, 194, 154)
A_SHIRT_L = (235, 217, 180)
A_SHIRT_D = (177, 154, 117)
A_BELT = (56, 37, 24)
A_BELT_L = (82, 57, 35)
A_GLOVE = (72, 47, 31)
A_GLOVE_L = (98, 66, 43)
A_TROUSER = (65, 46, 34)
A_TROUSER_L = (86, 63, 46)
A_BOOT = (48, 31, 21)
A_BOOT_L = (70, 47, 30)


def alder_head(skin: Skin) -> None:
    h = box_faces(0, 0, 8, 8, 8)

    # Base face.
    f = h["front"]
    field(f, skin, (A_SKIN, A_SKIN_L, A_SKIN_D), 101)
    for x in range(8):
        f.put(skin, x, 0, A_HAIR if h01(x, 0, 7) > 0.2 else A_HAIR_L)
    for x in (0, 1, 2, 5, 6, 7):
        f.put(skin, x, 1, A_HAIR if h01(x, 1, 8) < 0.75 else A_HAIR_L)
    f.put(skin, 3, 1, A_HAIR_D)
    f.put(skin, 4, 1, A_HAIR_D)

    for x in (0, 7):
        for y in range(2, 5):
            f.put(skin, x, y, A_HAIR if y < 4 else A_BEARD)
    for x in (1, 2):
        f.put(skin, x, 3, A_BROW)
    for x in (5, 6):
        f.put(skin, x, 3, A_BROW)

    f.put(skin, 1, 4, A_EYE_W)
    f.put(skin, 2, 4, A_IRIS)
    f.put(skin, 5, 4, A_IRIS)
    f.put(skin, 6, 4, A_EYE_W)
    f.put(skin, 2, 5, shade(A_IRIS, 0.75))
    f.put(skin, 5, 5, shade(A_IRIS, 0.75))

    f.put(skin, 4, 4, A_SKIN_D)
    f.put(skin, 4, 5, A_SKIN_D)
    f.put(skin, 3, 5, A_SKIN_L)
    f.put(skin, 0, 5, A_CHEEK)
    f.put(skin, 7, 5, A_CHEEK)

    f.put(skin, 3, 6, A_MOUTH)
    f.put(skin, 4, 6, mix(A_MOUTH, A_SKIN, 0.35))
    f.put(skin, 5, 6, A_MOUTH)
    f.put(skin, 4, 7, A_SKIN_L)

    # Weathered cheek and a short beard along the jaw.
    f.put(skin, 6, 5, A_SKIN_L)
    for x in range(1, 7):
        if h01(x, 7, 13) > 0.45:
            f.put(skin, x, 7, mix(A_BEARD, A_SKIN, 0.35))

    # Sides: skin, ears, hair, sideburns.
    for name in ("nx", "px"):
        s = h[name]
        field(s, skin, (A_SKIN, A_SKIN_L, A_SKIN_D), 110 if name == "nx" else 111)
        for ly in range(8):
            for lx in range(8):
                fd = s.front_distance(lx, ly)
                if ly <= 2 or fd >= 5 or (ly <= 4 and fd >= 4):
                    n = h01(s.x + lx, s.y + ly, 17)
                    s.put(skin, lx, ly, A_HAIR if n < 0.7 else (A_HAIR_L if n < 0.9 else A_HAIR_D))
                if 4 <= ly <= 5 and 2 <= fd <= 3:
                    s.put(skin, lx, ly, A_SKIN_D)
                    s.put(skin, lx, ly - 1, A_SKIN_L)
        # Ear canal and a little hair in front of it.
        for ly in range(4, 6):
            fd_ear = 2
            lx = s.w - 1 - fd_ear if name == "nx" else fd_ear
            s.put(skin, lx, ly, A_SKIN_SH)
        for ly in range(3, 6):
            fd = 0
            lx = s.w - 1 - fd if name == "nx" else fd
            s.put(skin, lx, ly, A_BEARD if ly >= 4 else A_HAIR_D)

    # Back: hair with a small nape of neck.
    b = h["back"]
    field(b, skin, (A_HAIR, A_HAIR_L, A_HAIR_D), 130, light=0.18, dark=0.22)
    for x in range(8):
        for y in range(8):
            n = h01(x, y, 131)
            if n > 0.78:
                b.put(skin, x, y, A_HAIR_L)
    b.put(skin, 3, 7, A_SKIN_D)
    b.put(skin, 4, 7, A_SKIN_D)

    # Top and bottom base.
    field(h["top"], skin, (A_HAIR, A_HAIR_L, A_HAIR_D), 140, light=0.15, dark=0.25)
    bottom = h["bottom"]
    field(bottom, skin, (A_SKIN_D, A_SKIN, A_SKIN_SH), 150)
    for x in range(8):
        for y in range(4, 8):
            if h01(x, y, 151) > 0.35:
                bottom.put(skin, x, y, A_HAIR_D)

    # Outer cap.
    c = box_faces(32, 0, 8, 8, 8)
    cf = c["front"]
    for y in range(4):
        for x in range(8):
            n = h01(x, y, 201)
            col = A_CAP if n < 0.62 else (A_CAP_L if n < 0.82 else A_CAP_D)
            if y == 3:
                col = A_CAP_D if n < 0.5 else A_CAP
            cf.put(skin, x, y, col)
    for x in (1, 3, 5, 7):
        cf.put(skin, x, 3, A_CAP_D)
    for x in (2, 4, 6):
        cf.put(skin, x, 2, A_CAP_L)
    cf.put(skin, 4, 0, A_CAP_D)
    cf.put(skin, 4, 1, A_CAP_D)
    cf.put(skin, 6, 2, A_BRASS_D)
    cf.put(skin, 6, 1, A_BRASS)

    for name in ("nx", "px"):
        s = c[name]
        for ly in range(4):
            for lx in range(8):
                fd = s.front_distance(lx, ly)
                if ly <= 2 or (ly == 3 and fd <= 3):
                    n = h01(s.x + lx, s.y + ly, 210)
                    col = A_CAP if n < 0.6 else (A_CAP_L if n < 0.8 else A_CAP_D)
                    if ly == 3:
                        col = A_CAP_D
                    s.put(skin, lx, ly, col)
        # Band and a little brass buckle on the right side.
        for lx in range(8):
            s.put(skin, lx, 2, A_CAP_D if h01(lx, 0, 211) < 0.65 else A_CAP)
        if name == "nx":
            s.put(skin, 5, 2, A_BRASS)
            s.put(skin, 6, 2, A_BRASS_L)
            s.put(skin, 5, 3, A_BRASS_D)

    cb = c["back"]
    for y in range(3):
        for x in range(8):
            n = h01(x, y, 220)
            cb.put(skin, x, y, A_CAP if n < 0.62 else (A_CAP_L if n < 0.82 else A_CAP_D))
    for x in range(8):
        cb.put(skin, x, 2, A_CAP_D)
    cb.put(skin, 4, 0, A_CAP_D)
    cb.put(skin, 4, 1, A_CAP_D)
    cb.put(skin, 3, 2, A_BRASS_D)
    cb.put(skin, 4, 2, A_BRASS)

    ct = c["top"]
    for y in range(8):
        for x in range(8):
            n = h01(x, y, 230)
            col = A_CAP if n < 0.5 else (A_CAP_L if n < 0.74 else A_CAP_D)
            # Round the crown a little by leaving the extreme corners clear.
            if (x in (0, 7)) and (y in (0, 7)):
                col = A_CAP_D
            ct.put(skin, x, y, col)
    for i in range(8):
        ct.put(skin, 4, i, A_CAP_D if i % 2 == 0 else A_CAP)
        ct.put(skin, i, 4, A_CAP_D if i % 2 == 1 else A_CAP)
    ct.put(skin, 4, 4, A_BRASS)
    ct.put(skin, 4, 3, A_BRASS_L)
    ct.put(skin, 4, 5, A_BRASS_D)


def alder_body(skin: Skin) -> None:
    b = box_faces(16, 16, 8, 12, 4)
    f = b["front"]
    # Shirt centre with an open coat.
    for y in range(12):
        for x in range(8):
            if 2 <= x <= 5 and y <= 8:
                n = h01(x, y, 301)
                col = A_SHIRT if n < 0.72 else (A_SHIRT_L if n < 0.9 else A_SHIRT_D)
                f.put(skin, x, y, col)
            else:
                n = h01(x, y, 302)
                col = A_COAT if n < 0.64 else (A_COAT_L if n < 0.82 else A_COAT_D)
                f.put(skin, x, y, col)
    # Collar and shirt folds.
    for x in (2, 5):
        f.put(skin, x, 0, A_SHIRT_L)
    f.put(skin, 3, 0, A_SHIRT_D)
    f.put(skin, 4, 0, A_SHIRT_D)
    for y in range(2, 8):
        if h01(3, y, 303) > 0.55:
            f.put(skin, 3, y, A_SHIRT_D)
        if h01(4, y, 304) > 0.65:
            f.put(skin, 4, y, A_SHIRT_L)
    # Coat opening edges.
    for y in range(1, 9):
        f.put(skin, 1, y, A_COAT_D if y % 3 else A_COAT_L)
        f.put(skin, 6, y, A_COAT_D if y % 3 else A_COAT_L)
    # Belt and brass buckle.
    for x in range(8):
        f.put(skin, x, 9, A_BELT if h01(x, 9, 305) < 0.75 else A_BELT_L)
    f.put(skin, 3, 9, A_BRASS)
    f.put(skin, 4, 9, A_BRASS_L)
    f.put(skin, 3, 10, A_BRASS_D)
    for y in range(10, 12):
        for x in range(8):
            n = h01(x, y, 306)
            f.put(skin, x, y, A_COAT if n < 0.7 else A_COAT_D)
    # Pocket flaps.
    rect(f, skin, 0, 7, 1, 7, A_COAT_L)
    rect(f, skin, 6, 7, 7, 7, A_COAT_L)
    f.put(skin, 0, 8, A_COAT_D)
    f.put(skin, 7, 8, A_COAT_D)

    # Back and sides of the coat.
    back = b["back"]
    field(back, skin, (A_COAT, A_COAT_L, A_COAT_D), 310)
    for y in range(1, 12):
        back.put(skin, 4, y, A_COAT_D if y % 4 else A_COAT_L)
    for x in range(8):
        back.put(skin, x, 2, A_COAT_L if x % 2 else A_COAT_D)
        back.put(skin, x, 9, A_BELT if h01(x, 9, 311) < 0.8 else A_BELT_L)
    back.put(skin, 3, 4, A_BRASS)
    back.put(skin, 4, 4, A_BRASS_D)
    for y in range(10, 12):
        for x in range(8):
            if h01(x, y, 312) > 0.7:
                back.put(skin, x, y, A_COAT_D)

    for name in ("nx", "px"):
        s = b[name]
        field(s, skin, (A_COAT, A_COAT_L, A_COAT_D), 320 if name == "nx" else 321)
        for y in range(12):
            s.put(skin, 0, y, A_COAT_D if y % 3 else A_COAT_L)
            s.put(skin, 3, y, A_COAT_D if y % 4 else A_COAT_L)
        rect(s, skin, 1, 6, 2, 7, A_COAT_D)
        s.put(skin, 1, 6, A_COAT_L)
        s.put(skin, 2, 7, A_BRASS_D)
        for x in range(4):
            s.put(skin, x, 9, A_BELT if h01(x, 9, 322) < 0.8 else A_BELT_L)

    top = b["top"]
    for y in range(4):
        for x in range(8):
            if 2 <= x <= 5:
                top.put(skin, x, y, A_SHIRT if h01(x, y, 330) < 0.8 else A_SHIRT_D)
            else:
                top.put(skin, x, y, A_COAT if h01(x, y, 331) < 0.75 else A_COAT_L)
    bottom = b["bottom"]
    field(bottom, skin, (A_COAT_D, A_COAT, A_BELT), 340)

    # Outer layer: scarf, lapels, pockets and hem.
    o = box_faces(16, 32, 8, 12, 4)
    of = o["front"]
    # Scarf wrapped around the neck.
    for y in (0, 1):
        for x in range(8):
            n = h01(x, y, 401)
            col = A_SCARF if n < 0.56 else (A_SCARF_L if n < 0.8 else A_SCARF_D)
            of.put(skin, x, y, col)
    # Scarf tail down the character's right chest.
    for y in range(2, 7):
        for x in range(1, 3):
            n = h01(x, y, 402)
            col = A_SCARF if n < 0.52 else (A_SCARF_L if n < 0.76 else A_SCARF_D)
            of.put(skin, x, y, col)
    for x in (1, 2):
        of.put(skin, x, 7, A_SCARF_D)
    for x in (1, 2):
        of.put(skin, x, 8, A_SCARF_D if h01(x, 8, 403) < 0.5 else (0, 0, 0), 0)
    # Lapels along the open front.
    for y in range(1, 9):
        of.put(skin, 0, y, A_COAT_D if y % 3 else A_COAT_L)
        of.put(skin, 7, y, A_COAT_D if y % 4 else A_COAT_L)
    for y in range(2, 8):
        of.put(skin, 1, y, A_COAT_L if y % 2 else A_COAT_D)
        of.put(skin, 6, y, A_COAT_L if y % 3 else A_COAT_D)
    # Pocket flaps on the lower coat.
    rect(of, skin, 2, 7, 3, 8, A_COAT_L)
    rect(of, skin, 4, 7, 5, 8, A_COAT_L)
    of.put(skin, 2, 8, A_COAT_D)
    of.put(skin, 5, 8, A_COAT_D)
    # Hem.
    for x in (0, 1, 6, 7):
        for y in (10, 11):
            of.put(skin, x, y, A_COAT_D if h01(x, y, 404) < 0.7 else A_COAT_L)
    # Brass clasp at the throat.
    of.put(skin, 4, 1, A_BRASS)
    of.put(skin, 4, 2, A_BRASS_D)

    # Scarf across the back and coat details.
    ob = o["back"]
    for y in (0, 1):
        for x in range(8):
            n = h01(x, y, 410)
            ob.put(skin, x, y, A_SCARF if n < 0.55 else (A_SCARF_L if n < 0.8 else A_SCARF_D))
    for y in range(2, 12):
        for x in range(8):
            n = h01(x, y, 411)
            col = A_COAT if n < 0.6 else (A_COAT_L if n < 0.8 else A_COAT_D)
            ob.put(skin, x, y, col)
    for y in range(2, 12):
        ob.put(skin, 4, y, A_COAT_D if y % 3 else A_COAT_L)
    for x in range(8):
        ob.put(skin, x, 3, A_COAT_L if x % 2 else A_COAT_D)
    # A small satchel pouch on the lower back.
    rect(ob, skin, 1, 7, 3, 9, A_BELT)
    rect(ob, skin, 1, 7, 3, 7, A_BELT_L)
    ob.put(skin, 2, 8, A_BRASS)
    ob.put(skin, 2, 9, A_BRASS_D)
    for x in (0, 1):
        ob.put(skin, x, 10, A_COAT_D)
        ob.put(skin, x, 11, A_COAT_D)

    for name in ("nx", "px"):
        s = o[name]
        # Scarf wraps around the neck.
        for x in range(4):
            for y in (0, 1):
                n = h01(s.x + x, s.y + y, 420)
                s.put(skin, x, y, A_SCARF if n < 0.55 else (A_SCARF_L if n < 0.8 else A_SCARF_D))
        # A stitched side seam and a pocket.
        for y in range(2, 10):
            s.put(skin, 0, y, A_COAT_D if y % 3 else A_COAT_L)
        rect(s, skin, 1, 5, 2, 7, A_COAT_D)
        s.put(skin, 1, 5, A_COAT_L)
        s.put(skin, 2, 7, A_BRASS_D)
        for x in range(4):
            s.put(skin, x, 9, A_BELT if h01(x, 9, 421) < 0.75 else A_BELT_L)
        if name == "nx":
            # Hip pouch.
            rect(s, skin, 1, 6, 2, 8, A_BELT)
            s.put(skin, 1, 6, A_BELT_L)
            s.put(skin, 2, 7, A_BRASS_D)

    ot = o["top"]
    for y in range(4):
        for x in range(8):
            if 1 <= x <= 6 and y <= 2:
                n = h01(x, y, 430)
                ot.put(skin, x, y, A_SCARF if n < 0.55 else (A_SCARF_L if n < 0.82 else A_SCARF_D))
            else:
                ot.put(skin, x, y, A_COAT if h01(x, y, 431) < 0.75 else A_COAT_L)
    field(o["bottom"], skin, (A_COAT_D, A_COAT, A_BELT), 440)


def alder_arms(skin: Skin) -> None:
    for side, arm, outer, salt in (
        ("right", box_faces(40, 16, 4, 12, 4), box_faces(40, 32, 4, 12, 4), 500),
        ("left", box_faces(32, 48, 4, 12, 4), box_faces(48, 48, 4, 12, 4), 540),
    ):
        # Base sleeve, cuff and fingerless glove.
        for name in ("front", "nx", "px", "back"):
            f = arm[name]
            field(f, skin, (A_COAT, A_COAT_L, A_COAT_D), salt + FACE_SALT[name])
            for y in range(0, 9):
                # Cloth folds.
                if h01(f.x, f.y + y, salt + 1) > 0.82:
                    f.put(skin, 0, y, A_COAT_L)
            for y in (9,):
                f.fill(skin, A_COAT_L)
            for y in (10, 11):
                for x in range(f.w):
                    n = h01(f.x + x, f.y + y, salt + 2)
                    f.put(skin, x, y, A_GLOVE if n < 0.7 else A_GLOVE_L)
            # Rolled cuff.
            f.put(skin, 0, 9, A_COAT_D)
            f.put(skin, f.w - 1, 9, A_COAT_D)
            f.put(skin, 1, 9, A_BRASS_D)
            # Skin fingertips on the bottom row.
            for x in (1, 2):
                f.put(skin, x, 11, mix(A_SKIN, A_GLOVE, 0.35))
        if side == "left":
            # Elbow patch.
            for name in ("front", "nx", "px", "back"):
                f = arm[name]
                rect(f, skin, 0, 5, f.w - 1, 6, A_COAT_D)
                f.put(skin, 1, 5, A_COAT_L)
        top = arm["top"]
        field(top, skin, (A_COAT, A_COAT_L, A_COAT_D), salt + 3)
        bottom = arm["bottom"]
        field(bottom, skin, (A_GLOVE, A_GLOVE_L, A_BELT), salt + 4)

        # Outer sleeve, stopping above the hand.
        for name in ("front", "nx", "px", "back"):
            f = outer[name]
            field(f, skin, (A_COAT, A_COAT_L, A_COAT_D), salt + 30 + FACE_SALT[name])
            for y in range(0, 9):
                if h01(f.x, f.y + y, salt + 31) > 0.84:
                    f.put(skin, 0, y, A_COAT_L)
            # Shoulder seam and cuff.
            for x in range(f.w):
                f.put(skin, x, 0, A_COAT_D if x % 2 else A_COAT_L)
                f.put(skin, x, 8, A_COAT_L if x % 2 else A_COAT_D)
            if side == "right":
                f.put(skin, 1, 8, A_BRASS_D)
            else:
                rect(f, skin, 0, 5, f.w - 1, 6, A_COAT_D)
                f.put(skin, 1, 5, A_COAT_L)
            # The hand rows stay transparent so the base hand shows.
        field(outer["top"], skin, (A_COAT, A_COAT_L, A_COAT_D), salt + 33)
        # Outer bottom remains transparent.


def alder_legs(skin: Skin) -> None:
    for leg, outer, salt in (
        (box_faces(0, 16, 4, 12, 4), box_faces(0, 32, 4, 12, 4), 600),
        (box_faces(16, 48, 4, 12, 4), box_faces(0, 48, 4, 12, 4), 640),
    ):
        for name in ("front", "nx", "px", "back"):
            f = leg[name]
            for y in range(12):
                for x in range(f.w):
                    n = h01(f.x + x, f.y + y, salt)
                    if y < 8:
                        c = A_TROUSER if n < 0.68 else (A_TROUSER_L if n < 0.88 else (49, 34, 25))
                    else:
                        c = A_BOOT if n < 0.6 else (A_BOOT_L if n < 0.85 else (34, 21, 14))
                    f.put(skin, x, y, c)
            # Knee patch.
            rect(f, skin, 0, 5, f.w - 1, 6, mix(A_TROUSER, A_TROUSER_L, 0.35))
            f.put(skin, 1, 5, A_TROUSER_L)
            # Boot laces and sole.
            for x in range(f.w):
                if x % 2 == 0:
                    f.put(skin, x, 9, A_BOOT_L)
                f.put(skin, x, 11, (32, 20, 14))
        field(leg["top"], skin, (A_TROUSER, A_TROUSER_L, (49, 34, 25)), salt + 2)
        field(leg["bottom"], skin, ((32, 20, 14), A_BOOT, A_BOOT_L), salt + 3)

        # Long coat tails over the top of each leg, plus boot cuffs.
        for name in ("front", "nx", "px", "back"):
            f = outer[name]
            for y in range(3):
                for x in range(f.w):
                    n = h01(f.x + x, f.y + y, salt + 20)
                    f.put(skin, x, y, A_COAT if n < 0.65 else (A_COAT_L if n < 0.85 else A_COAT_D))
            for x in range(f.w):
                f.put(skin, x, 2, A_COAT_D)
                f.put(skin, x, 8, A_BOOT_L if x % 2 else A_BOOT)
            f.put(skin, 1, 8, A_BRASS_D)
        field(outer["top"], skin, (A_COAT, A_COAT_L, A_COAT_D), salt + 22)


def make_alder() -> Skin:
    skin = Skin()
    alder_head(skin)
    alder_body(skin)
    alder_arms(skin)
    alder_legs(skin)
    return skin


# ---------------------------------------------------------------------------
# Ember: dark curls, copper goggles, denim jacket, slim arms
# ---------------------------------------------------------------------------

E_SKIN = (138, 86, 56)
E_SKIN_L = (166, 110, 75)
E_SKIN_D = (108, 64, 40)
E_SKIN_SH = (86, 48, 30)
E_CHEEK = (152, 83, 55)
E_HAIR = (32, 23, 23)
E_HAIR_L = (58, 42, 38)
E_HAIR_H = (84, 60, 52)
E_HAIR_D = (19, 13, 14)
E_EYE_W = (238, 225, 205)
E_IRIS = (176, 112, 56)
E_IRIS_D = (103, 59, 29)
E_BROW = (35, 24, 22)
E_MOUTH = (86, 43, 38)
E_LIP = (121, 66, 54)
E_COPPER = (196, 115, 54)
E_COPPER_L = (231, 158, 88)
E_COPPER_D = (127, 66, 30)
E_GLASS = (42, 74, 74)
E_GLASS_L = (96, 138, 132)
E_STRAP = (57, 39, 32)
E_DENIM = (45, 72, 104)
E_DENIM_L = (67, 102, 140)
E_DENIM_D = (28, 47, 71)
E_STITCH = (154, 178, 199)
E_TEE = (44, 41, 40)
E_TEE_L = (68, 63, 60)
E_JEANS = (37, 37, 42)
E_JEANS_L = (55, 55, 62)
E_BOOT = (35, 31, 31)
E_BOOT_L = (57, 51, 50)


def ember_head(skin: Skin) -> None:
    h = box_faces(0, 0, 8, 8, 8)
    f = h["front"]
    field(f, skin, (E_SKIN, E_SKIN_L, E_SKIN_D), 701)

    # Hairline and temple curls under the outer curls.
    for x in range(8):
        f.put(skin, x, 0, E_HAIR if h01(x, 0, 702) < 0.7 else E_HAIR_L)
    for x in (0, 1, 6, 7):
        f.put(skin, x, 1, E_HAIR if h01(x, 1, 703) < 0.7 else E_HAIR_L)
    for x in (0, 7):
        for y in range(2, 6):
            f.put(skin, x, y, E_HAIR_D if h01(x, y, 704) < 0.6 else E_HAIR)

    # Brows, amber eyes, nose and mouth.
    for x in (1, 2):
        f.put(skin, x, 3, E_BROW)
    for x in (5, 6):
        f.put(skin, x, 3, E_BROW)
    f.put(skin, 1, 4, E_EYE_W)
    f.put(skin, 2, 4, E_IRIS)
    f.put(skin, 5, 4, E_IRIS)
    f.put(skin, 6, 4, E_EYE_W)
    f.put(skin, 2, 3, E_IRIS_D)
    f.put(skin, 5, 3, E_IRIS_D)
    f.put(skin, 3, 4, E_SKIN_L)
    f.put(skin, 4, 4, E_SKIN_D)
    f.put(skin, 4, 5, E_SKIN_D)
    f.put(skin, 3, 5, E_SKIN_L)
    f.put(skin, 0, 5, E_CHEEK)
    f.put(skin, 7, 5, E_CHEEK)
    f.put(skin, 1, 5, mix(E_SKIN_L, (220, 180, 150), 0.25))
    f.put(skin, 6, 5, mix(E_SKIN_L, (220, 180, 150), 0.25))
    f.put(skin, 3, 6, E_MOUTH)
    f.put(skin, 4, 6, E_LIP)
    f.put(skin, 5, 6, E_MOUTH)
    f.put(skin, 4, 7, E_SKIN_L)

    # Sides with ears and a copper earring.
    for name in ("nx", "px"):
        s = h[name]
        field(s, skin, (E_SKIN, E_SKIN_L, E_SKIN_D), 710 if name == "nx" else 711)
        for ly in range(8):
            for lx in range(8):
                fd = s.front_distance(lx, ly)
                if ly <= 2 or fd >= 5 or (ly <= 4 and fd >= 4):
                    n = h01(s.x + lx, s.y + ly, 712)
                    s.put(skin, lx, ly, E_HAIR if n < 0.55 else (E_HAIR_L if n < 0.82 else E_HAIR_D))
                if 4 <= ly <= 5 and 2 <= fd <= 3:
                    s.put(skin, lx, ly, E_SKIN_D)
                    s.put(skin, lx, ly - 1, E_SKIN_L)
        # Earring on the visible ear.
        fd_ear = 2
        lx = s.w - 1 - fd_ear if name == "nx" else fd_ear
        s.put(skin, lx, 6, E_COPPER)
        s.put(skin, lx, 7, E_COPPER_D)
        for ly in range(3, 6):
            lx = s.w - 1 if name == "nx" else 0
            s.put(skin, lx, ly, E_HAIR_D if ly < 4 else E_HAIR)

    b = h["back"]
    field(b, skin, (E_HAIR, E_HAIR_L, E_HAIR_D), 730, light=0.16, dark=0.24)
    for x in range(8):
        for y in range(8):
            if h01(x, y, 731) > 0.84:
                b.put(skin, x, y, E_HAIR_H)
    b.put(skin, 3, 7, E_SKIN_D)
    b.put(skin, 4, 7, E_SKIN_D)

    field(h["top"], skin, (E_HAIR, E_HAIR_L, E_HAIR_D), 740, light=0.14, dark=0.26)
    bt = h["bottom"]
    field(bt, skin, (E_SKIN_D, E_SKIN, E_SKIN_SH), 750)
    for x in range(8):
        for y in range(4, 8):
            if h01(x, y, 751) > 0.4:
                bt.put(skin, x, y, E_HAIR_D)

    # Outer layer: curls and copper goggles.
    c = box_faces(32, 0, 8, 8, 8)
    cf = c["front"]
    # Curls across the crown.
    for x in range(8):
        n = h01(x, 0, 801)
        cf.put(skin, x, 0, E_HAIR if n < 0.55 else (E_HAIR_L if n < 0.8 else E_HAIR_H))
    # Goggles: two 2-pixel glass lenses, copper rims, leather strap.
    for x in range(8):
        cf.put(skin, x, 1, E_HAIR if x in (0, 7) else E_COPPER)
        cf.put(skin, x, 2, E_HAIR if x in (0, 7) else E_COPPER)
    for x in (1, 3, 5, 7):
        cf.put(skin, x, 2, E_COPPER_L)
    cf.put(skin, 4, 2, E_COPPER_D)
    for x in (2, 3, 5, 6):
        cf.put(skin, x, 2, E_GLASS if x in (2, 5) else E_GLASS_L)
    for x in (1, 2, 3, 4, 5, 6, 7):
        cf.put(skin, x, 3, E_COPPER if x % 2 else E_COPPER_D)
    # Temples.
    for y in range(1, 6):
        for x in (0, 7):
            n = h01(x, y, 802)
            if y <= 4 or n > 0.4:
                cf.put(skin, x, y, E_HAIR if n < 0.6 else (E_HAIR_L if n < 0.85 else E_HAIR_D))
    cf.put(skin, 0, 4, E_HAIR_L)
    cf.put(skin, 7, 4, E_HAIR_H)
    cf.put(skin, 4, 1, E_COPPER_L)
    cf.put(skin, 1, 1, E_COPPER_L)

    for name in ("nx", "px"):
        s = c[name]
        for ly in range(8):
            for lx in range(8):
                fd = s.front_distance(lx, ly)
                painted = ly <= 2 or fd >= 4 or (ly <= 4 and fd >= 2)
                if painted:
                    n = h01(s.x + lx, s.y + ly, 810)
                    s.put(skin, lx, ly, E_HAIR if n < 0.55 else (E_HAIR_L if n < 0.8 else E_HAIR_D))
        # Copper strap around the head.
        for lx in range(8):
            s.put(skin, lx, 1, E_STRAP if h01(lx, 1, 811) < 0.7 else E_STRAP)
            s.put(skin, lx, 2, E_COPPER if h01(lx, 2, 812) < 0.82 else E_COPPER_L)
        # Buckle on the right side.
        if name == "nx":
            s.put(skin, 5, 2, E_COPPER_L)
            s.put(skin, 6, 2, E_COPPER_D)
            s.put(skin, 6, 1, E_COPPER)
        # Loose curls at the back.
        for ly in range(4, 7):
            for lx in range(4, 8):
                if h01(s.x + lx, s.y + ly, 813) > 0.42:
                    s.put(skin, lx, ly, E_HAIR if h01(lx, ly, 814) < 0.6 else E_HAIR_L)

    cb = c["back"]
    for y in range(8):
        for x in range(8):
            n = h01(x, y, 820)
            cb.put(skin, x, y, E_HAIR if n < 0.55 else (E_HAIR_L if n < 0.8 else E_HAIR_H))
    for x in range(8):
        cb.put(skin, x, 1, E_STRAP)
        cb.put(skin, x, 2, E_COPPER if h01(x, 2, 821) < 0.8 else E_COPPER_D)
    cb.put(skin, 3, 2, E_COPPER_L)
    cb.put(skin, 4, 2, E_COPPER_L)
    # Curl separations.
    for x in (1, 4, 6):
        for y in range(4, 7):
            if h01(x, y, 822) > 0.35:
                cb.put(skin, x, y, E_HAIR_D)
    for x in range(8):
        cb.put(skin, x, 7, E_HAIR_D if h01(x, 7, 823) < 0.6 else E_HAIR)

    ct = c["top"]
    hair_field_for_outer(ct, skin, 830)
    # The outermost top corners stay transparent to soften the silhouette.
    for x, y in ((0, 0), (7, 0), (0, 7), (7, 7)):
        ct.put(skin, x, y, (0, 0, 0), 0)


def hair_field_for_outer(face: Face, skin: Skin, salt: int) -> None:
    for y in range(face.h):
        for x in range(face.w):
            n = h01(x // 2, y // 2, salt)
            c = E_HAIR
            if n < 0.24:
                c = E_HAIR_D
            elif n > 0.68:
                c = E_HAIR_L
            if h01(x, y, salt + 4) > 0.88:
                c = E_HAIR_H
            face.put(skin, x, y, c)
    for cx, cy, cw, ch in ((1, 1, 2, 1), (5, 1, 2, 1), (2, 3, 2, 1), (5, 5, 2, 1), (1, 5, 2, 1)):
        for y in range(cy, min(face.h, cy + ch)):
            for x in range(cx, min(face.w, cx + cw)):
                face.put(skin, x, y, E_HAIR_H if y == cy else E_HAIR_L)
        if cy + ch < face.h:
            for x in range(cx, min(face.w, cx + cw)):
                face.put(skin, x, cy + ch, E_HAIR_D)


def ember_body(skin: Skin) -> None:
    b = box_faces(16, 16, 8, 12, 4)
    f = b["front"]
    # Charcoal tee.
    for y in range(12):
        for x in range(8):
            if y < 9:
                n = h01(x, y, 901)
                f.put(skin, x, y, E_TEE if n < 0.72 else (E_TEE_L if n < 0.9 else (34, 31, 30)))
            else:
                n = h01(x, y, 902)
                f.put(skin, x, y, E_JEANS if n < 0.75 else (E_JEANS_L if n < 0.92 else (26, 26, 30)))
    # Neckline and a copper chain/pendant.
    for x in (2, 3, 4, 5):
        f.put(skin, x, 0, mix(E_TEE, E_SKIN, 0.2))
    f.put(skin, 3, 1, E_COPPER_D)
    f.put(skin, 4, 1, E_COPPER)
    f.put(skin, 4, 2, E_COPPER_L)
    # Small ember mark on the tee.
    f.put(skin, 4, 4, E_COPPER)
    f.put(skin, 3, 5, E_COPPER_D)
    f.put(skin, 4, 5, E_COPPER_L)
    f.put(skin, 5, 5, E_COPPER_D)
    f.put(skin, 4, 6, (205, 90, 45))
    # Waistband.
    for x in range(8):
        f.put(skin, x, 9, E_JEANS_L if x % 2 else (28, 28, 32))
    f.put(skin, 3, 9, E_COPPER)
    f.put(skin, 4, 9, E_COPPER_L)

    for name in ("back",):
        back = b[name]
        for y in range(12):
            for x in range(8):
                n = h01(x, y, 910)
                c = E_TEE if y < 9 and n < 0.72 else (E_TEE_L if y < 9 else (E_JEANS if n < 0.7 else E_JEANS_L))
                back.put(skin, x, y, c)
        for x in range(8):
            back.put(skin, x, 9, (28, 28, 32))

    for name in ("nx", "px"):
        s = b[name]
        for y in range(12):
            for x in range(4):
                n = h01(s.x + x, s.y + y, 920)
                if y < 9:
                    s.put(skin, x, y, E_TEE if n < 0.72 else E_TEE_L)
                else:
                    s.put(skin, x, y, E_JEANS if n < 0.7 else E_JEANS_L)
        for x in range(4):
            s.put(skin, x, 9, (28, 28, 32))

    field(b["top"], skin, (E_TEE, E_TEE_L, (34, 31, 30)), 930)
    field(b["bottom"], skin, (E_JEANS, E_JEANS_L, (26, 26, 30)), 940)

    # Outer denim jacket.
    o = box_faces(16, 32, 8, 12, 4)
    of = o["front"]
    # Collar/shoulders.
    for y in (0, 1):
        for x in range(8):
            n = h01(x, y, 1001)
            of.put(skin, x, y, E_DENIM if n < 0.55 else (E_DENIM_L if n < 0.8 else E_DENIM_D))
    # Open front: only the side panels and hem are denim.
    for y in range(2, 11):
        for x in (0, 1, 6, 7):
            n = h01(x, y, 1002)
            of.put(skin, x, y, E_DENIM if n < 0.62 else (E_DENIM_L if n < 0.84 else E_DENIM_D))
    for y in (10, 11):
        for x in (0, 1, 6, 7):
            of.put(skin, x, y, E_DENIM_D if h01(x, y, 1003) < 0.6 else E_DENIM)
    # Stitching along the lapels and hem.
    for y in range(2, 10):
        if y % 2 == 0:
            of.put(skin, 1, y, E_STITCH)
            of.put(skin, 6, y, E_STITCH)
    for x in (0, 1, 6, 7):
        of.put(skin, x, 10, E_STITCH if x % 2 else E_DENIM_D)
    # Chest pockets with copper buttons.
    for y in (4, 5):
        for x in (1, 2, 5, 6):
            of.put(skin, x, y, E_DENIM_L if h01(x, y, 1004) < 0.5 else E_DENIM)
    of.put(skin, 2, 5, E_COPPER)
    of.put(skin, 5, 5, E_COPPER)
    of.put(skin, 2, 4, E_STITCH)
    of.put(skin, 5, 4, E_STITCH)
    # A short copper chain at the right hip.
    for x, y in ((6, 7), (6, 8), (5, 9), (5, 10)):
        of.put(skin, x, y, E_COPPER if (x + y) % 2 else E_COPPER_D)

    ob = o["back"]
    for y in range(12):
        for x in range(8):
            n = h01(x, y, 1010)
            ob.put(skin, x, y, E_DENIM if n < 0.62 else (E_DENIM_L if n < 0.84 else E_DENIM_D))
    # Yoke, seams, hem.
    for x in range(8):
        ob.put(skin, x, 2, E_DENIM_L if x % 2 else E_DENIM_D)
        ob.put(skin, x, 3, E_STITCH if x % 3 == 0 else E_DENIM)
        ob.put(skin, x, 10, E_DENIM_D if x % 2 else E_STITCH)
    for y in range(2, 10):
        ob.put(skin, 4, y, E_DENIM_D if y % 3 else E_DENIM_L)
    # Embroidered ember/flame on the back.
    ember = [
        (4, 4, E_COPPER_L),
        (3, 5, E_COPPER),
        (4, 5, (226, 116, 51)),
        (5, 5, E_COPPER),
        (2, 6, E_COPPER_D),
        (3, 6, E_COPPER_L),
        (4, 6, (238, 153, 73)),
        (5, 6, E_COPPER_L),
        (6, 6, E_COPPER_D),
        (3, 7, E_COPPER_D),
        (4, 7, E_COPPER),
        (5, 7, E_COPPER_D),
        (4, 8, E_COPPER_D),
    ]
    for x, y, c in ember:
        ob.put(skin, x, y, c)
    # Leather patch at the lower left.
    for y in (8, 9):
        for x in (1, 2):
            ob.put(skin, x, y, (94, 62, 42) if h01(x, y, 1011) < 0.5 else (117, 79, 53))
    ob.put(skin, 2, 8, E_STITCH)
    ob.put(skin, 2, 9, E_COPPER_D)

    for name in ("nx", "px"):
        s = o[name]
        for y in range(12):
            for x in range(4):
                n = h01(s.x + x, s.y + y, 1020)
                s.put(skin, x, y, E_DENIM if n < 0.62 else (E_DENIM_L if n < 0.84 else E_DENIM_D))
        for y in range(1, 10, 2):
            s.put(skin, 0, y, E_STITCH)
        # Side pocket and rivet.
        rect(s, skin, 1, 5, 2, 6, E_DENIM_D)
        s.put(skin, 1, 5, E_DENIM_L)
        s.put(skin, 2, 6, E_COPPER)
        # Hem.
        for x in range(4):
            s.put(skin, x, 10, E_DENIM_D)
            s.put(skin, x, 11, E_DENIM_D if x % 2 else E_DENIM)

    ot = o["top"]
    for y in range(4):
        for x in range(8):
            if 2 <= x <= 5 and y <= 1:
                ot.put(skin, x, y, E_TEE if h01(x, y, 1030) < 0.7 else E_TEE_L)
            else:
                n = h01(x, y, 1031)
                ot.put(skin, x, y, E_DENIM if n < 0.62 else (E_DENIM_L if n < 0.84 else E_DENIM_D))
    field(o["bottom"], skin, (E_DENIM_D, E_DENIM, E_STITCH), 1040)


def ember_arms(skin: Skin) -> None:
    # Slim arms are 3 pixels wide.  Base arms are skin; the jacket sleeves are
    # outer-layer geometry and stop above the hands.
    for side, arm, outer, salt in (
        ("right", box_faces(40, 16, 3, 12, 4), box_faces(40, 32, 3, 12, 4), 1100),
        ("left", box_faces(32, 48, 3, 12, 4), box_faces(48, 48, 3, 12, 4), 1140),
    ):
        for name in ("front", "nx", "px", "back"):
            f = arm[name]
            field(f, skin, (E_SKIN, E_SKIN_L, E_SKIN_D), salt + FACE_SALT[name])
            # Knuckles / fingers at the bottom.
            for x in range(f.w):
                f.put(skin, x, 11, mix(E_SKIN_L, E_SKIN, 0.35))
            if side == "left":
                # Copper bracelet on the left wrist.
                for x in range(f.w):
                    f.put(skin, x, 9, E_COPPER if x % 2 else E_COPPER_D)
                    f.put(skin, x, 8, E_COPPER_D)
            else:
                f.put(skin, 1, 9, E_SKIN_SH)
        field(arm["top"], skin, (E_SKIN, E_SKIN_L, E_SKIN_D), salt + 2)
        field(arm["bottom"], skin, (E_SKIN_SH, E_SKIN_D, E_SKIN), salt + 3)

        # Denim sleeve.
        for name in ("front", "nx", "px", "back"):
            f = outer[name]
            for y in range(9):
                for x in range(f.w):
                    n = h01(f.x + x, f.y + y, salt + 20)
                    f.put(skin, x, y, E_DENIM if n < 0.6 else (E_DENIM_L if n < 0.84 else E_DENIM_D))
            # Seam and cuff.
            for y in range(1, 8, 2):
                f.put(skin, 0, y, E_STITCH)
            for x in range(f.w):
                f.put(skin, x, 8, E_DENIM_L if x % 2 else E_STITCH)
                f.put(skin, x, 7, E_DENIM_D)
            # Elbow patch.
            rect(f, skin, 0, 5, f.w - 1, 6, E_DENIM_D)
            if side == "left":
                # A tiny tear showing the base skin.
                f.put(skin, 1, 6, (0, 0, 0), 0)
            else:
                f.put(skin, 1, 8, E_COPPER)
        field(outer["top"], skin, (E_DENIM, E_DENIM_L, E_DENIM_D), salt + 22)
        # Hand rows in the outer layer stay transparent.


def ember_legs(skin: Skin) -> None:
    for leg, outer, salt in (
        (box_faces(0, 16, 4, 12, 4), box_faces(0, 32, 4, 12, 4), 1200),
        (box_faces(16, 48, 4, 12, 4), box_faces(0, 48, 4, 12, 4), 1240),
    ):
        for name in ("front", "nx", "px", "back"):
            f = leg[name]
            for y in range(12):
                for x in range(f.w):
                    n = h01(f.x + x, f.y + y, salt)
                    if y < 8:
                        c = E_JEANS if n < 0.68 else (E_JEANS_L if n < 0.9 else (27, 27, 31))
                    else:
                        c = E_BOOT if n < 0.62 else (E_BOOT_L if n < 0.88 else (25, 22, 22))
                    f.put(skin, x, y, c)
            # Knee rip and copper rivet.
            for x in (1, 2):
                f.put(skin, x, 5, mix(E_SKIN_D, E_JEANS, 0.45))
            f.put(skin, 2, 6, E_COPPER)
            for x in range(f.w):
                f.put(skin, x, 11, (23, 20, 20))
                if x % 2 == 0:
                    f.put(skin, x, 9, E_BOOT_L)
        field(leg["top"], skin, (E_JEANS, E_JEANS_L, (27, 27, 31)), salt + 2)
        field(leg["bottom"], skin, ((23, 20, 20), E_BOOT, E_BOOT_L), salt + 3)

        # Outer leg layer: a chain loop and boot cuff.
        for name in ("front", "nx", "px", "back"):
            f = outer[name]
            for x in range(f.w):
                f.put(skin, x, 8, E_BOOT_L if x % 2 else E_BOOT)
                f.put(skin, x, 9, E_BOOT)
            if name == "front":
                for x, y in ((1, 3), (1, 4), (0, 5), (0, 6)):
                    f.put(skin, x, y, E_COPPER if (x + y) % 2 else E_COPPER_D)
        field(outer["top"], skin, (E_JEANS, E_JEANS_L, (27, 27, 31)), salt + 22)


def make_ember() -> Skin:
    skin = Skin()
    ember_head(skin)
    ember_body(skin)
    ember_arms(skin)
    ember_legs(skin)
    return skin


# ---------------------------------------------------------------------------
# output
# ---------------------------------------------------------------------------


def write_skin(name: str, skin: Skin, out_dir: Path) -> None:
    img = skin.image()
    png = out_dir / f"{name}.png"
    raw = out_dir / f"{name}.rgba"
    img.save(png, format="PNG", optimize=False, compress_level=9)
    data = img.tobytes()
    if len(data) != 64 * 64 * 4:
        raise RuntimeError(f"unexpected raw size for {name}: {len(data)}")
    raw.write_bytes(data)
    opaque = sum(1 for p in img.getdata() if p[3] == 255)
    partial = sum(1 for p in img.getdata() if 0 < p[3] < 255)
    print(f"{name}: {png.name} + {raw.name} ({opaque} opaque, {partial} partial texels)")


def main() -> None:
    out_dir = Path(__file__).resolve().parent
    write_skin("alder", make_alder(), out_dir)
    write_skin("ember", make_ember(), out_dir)


if __name__ == "__main__":
    main()
