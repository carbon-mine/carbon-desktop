// SPDX-License-Identifier: MPL-2.0
use carbon_profiles::icons::{InstanceIcon, catalog};
use gpui::{AssetSource, Pixels, Rgba, SharedString, Svg, prelude::*, svg};
use std::borrow::Cow;

#[derive(Clone, Copy)]
pub enum Icon {
    Builtin(&'static InstanceIcon),
    Add,
    Account,
    Play,
    Instance,
    Pack,
}

impl Icon {
    fn path(self) -> SharedString {
        match self {
            Self::Builtin(icon) => return format!("instances/{}.svg", icon.id).into(),
            Self::Add => "icons/add.svg",
            Self::Account => "icons/account.svg",
            Self::Play => "icons/play.svg",
            Self::Instance => "icons/instance.svg",
            Self::Pack => "icons/pack.svg",
        }
        .into()
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
        if let Some(id) = path
            .strip_prefix("instances/")
            .and_then(|id| id.strip_suffix(".svg"))
        {
            return Ok(catalog().iter().find(|icon| icon.id == id).map(|icon| {
                use std::fmt::Write;
                let mut svg = String::from(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d=""#,
                );
                for (y, row) in icon.rows.iter().enumerate() {
                    for x in 0..12 {
                        if row & (1 << (11 - x)) != 0 {
                            write!(svg, "M{x} {y}h1v1h-1z").expect("write SVG path");
                        }
                    }
                }
                svg.push_str(r#""/></svg>"#);
                Cow::Owned(svg.into_bytes())
            }));
        }
        Ok(ICONS.iter().find_map(|(icon, bytes)| {
            (icon.path().as_ref() == path).then_some(Cow::Borrowed(*bytes))
        }))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .map(|(icon, _)| icon.path())
            .chain(catalog().iter().map(|icon| Icon::Builtin(icon).path()))
            .filter(|name| name.starts_with(path))
            .collect())
    }
}
