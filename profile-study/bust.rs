// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A small, dependency-free software renderer for Minecraft Java skin busts.
//!
//! The renderer consumes a 64x64 RGBA8 skin and produces a square, transparent
//! RGBA8 image containing the head, torso and arms.  Both the base skin and the
//! translucent outer layer are sampled with nearest-neighbour filtering.  The
//! camera orbits the bust; yaw = 0 and pitch = 0 are the straight-on front view.
//!
//! Geometry is deliberately Minecraft-shaped: the head is 8x8x8, the torso is
//! 8x12x4 and the arms are either 4x12x4 (classic) or 3x12x4 (slim).  There are
//! no legs in the bust, exactly like the upper part of the player model.

#![allow(clippy::many_single_char_names)]

const TEX_W: usize = 64;
const TEX_H: usize = 64;
const SKIN_BYTES: usize = TEX_W * TEX_H * 4;
const CAM_DIST: f32 = 80.0;
const FOCAL_PER_SIDE: f32 = 3.30;
const OVERLAY_INFLATE: f32 = 0.30;

/// Render a Minecraft Java skin bust.
///
/// `skin` must be a 64x64 RGBA8 buffer (16384 bytes).  A shorter buffer yields
/// a fully transparent image rather than a panic.  `slim` selects the 3-pixel
/// arm model; `yaw` and `pitch` are in degrees, with zero meaning the front
/// view.  `side` is the width and height of the returned square RGBA8 image.
pub fn render(skin: &[u8], slim: bool, yaw: f32, pitch: f32, side: usize) -> Vec<u8> {
    let pixel_count = side.saturating_mul(side);
    let mut out = vec![0u8; pixel_count.saturating_mul(4)];
    if side == 0 || skin.len() < SKIN_BYTES {
        return out;
    }

    let yaw = if yaw.is_finite() { yaw } else { 0.0 };
    let pitch = if pitch.is_finite() { pitch } else { 0.0 };
    let camera = Camera::new(side as f32, yaw, pitch);

    let mut base = Vec::with_capacity(36);
    let mut overlay = Vec::with_capacity(36);
    build_model(slim, &mut base, &mut overlay);

    let mut zbuf = vec![f32::INFINITY; pixel_count];

    // Base texels first.  The inflated outer layer is drawn afterwards, which
    // makes a transparent outer texel leave the base texel visible.
    for face in &base {
        draw_face(&mut out, &mut zbuf, skin, side, &camera, face);
    }
    for face in &overlay {
        draw_face(&mut out, &mut zbuf, skin, side, &camera, face);
    }

    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct V3 {
    x: f32,
    y: f32,
    z: f32,
}

impl V3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }

    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }

    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }

    fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn normalized(self) -> Self {
        let n = self.length();
        if n > 1.0e-8 {
            self.mul(1.0 / n)
        } else {
            Self::new(0.0, 0.0, 0.0)
        }
    }
}

struct Camera {
    pos: V3,
    right: V3,
    up: V3,
    forward: V3,
    focal: f32,
    cx: f32,
    cy: f32,
}

impl Camera {
    fn new(side: f32, yaw_deg: f32, pitch_deg: f32) -> Self {
        let yaw = yaw_deg.to_radians();
        let pitch = pitch_deg.to_radians();
        let target = V3::new(0.0, 10.0, 0.0);

        // Direction from the target to the camera.  At zero yaw/pitch this is
        // +Z, so the camera sees the front of the model.
        let dir = V3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        );
        let pos = target.add(dir.mul(CAM_DIST));
        let forward = dir.mul(-1.0);

        let mut right = forward.cross(V3::new(0.0, 1.0, 0.0)).normalized();
        if right.length() < 1.0e-5 {
            // Straight-up or straight-down view: choose a stable screen-right
            // direction instead of producing NaNs.
            right = forward.cross(V3::new(0.0, 0.0, 1.0)).normalized();
        }
        let up = right.cross(forward).normalized();

        Self {
            pos,
            right,
            up,
            forward,
            focal: side * FOCAL_PER_SIDE,
            cx: side * 0.5,
            cy: side * 0.5,
        }
    }

    /// Project a world-space point to screen space.  The returned z is the
    /// positive distance along the camera's forward axis.
    fn project(&self, p: V3) -> Option<Vert> {
        let rel = p.sub(self.pos);
        let z = rel.dot(self.forward);
        if z <= 0.05 {
            return None;
        }
        let x = rel.dot(self.right);
        let y = rel.dot(self.up);
        Some(Vert {
            sx: self.cx + self.focal * x / z,
            sy: self.cy - self.focal * y / z,
            z,
            u: 0.0,
            v: 0.0,
        })
    }
}

#[derive(Clone, Copy)]
struct Uv {
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
}

impl Uv {
    fn new(u: usize, v: usize, w: usize, h: usize) -> Self {
        Self {
            u0: u as f32,
            v0: v as f32,
            u1: (u + w) as f32,
            v1: (v + h) as f32,
        }
    }
}

struct BoxUvs {
    /// -X side (the "right" rectangle in the skin layout).
    nx: Uv,
    /// +X side (the "left" rectangle in the skin layout).
    px: Uv,
    /// -Y side.
    ny: Uv,
    /// +Y side.
    py: Uv,
    /// -Z side.
    nz: Uv,
    /// +Z side (the face).
    pz: Uv,
}

/// Standard Java skin cube layout.  `u`, `v` are the top-left corner of the
/// cube's UV block; `w`, `h`, `d` are its width, height and depth in pixels.
fn box_uvs(u: usize, v: usize, w: usize, h: usize, d: usize) -> BoxUvs {
    BoxUvs {
        nx: Uv::new(u, v + d, d, h),
        pz: Uv::new(u + d, v + d, w, h),
        px: Uv::new(u + d + w, v + d, d, h),
        nz: Uv::new(u + 2 * d + w, v + d, w, h),
        py: Uv::new(u + d, v, w, d),
        ny: Uv::new(u + d + w, v, w, d),
    }
}

struct Face {
    p: [V3; 4],
    uv: [(f32, f32); 4],
    n: V3,
    bounds: (f32, f32, f32, f32),
}

impl Face {
    fn new(p: [V3; 4], uv: Uv, n: V3) -> Self {
        Self {
            p,
            uv: [
                (uv.u0, uv.v0),
                (uv.u1, uv.v0),
                (uv.u1, uv.v1),
                (uv.u0, uv.v1),
            ],
            n,
            bounds: (uv.u0, uv.u1, uv.v0, uv.v1),
        }
    }
}

fn push_box(dst: &mut Vec<Face>, min: V3, max: V3, uv: BoxUvs) {
    let p = |x: f32, y: f32, z: f32| V3::new(x, y, z);

    // Front (+Z).  Texture u follows +X and v follows -Y.
    dst.push(Face::new(
        [
            p(min.x, max.y, max.z),
            p(max.x, max.y, max.z),
            p(max.x, min.y, max.z),
            p(min.x, min.y, max.z),
        ],
        uv.pz,
        V3::new(0.0, 0.0, 1.0),
    ));

    // Back (-Z).  Seen from behind, texture u follows -X.
    dst.push(Face::new(
        [
            p(max.x, max.y, min.z),
            p(min.x, max.y, min.z),
            p(min.x, min.y, min.z),
            p(max.x, min.y, min.z),
        ],
        uv.nz,
        V3::new(0.0, 0.0, -1.0),
    ));

    // -X side (skin "right").  Texture u follows -Z.
    dst.push(Face::new(
        [
            p(min.x, max.y, min.z),
            p(min.x, max.y, max.z),
            p(min.x, min.y, max.z),
            p(min.x, min.y, min.z),
        ],
        uv.nx,
        V3::new(-1.0, 0.0, 0.0),
    ));

    // +X side (skin "left").  Texture u follows +Z.
    dst.push(Face::new(
        [
            p(max.x, max.y, max.z),
            p(max.x, max.y, min.z),
            p(max.x, min.y, min.z),
            p(max.x, min.y, max.z),
        ],
        uv.px,
        V3::new(1.0, 0.0, 0.0),
    ));

    // Top (+Y).  The bottom edge of the top texture is the front edge.
    dst.push(Face::new(
        [
            p(min.x, max.y, min.z),
            p(max.x, max.y, min.z),
            p(max.x, max.y, max.z),
            p(min.x, max.y, max.z),
        ],
        uv.py,
        V3::new(0.0, 1.0, 0.0),
    ));

    // Bottom (-Y).  The top edge of the bottom texture is the front edge.
    dst.push(Face::new(
        [
            p(min.x, min.y, max.z),
            p(max.x, min.y, max.z),
            p(max.x, min.y, min.z),
            p(min.x, min.y, min.z),
        ],
        uv.ny,
        V3::new(0.0, -1.0, 0.0),
    ));
}

fn build_model(slim: bool, base: &mut Vec<Face>, overlay: &mut Vec<Face>) {
    let arm_w = if slim { 3.0 } else { 4.0 };
    let d = OVERLAY_INFLATE;

    // Torso.
    push_box(
        base,
        V3::new(-4.0, 0.0, -2.0),
        V3::new(4.0, 12.0, 2.0),
        box_uvs(16, 16, 8, 12, 4),
    );
    push_box(
        overlay,
        V3::new(-4.0 - d, -d, -2.0 - d),
        V3::new(4.0 + d, 12.0 + d, 2.0 + d),
        box_uvs(16, 32, 8, 12, 4),
    );

    // Head sits directly on the torso.
    push_box(
        base,
        V3::new(-4.0, 12.0, -4.0),
        V3::new(4.0, 20.0, 4.0),
        box_uvs(0, 0, 8, 8, 8),
    );
    push_box(
        overlay,
        V3::new(-4.0 - d, 12.0 - d, -4.0 - d),
        V3::new(4.0 + d, 20.0 + d, 4.0 + d),
        box_uvs(32, 0, 8, 8, 8),
    );

    // Right arm (the character's right; world -X, screen-left from the front).
    let rx0 = -4.0 - arm_w;
    push_box(
        base,
        V3::new(rx0, 0.0, -2.0),
        V3::new(-4.0, 12.0, 2.0),
        box_uvs(40, 16, arm_w as usize, 12, 4),
    );
    push_box(
        overlay,
        V3::new(rx0 - d, -d, -2.0 - d),
        V3::new(-4.0 + d, 12.0 + d, 2.0 + d),
        box_uvs(40, 32, arm_w as usize, 12, 4),
    );

    // Left arm (the character's left; world +X, screen-right from the front).
    push_box(
        base,
        V3::new(4.0, 0.0, -2.0),
        V3::new(4.0 + arm_w, 12.0, 2.0),
        box_uvs(32, 48, arm_w as usize, 12, 4),
    );
    push_box(
        overlay,
        V3::new(4.0 - d, -d, -2.0 - d),
        V3::new(4.0 + arm_w + d, 12.0 + d, 2.0 + d),
        box_uvs(48, 48, arm_w as usize, 12, 4),
    );
}

#[derive(Clone, Copy, Debug, Default)]
struct Vert {
    sx: f32,
    sy: f32,
    z: f32,
    u: f32,
    v: f32,
}

fn edge(a: &Vert, b: &Vert, px: f32, py: f32) -> f32 {
    (b.sx - a.sx) * (py - a.sy) - (b.sy - a.sy) * (px - a.sx)
}

fn face_shade(n: V3, cam: &Camera) -> f32 {
    // Put the normal in camera space so the studio lights stay attached to the
    // viewer instead of to the model.  That gives even, readable lighting at
    // every yaw and pitch.
    let nv = V3::new(
        n.dot(cam.right),
        n.dot(cam.up),
        n.dot(cam.forward.mul(-1.0)),
    )
    .normalized();

    let key = nv.dot(V3::new(-0.42, 0.78, 0.72).normalized()).max(0.0);
    let fill = nv.dot(V3::new(0.62, 0.18, 0.58).normalized()).max(0.0);
    let rim = nv.dot(V3::new(0.10, 0.30, -0.95).normalized()).max(0.0);

    (0.58 + 0.33 * key + 0.13 * fill + 0.03 * rim).clamp(0.0, 1.0)
}

fn draw_face(
    out: &mut [u8],
    zbuf: &mut [f32],
    skin: &[u8],
    side: usize,
    cam: &Camera,
    face: &Face,
) {
    let center = face
        .p
        .iter()
        .fold(V3::default(), |acc, p| acc.add(*p))
        .mul(0.25);
    if face.n.dot(cam.pos.sub(center)) <= 0.0 {
        return; // Back face of a closed box.
    }

    let shade = face_shade(face.n, cam);
    let mut v = [Vert::default(); 4];
    for (i, vertex) in v.iter_mut().enumerate() {
        match cam.project(face.p[i]) {
            Some(mut p) => {
                p.u = face.uv[i].0;
                p.v = face.uv[i].1;
                *vertex = p;
            }
            None => return,
        }
    }

    let (umin, umax, vmin, vmax) = face.bounds;
    raster_tri(
        out, zbuf, skin, side, v[0], v[1], v[2], shade, umin, umax, vmin, vmax,
    );
    raster_tri(
        out, zbuf, skin, side, v[0], v[2], v[3], shade, umin, umax, vmin, vmax,
    );
}

#[allow(clippy::too_many_arguments)]
fn raster_tri(
    out: &mut [u8],
    zbuf: &mut [f32],
    skin: &[u8],
    side: usize,
    a: Vert,
    b: Vert,
    c: Vert,
    shade: f32,
    umin: f32,
    umax: f32,
    vmin: f32,
    vmax: f32,
) {
    let area = edge(&a, &b, c.sx, c.sy);
    if area.abs() < 1.0e-7 {
        return;
    }

    let min_x = a.sx.min(b.sx).min(c.sx).floor().max(0.0);
    let min_y = a.sy.min(b.sy).min(c.sy).floor().max(0.0);
    let max_x = a.sx.max(b.sx).max(c.sx).ceil().min((side - 1) as f32);
    let max_y = a.sy.max(b.sy).max(c.sy).ceil().min((side - 1) as f32);
    if max_x < min_x || max_y < min_y {
        return;
    }

    let x0 = min_x as usize;
    let y0 = min_y as usize;
    let x1 = max_x as usize;
    let y1 = max_y as usize;

    for py in y0..=y1 {
        let fy = py as f32 + 0.5;
        for px in x0..=x1 {
            let fx = px as f32 + 0.5;
            let w0 = edge(&b, &c, fx, fy) / area;
            let w1 = edge(&c, &a, fx, fy) / area;
            let w2 = edge(&a, &b, fx, fy) / area;
            if w0 < -1.0e-5 || w1 < -1.0e-5 || w2 < -1.0e-5 {
                continue;
            }

            let inv_z = w0 / a.z + w1 / b.z + w2 / c.z;
            if inv_z <= 0.0 {
                continue;
            }
            let z = 1.0 / inv_z;
            let idx = py * side + px;
            if z >= zbuf[idx] {
                continue;
            }

            let mut u = (w0 * a.u / a.z + w1 * b.u / b.z + w2 * c.u / c.z) / inv_z;
            let mut v = (w0 * a.v / a.z + w1 * b.v / b.z + w2 * c.v / c.z) / inv_z;
            u = u.clamp(umin, umax - 0.001);
            v = v.clamp(vmin, vmax - 0.001);

            let tx = u.floor() as usize;
            let ty = v.floor() as usize;
            let si = (ty * TEX_W + tx) * 4;
            let alpha = skin[si + 3];
            if alpha == 0 {
                continue;
            }

            let sr = (skin[si] as f32 * shade).min(255.0) as u8;
            let sg = (skin[si + 1] as f32 * shade).min(255.0) as u8;
            let sb = (skin[si + 2] as f32 * shade).min(255.0) as u8;
            let oi = idx * 4;

            if alpha == 255 {
                out[oi] = sr;
                out[oi + 1] = sg;
                out[oi + 2] = sb;
                out[oi + 3] = 255;
            } else {
                let sa = alpha as f32 / 255.0;
                let da = out[oi + 3] as f32 / 255.0;
                let oa = sa + da * (1.0 - sa);
                if oa > 1.0e-6 {
                    out[oi] =
                        ((sr as f32 * sa + out[oi] as f32 * da * (1.0 - sa)) / oa).min(255.0) as u8;
                    out[oi + 1] = ((sg as f32 * sa + out[oi + 1] as f32 * da * (1.0 - sa)) / oa)
                        .min(255.0) as u8;
                    out[oi + 2] = ((sb as f32 * sa + out[oi + 2] as f32 * da * (1.0 - sa)) / oa)
                        .min(255.0) as u8;
                    out[oi + 3] = (oa * 255.0).min(255.0) as u8;
                }
            }
            zbuf[idx] = z;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 3] = [235, 40, 40];
    const GREEN: [u8; 3] = [40, 220, 60];
    const BLUE: [u8; 3] = [40, 90, 235];
    const YELLOW: [u8; 3] = [235, 220, 40];
    const MAGENTA: [u8; 3] = [220, 40, 220];
    const CYAN: [u8; 3] = [40, 220, 220];
    const ORANGE: [u8; 3] = [235, 135, 35];
    const SKY: [u8; 3] = [120, 210, 255];
    const WHITE: [u8; 3] = [250, 250, 250];
    const LAVENDER: [u8; 3] = [180, 120, 255];

    fn put(skin: &mut [u8], x: usize, y: usize, c: [u8; 3], a: u8) {
        let i = (y * 64 + x) * 4;
        skin[i] = c[0];
        skin[i + 1] = c[1];
        skin[i + 2] = c[2];
        skin[i + 3] = a;
    }

    fn fill_rect(skin: &mut [u8], x: usize, y: usize, w: usize, h: usize, c: [u8; 3], a: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                put(skin, xx, yy, c, a);
            }
        }
    }

    fn clear_outer(skin: &mut [u8]) {
        // Head hat, body jacket, right sleeve and left sleeve.
        for &(x, y, w, h) in &[
            (32usize, 0usize, 32usize, 16usize),
            (16, 32, 24, 16),
            (40, 32, 16, 16),
            (48, 48, 16, 16),
        ] {
            fill_rect(skin, x, y, w, h, [0, 0, 0], 0);
        }
    }

    fn test_skin() -> Vec<u8> {
        let mut skin = vec![0u8; SKIN_BYTES];
        clear_outer(&mut skin);

        // Head base, one solid colour per face.
        fill_rect(&mut skin, 0, 8, 8, 8, BLUE, 255); // -X
        fill_rect(&mut skin, 8, 8, 8, 8, RED, 255); // front
        fill_rect(&mut skin, 16, 8, 8, 8, YELLOW, 255); // +X
        fill_rect(&mut skin, 24, 8, 8, 8, MAGENTA, 255); // back
        fill_rect(&mut skin, 8, 0, 8, 8, GREEN, 255); // top
        fill_rect(&mut skin, 16, 0, 8, 8, CYAN, 255); // bottom

        // Body base.
        fill_rect(&mut skin, 16, 20, 4, 12, [90, 55, 160], 255); // -X
        fill_rect(&mut skin, 20, 20, 8, 12, ORANGE, 255); // front
        fill_rect(&mut skin, 28, 20, 4, 12, [235, 90, 180], 255); // +X
        fill_rect(&mut skin, 32, 20, 8, 12, [90, 235, 170], 255); // back
        fill_rect(&mut skin, 20, 16, 8, 4, [120, 235, 40], 255); // top
        fill_rect(&mut skin, 28, 16, 8, 4, [55, 55, 65], 255); // bottom

        // Right arm base: sky-blue front, grey sides.
        fill_rect(&mut skin, 40, 20, 4, 12, [150, 150, 150], 255); // -X
        fill_rect(&mut skin, 44, 20, 4, 12, SKY, 255); // front
        fill_rect(&mut skin, 48, 20, 4, 12, [130, 130, 130], 255); // +X
        fill_rect(&mut skin, 52, 20, 4, 12, [110, 110, 110], 255); // back

        // Left arm base: lavender front, sage sides.
        fill_rect(&mut skin, 32, 52, 4, 12, [120, 180, 120], 255); // -X
        fill_rect(&mut skin, 36, 52, 4, 12, LAVENDER, 255); // front
        fill_rect(&mut skin, 40, 52, 4, 12, [100, 160, 100], 255); // +X
        fill_rect(&mut skin, 44, 52, 4, 12, [90, 140, 90], 255); // back

        skin
    }

    fn is_opaque(p: &[u8]) -> bool {
        p[3] > 200
    }

    fn gt(a: u8, b: u8, d: u8) -> bool {
        a as i32 > b as i32 + d as i32
    }

    fn is_red(p: &[u8]) -> bool {
        is_opaque(p) && gt(p[0], p[1], 70) && gt(p[0], p[2], 70) && p[1] < 100
    }

    fn is_green(p: &[u8]) -> bool {
        is_opaque(p) && gt(p[1], p[0], 50) && gt(p[1], p[2], 50)
    }

    fn is_blue(p: &[u8]) -> bool {
        is_opaque(p) && gt(p[2], p[0], 100) && gt(p[2], p[1], 50)
    }

    fn is_yellow(p: &[u8]) -> bool {
        is_opaque(p) && gt(p[0], p[2], 60) && gt(p[1], p[2], 60) && p[0] > 100 && p[1] > 150
    }

    fn is_magenta(p: &[u8]) -> bool {
        is_opaque(p)
            && gt(p[0], p[1], 50)
            && gt(p[2], p[1], 50)
            && (p[0] as i32 - p[2] as i32).abs() < 50
    }

    fn is_cyan(p: &[u8]) -> bool {
        is_opaque(p) && p[0] < 80 && gt(p[1], p[0], 50) && gt(p[2], p[0], 50)
    }

    fn is_orange(p: &[u8]) -> bool {
        is_opaque(p) && gt(p[0], p[1], 30) && gt(p[1], p[2], 25) && p[1] > 70
    }

    fn is_sky(p: &[u8]) -> bool {
        is_opaque(p) && gt(p[2], p[0], 50) && gt(p[2], p[1], 20) && p[1] > 100
    }

    fn is_white(p: &[u8]) -> bool {
        is_opaque(p) && p[0] > 180 && p[1] > 180 && p[2] > 180
    }

    fn is_lavender(p: &[u8]) -> bool {
        is_opaque(p) && gt(p[2], p[0], 30) && gt(p[0], p[1], 25)
    }

    fn count<F: Fn(&[u8]) -> bool>(img: &[u8], f: F) -> usize {
        img.chunks_exact(4).filter(|p| f(p)).count()
    }

    fn centroid<F: Fn(&[u8]) -> bool>(img: &[u8], side: usize, f: F) -> Option<(f32, f32)> {
        let mut n = 0u32;
        let mut sx = 0f64;
        let mut sy = 0f64;
        for y in 0..side {
            for x in 0..side {
                let i = (y * side + x) * 4;
                if f(&img[i..i + 4]) {
                    n += 1;
                    sx += x as f64;
                    sy += y as f64;
                }
            }
        }
        if n == 0 {
            None
        } else {
            Some(((sx / n as f64) as f32, (sy / n as f64) as f32))
        }
    }

    fn alpha_bbox(img: &[u8], side: usize) -> Option<(usize, usize, usize, usize)> {
        let mut minx = side;
        let mut miny = side;
        let mut maxx = 0usize;
        let mut maxy = 0usize;
        let mut any = false;
        for y in 0..side {
            for x in 0..side {
                if img[(y * side + x) * 4 + 3] > 0 {
                    any = true;
                    minx = minx.min(x);
                    miny = miny.min(y);
                    maxx = maxx.max(x);
                    maxy = maxy.max(y);
                }
            }
        }
        if any {
            Some((minx, miny, maxx, maxy))
        } else {
            None
        }
    }

    #[test]
    fn output_is_square_rgba_with_transparent_corners() {
        let skin = test_skin();
        let side = 128;
        let out = render(&skin, false, 0.0, 0.0, side);
        assert_eq!(out.len(), side * side * 4);
        assert_eq!(out[3], 0, "top-left corner must be transparent");
        assert_eq!(out[(side * side - 1) * 4 + 3], 0);
        assert!(count(&out, is_opaque) > side * side / 6);
        assert!(count(&out, |_| true) == side * side);
    }

    #[test]
    fn front_view_shows_front_faces_and_keeps_orientation() {
        let skin = test_skin();
        let side = 256;
        let out = render(&skin, false, 0.0, 0.0, side);

        let red = count(&out, is_red);
        let orange = count(&out, is_orange);
        let sky = count(&out, is_sky);
        let lavender = count(&out, is_lavender);
        assert!(red > 200, "head front missing: {}", red);
        assert!(orange > 200, "torso front missing: {}", orange);
        assert!(sky > 100, "right arm front missing: {}", sky);
        assert!(lavender > 100, "left arm front missing: {}", lavender);

        let head = centroid(&out, side, is_red).unwrap();
        let chest = centroid(&out, side, is_orange).unwrap();
        assert!(head.1 < chest.1, "head should be above chest");
        assert!(
            (head.0 - chest.0).abs() < 8.0,
            "head and chest should align"
        );

        let right = centroid(&out, side, is_sky).unwrap();
        let left = centroid(&out, side, is_lavender).unwrap();
        assert!(
            right.0 < left.0,
            "character's right arm should be screen-left"
        );

        assert_eq!(count(&out, is_green), 0, "top should not show from front");
        assert_eq!(
            count(&out, is_blue),
            0,
            "right side should not show from front"
        );
        assert_eq!(
            count(&out, is_magenta),
            0,
            "back should not show from front"
        );
    }

    #[test]
    fn yaw_rotates_through_sides_and_back() {
        let skin = test_skin();
        let side = 192;

        let right = render(&skin, false, 90.0, 0.0, side);
        assert!(
            count(&right, is_yellow) > 100,
            "+X side not visible at yaw 90"
        );
        assert!(
            count(&right, is_red) < 50,
            "front should be edge-on at yaw 90"
        );

        let left = render(&skin, false, -90.0, 0.0, side);
        assert!(
            count(&left, is_blue) > 100,
            "-X side not visible at yaw -90"
        );

        let back = render(&skin, false, 180.0, 0.0, side);
        assert!(
            count(&back, is_magenta) > 100,
            "back not visible at yaw 180"
        );
    }

    #[test]
    fn pitch_shows_top_and_bottom_faces() {
        let skin = test_skin();
        let side = 192;

        let top = render(&skin, false, 0.0, 85.0, side);
        assert!(count(&top, is_green) > 80, "top not visible from above");

        let bottom = render(&skin, false, 0.0, -85.0, side);
        assert!(
            count(&bottom, is_cyan) > 50,
            "head bottom not visible from below"
        );
    }

    #[test]
    fn slim_arms_are_narrower_than_classic_arms() {
        let skin = test_skin();
        let side = 256;
        let classic = render(&skin, false, 0.0, 0.0, side);
        let slim = render(&skin, true, 0.0, 0.0, side);

        let cb = alpha_bbox(&classic, side).unwrap();
        let sb = alpha_bbox(&slim, side).unwrap();
        let cw = cb.2 - cb.0 + 1;
        let sw = sb.2 - sb.0 + 1;
        assert!(
            cw >= sw + 10,
            "classic bust ({}px) should be wider than slim bust ({}px)",
            cw,
            sw
        );
        assert!(count(&classic, is_sky) > 100);
        assert!(count(&slim, is_sky) > 100);
    }

    #[test]
    fn transparent_outer_texels_leave_the_base_visible() {
        let mut skin = test_skin();
        let side = 256;
        let base = render(&skin, false, 0.0, 0.0, side);
        assert_eq!(count(&base, is_white), 0);
        assert!(count(&base, is_red) > 200);

        // A small opaque patch on the hat's front layer.  It must cover part of
        // the red face, proving that the outer layer is mapped and layered.
        fill_rect(&mut skin, 43, 11, 3, 3, WHITE, 255);
        let patched = render(&skin, false, 0.0, 0.0, side);
        assert!(count(&patched, is_white) > 0, "opaque hat patch missing");
        assert!(
            count(&patched, is_red) < count(&base, is_red),
            "hat patch should cover some face pixels"
        );
    }

    #[test]
    fn transparent_base_texels_show_background() {
        let mut skin = test_skin();
        // Punch a hole in the middle of the face.  With back-face culling the
        // camera looks straight through the head, so the hole stays transparent.
        fill_rect(&mut skin, 11, 11, 2, 2, [0, 0, 0], 0);
        let side = 256;
        let out = render(&skin, false, 0.0, 0.0, side);
        let i = (60 * side + 128) * 4;
        assert_eq!(
            out[i + 3],
            0,
            "transparent face texel should reveal background"
        );
    }

    #[test]
    fn opaque_skin_has_hard_alpha_edges() {
        let skin = test_skin();
        let out = render(&skin, false, 25.0, 12.0, 192);
        assert!(out.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 255));
    }

    #[test]
    fn invalid_skin_is_transparent() {
        let out = render(&[], false, 0.0, 0.0, 32);
        assert_eq!(out.len(), 32 * 32 * 4);
        assert!(out.iter().all(|&b| b == 0));
    }

    #[test]
    fn generated_character_fixtures_render() {
        let alder = include_bytes!("alder.rgba");
        let ember = include_bytes!("ember.rgba");
        assert_eq!(alder.len(), SKIN_BYTES);
        assert_eq!(ember.len(), SKIN_BYTES);

        let side = 192;
        let a = render(alder, false, 24.0, 12.0, side);
        let e = render(ember, true, -24.0, 12.0, side);

        let coverage = |img: &[u8]| count(img, is_opaque) as f32 / (side * side) as f32;
        assert!(coverage(&a) > 0.20 && coverage(&a) < 0.80);
        assert!(coverage(&e) > 0.20 && coverage(&e) < 0.80);

        let unique = |img: &[u8]| {
            let mut seen = std::collections::BTreeSet::new();
            for p in img.chunks_exact(4) {
                if p[3] > 0 {
                    seen.insert((p[0], p[1], p[2]));
                }
            }
            seen.len()
        };
        assert!(unique(&a) > 40, "alder fixture looks too flat");
        assert!(unique(&e) > 40, "ember fixture looks too flat");

        let ab = alpha_bbox(&a, side).unwrap();
        let eb = alpha_bbox(&e, side).unwrap();
        assert!(
            ab.2 - ab.0 > eb.2 - eb.0,
            "alder should be wider than ember"
        );
    }
}
