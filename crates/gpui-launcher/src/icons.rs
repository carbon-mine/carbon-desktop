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
        Ok(ICONS
            .iter()
            .find_map(|(icon, bytes)| (icon.path() == path).then_some(Cow::Borrowed(*bytes))))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .map(|(icon, _)| icon.path())
            .filter(|name| name.starts_with(path))
            .map(Into::into)
            .collect())
    }
}
