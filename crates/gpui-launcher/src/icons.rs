// SPDX-License-Identifier: MPL-2.0
use carbon_profiles::icons::{InstanceIcon, catalog};
use gpui::{AnyElement, AssetSource, Pixels, Rgba, SharedString, Svg, img, prelude::*, svg};
use std::borrow::Cow;

#[derive(Clone, Copy)]
pub enum Icon {
    Builtin(&'static InstanceIcon),
    Add,
    Account,
    ChevronDown,
    Play,
    Instance,
    Pack,
}

impl Icon {
    fn path(self) -> SharedString {
        match self {
            Self::Builtin(icon) => return format!("instances/{}.png", icon.id).into(),
            Self::ChevronDown => "icons/chevron-down.svg",
            Self::Add => "icons/add.svg",
            Self::Account => "icons/account.svg",
            Self::Play => "icons/play.svg",
            Self::Instance => "icons/instance.svg",
            Self::Pack => "icons/pack.svg",
        }
        .into()
    }

    pub fn render(self, size: Pixels, color: Rgba) -> AnyElement {
        if let Self::Builtin(_) = self {
            return img(self.path()).flex_none().size(size).into_any_element();
        }
        svg()
            .path(self.path())
            .flex_none()
            .size(size)
            .text_color(color)
            .into_any_element()
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
        "brand/home-header-shade.png",
        include_bytes!("../assets/home-header-shade.png"),
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

const ICONS: [(Icon, &[u8]); 6] = [
    (
        Icon::ChevronDown,
        include_bytes!("../assets/icons/chevron-down.svg"),
    ),
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
        if let Some(id) = path
            .strip_prefix("instances/")
            .and_then(|id| id.strip_suffix(".png"))
        {
            return Ok(catalog()
                .iter()
                .find(|icon| icon.id == id)
                .map(|icon| Cow::Borrowed(icon.png)));
        }
        if let Some((_, data)) = BRAND.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*data)));
        }
        Ok(ICONS.iter().find_map(|(icon, bytes)| {
            (icon.path().as_ref() == path).then_some(Cow::Borrowed(*bytes))
        }))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .map(|(icon, _)| icon.path())
            .chain(BRAND.iter().map(|(name, _)| SharedString::from(*name)))
            .chain(catalog().iter().map(|icon| Icon::Builtin(icon).path()))
            .filter(|name| name.starts_with(path))
            .collect())
    }
}
