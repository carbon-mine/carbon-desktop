// SPDX-License-Identifier: MPL-2.0
use gpui::{
    Bounds, Image, ImageFormat, Pixels, Window, canvas, fill, point, prelude::*, px, rgb, size,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct Terrain {
    alder: Arc<Image>,
    ember: Arc<Image>,
}

impl Terrain {
    pub fn load() -> Self {
        let png = |bytes: &[u8]| Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec()));
        Self {
            alder: png(include_bytes!(
                "../../../../docs/design/map-prototype/alder-reach.png"
            )),
            ember: png(include_bytes!(
                "../../../../docs/design/map-prototype/ember-vault.png"
            )),
        }
    }

    pub fn surface(
        &self,
        coverage: Coverage,
        dimension: usize,
        camera: Camera,
        last_position: Option<(f32, f32)>,
    ) -> impl IntoElement {
        let source = if dimension == 1 {
            self.ember.clone()
        } else {
            self.alder.clone()
        };
        canvas(
            move |_, window, cx| source.use_render_image(window, cx),
            move |bounds, decoded, window, _| {
                if let Some(image) = decoded {
                    let extent = f32::from(bounds.size.width.max(bounds.size.height)) * camera.zoom;
                    let unit = extent / 512.;
                    let origin = point(
                        bounds.center().x - px(camera.x * unit),
                        bounds.center().y - px(camera.z * unit),
                    );
                    let terrain = Bounds::new(origin, size(px(extent), px(extent)));
                    let _ = window.paint_image(terrain, Default::default(), image, 0, false);
                    if coverage == Coverage::Partial {
                        let missing = Bounds::new(
                            point(origin.x + px(300. * unit), origin.y + px(238. * unit)),
                            size(px(212. * unit), px(122. * unit)),
                        );
                        hatch(missing.intersect(&bounds), window);
                    }
                    if let Some((x, z)) = last_position {
                        let marker = point(origin.x + px(x * unit), origin.y + px(z * unit));
                        let base =
                            Bounds::new(marker - point(px(5.), px(5.)), size(px(10.), px(10.)));
                        window.paint_quad(fill(base, rgb(0x262829)).corner_radii(px(5.)));
                        let dot = Bounds::new(marker - point(px(3.), px(3.)), size(px(6.), px(6.)));
                        window.paint_quad(fill(dot, rgb(0xf3eee3)).corner_radii(px(3.)));
                    }
                }
            },
        )
        .size_full()
    }
}

fn hatch(bounds: Bounds<Pixels>, window: &mut Window) {
    if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
        return;
    }
    window.paint_quad(fill(bounds, rgb(0x292c2e)));
    let step = 14;
    let w = f32::from(bounds.size.width).ceil() as usize;
    let h = f32::from(bounds.size.height).ceil() as usize;
    for y in (0..h).step_by(step) {
        for x in ((y / step % 2) * step..w).step_by(step * 2) {
            let cell = Bounds::new(
                bounds.origin + point(px(x as f32), px(y as f32)),
                size(px(step as f32), px(step as f32)),
            );
            window.paint_quad(fill(cell.intersect(&bounds), rgb(0x333638)));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Saved,
    Partial,
    Empty,
}

impl Coverage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Saved => "Saved terrain",
            Self::Partial => "Partial terrain · checkered areas are missing",
            Self::Empty => "No saved terrain",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Camera {
    pub x: f32,
    pub z: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            x: 256.,
            z: 256.,
            zoom: 1.,
        }
    }
}

impl Camera {
    pub fn pan(&mut self, dx: f32, dz: f32) {
        self.x = (self.x + dx).clamp(0., 512.);
        self.z = (self.z + dz).clamp(0., 512.);
    }
    pub fn magnify(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.5, 8.);
    }
}
