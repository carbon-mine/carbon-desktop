// SPDX-License-Identifier: MPL-2.0
use gpui::{AssetSource, Pixels, Rgba, SharedString, Svg, prelude::*, svg};
use std::borrow::Cow;

#[derive(Clone, Copy)]
pub enum Icon {
    Add,
    Account,
    Play,
    Instance,
    Pack,
}

impl Icon {
    fn path(self) -> &'static str {
        match self {
            Self::Add => "icons/add.svg",
            Self::Account => "icons/account.svg",
            Self::Play => "icons/play.svg",
            Self::Instance => "icons/instance.svg",
            Self::Pack => "icons/pack.svg",
        }
    }

    pub fn render(self, size: Pixels, color: Rgba) -> Svg {
        svg()
            .path(self.path())
            .flex_none()
            .size(size)
            .text_color(color)
    }
}

pub fn brand_mark(light_ink: bool, height: Pixels) -> Svg {
    let (path, shade) = if light_ink {
        ("brand/mark-light.svg", 0xf4f4f4)
    } else {
        ("brand/mark-dark.svg", 0x202427)
    };
    svg()
        .path(path)
        .flex_none()
        .h(height)
        .w(height * (81. / 87.75))
        .text_color(gpui::rgb(shade))
}

const BRAND: [(&str, &[u8]); 3] = [
    (
        "brand/home-identity-shadow.png",
        include_bytes!("../assets/home-identity-shadow.png"),
    ),
    (
        "brand/mark-light.svg",
        include_bytes!("../../../design-system/inherited/brand/mark-light.svg"),
    ),
    (
        "brand/mark-dark.svg",
        include_bytes!("../../../design-system/inherited/brand/mark-dark.svg"),
    ),
];

pub struct Assets;

const ICONS: [(Icon, &[u8]); 5] = [
    (Icon::Add, include_bytes!("../assets/icons/add.svg")),
    (Icon::Account, include_bytes!("../assets/icons/account.svg")),
    (Icon::Play, include_bytes!("../assets/icons/play.svg")),
    (
        Icon::Instance,
        include_bytes!("../assets/icons/instance.svg"),
    ),
    (Icon::Pack, include_bytes!("../assets/icons/pack.svg")),
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, data)) = BRAND.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*data)));
        }
        Ok(ICONS
            .iter()
            .find_map(|(icon, bytes)| (icon.path() == path).then_some(Cow::Borrowed(*bytes))))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .map(|(icon, _)| icon.path())
            .chain(BRAND.iter().map(|(name, _)| *name))
            .filter(|name| name.starts_with(path))
            .map(Into::into)
            .collect())
    }
}
