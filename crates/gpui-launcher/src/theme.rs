/* SPDX-License-Identifier: MPL-2.0 */
use gpui::{
    App, Div, ElementId, Hsla, Rgba, SharedString, Window, WindowAppearance, div, prelude::*, px,
    rgb, rgba,
};
use gpui_component::{
    Disableable, Selectable, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    theme::{Theme, ThemeMode},
};

#[derive(Clone, Copy)]
pub enum Partition {
    Neutral,
    Primary,
    Selected,
}

pub fn partition_strip(palette: Palette) -> Div {
    div()
        .flex()
        .flex_wrap()
        .w_full()
        .gap(px(1.))
        .bg(palette.stroke)
}

pub fn partition(
    id: impl Into<ElementId>,
    palette: Palette,
    treatment: Partition,
    disabled: bool,
    cx: &App,
) -> Button {
    action_button(id, palette, treatment, disabled, cx)
        .h(px(42.))
        .flex_1()
        .min_w(px(120.))
}

pub fn compact_action(
    id: impl Into<ElementId>,
    palette: Palette,
    treatment: Partition,
    disabled: bool,
    cx: &App,
) -> Button {
    action_button(id, palette, treatment, disabled, cx)
        .h(px(32.))
        .when(
            !disabled && !matches!(treatment, Partition::Selected),
            |button| button.border_1().border_color(palette.stroke),
        )
}

/// Translucent glass button variant for controls sitting over the Home backdrop shelf.
pub fn glass_action(
    id: impl Into<ElementId>,
    palette: Palette,
    treatment: Partition,
    disabled: bool,
    cx: &App,
) -> Button {
    let primary = matches!(treatment, Partition::Primary);
    if primary {
        return compact_action(id, palette, treatment, disabled, cx);
    }
    let colors = if palette.dark {
        [0xffffff12, 0xffffff20, 0xffffff28]
    } else {
        [0xffffff7a, 0xffffffa8, 0xffffffc8]
    }
    .map(rgba);
    let (disabled_bg, disabled_border, border) = if palette.dark {
        (rgba(0xffffff0a), rgba(0xffffff18), rgba(0xffffff24))
    } else {
        (rgba(0x00000008), rgba(0x00000018), rgba(0x00000024))
    };
    custom_button(
        id,
        palette,
        colors,
        disabled_bg,
        disabled_border,
        treatment,
        disabled,
        cx,
    )
    .h(px(32.))
    .when(
        !disabled && !matches!(treatment, Partition::Selected),
        |button| button.border_1().border_color(border),
    )
}

/// The selected instance's primary action. The hit area stays still as its face depresses.
pub fn launch_action(label: &'static str, palette: Palette, disabled: bool, cx: &App) -> Button {
    let ink = action_ink(palette, Partition::Primary, disabled);
    let face = if disabled {
        palette.layer
    } else if palette.dark {
        rgb(0xe0e1dc)
    } else {
        rgb(0x292b2c)
    };
    let lip = if disabled {
        palette.stroke
    } else if palette.dark {
        rgb(0x969995)
    } else {
        rgb(0x151617)
    };
    let plate = div()
        .id("launch-plate")
        .relative()
        .min_w(px(82.))
        .px_3()
        .h(px(29.))
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .bg(face)
        .text_color(ink)
        .when(disabled, |d| d.border_1().border_color(palette.stroke))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .when(!disabled, |d| {
            d.hover(|s| {
                s.bg(if palette.dark {
                    rgb(0xf0f0ec)
                } else {
                    rgb(0x383a3b)
                })
            })
            .active(|s| {
                s.top(px(2.)).bg(if palette.dark {
                    rgb(0xcacbc7)
                } else {
                    rgb(0x202223)
                })
            })
        })
        .child(crate::icons::Icon::Play.render(px(16.), ink))
        .child(label);
    action_button("play", palette, Partition::Primary, disabled, cx)
        .p_0()
        .border_0()
        .bg(palette.layer)
        .h(px(32.))
        .child(
            div()
                .id("launch-surface")
                .relative()
                .h(px(32.))
                .bg(palette.layer)
                .child(
                    div()
                        .absolute()
                        .left(px(2.))
                        .right(px(2.))
                        .bottom_0()
                        .h(px(4.))
                        .bg(lip),
                )
                .child(plate),
        )
}

pub fn label(
    button: Button,
    text: impl Into<SharedString>,
    palette: Palette,
    treatment: Partition,
    disabled: bool,
) -> Button {
    let text = text.into();
    button.child(
        div()
            .text_color(action_ink(palette, treatment, disabled))
            .child(text),
    )
}

fn action_ink(palette: Palette, treatment: Partition, disabled: bool) -> Rgba {
    if disabled {
        return palette.muted;
    }
    match treatment {
        Partition::Primary if palette.dark => rgb(0x161616),
        Partition::Primary => rgb(0xffffff),
        _ => palette.ink,
    }
}

fn action_button(
    id: impl Into<ElementId>,
    palette: Palette,
    treatment: Partition,
    disabled: bool,
    cx: &App,
) -> Button {
    let primary = matches!(treatment, Partition::Primary);
    let colors = match (primary, palette.dark) {
        (true, true) => [0xd7d7d7, 0xeeeeee, 0xa8a8a8].map(rgb),
        (true, false) => [0x353535, 0x191919, 0x525252].map(rgb),
        (false, _) => [palette.layer, palette.selected, palette.selected],
    };
    custom_button(
        id,
        palette,
        colors,
        palette.layer,
        palette.stroke,
        treatment,
        disabled,
        cx,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "The shared builder binds button identity, state palette colors, and partition rules in one place."
)]
fn custom_button(
    id: impl Into<ElementId>,
    palette: Palette,
    colors: [Rgba; 3],
    disabled_bg: Rgba,
    disabled_border: Rgba,
    treatment: Partition,
    disabled: bool,
    cx: &App,
) -> Button {
    let selected = matches!(treatment, Partition::Selected);
    let variant = ButtonCustomVariant::new(cx)
        .color(colors[0].into())
        .foreground(action_ink(palette, treatment, disabled).into())
        .hover(colors[1].into())
        .active(colors[2].into());
    Button::new(id)
        .custom(variant)
        .selected(selected)
        .disabled(disabled)
        .small()
        .rounded(px(0.))
        .when(disabled, |button| {
            button
                .bg(disabled_bg)
                .text_color(palette.muted)
                .border_1()
                .border_color(disabled_border)
        })
        .when(selected && !disabled, |button| {
            button.border_0().border_b_2().border_color(palette.accent)
        })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub frame: Rgba,
    pub surface: Rgba,
    pub layer: Rgba,
    pub stroke: Rgba,
    pub control_line: Rgba,
    pub muted: Rgba,
    pub ink: Rgba,
    pub accent: Rgba,
    pub selected: Rgba,
    pub warning: Rgba,
}

impl Palette {
    pub fn read(preference: Preference, appearance: WindowAppearance) -> Self {
        let dark = match preference {
            Preference::Dark => true,
            Preference::Light => false,
            Preference::System => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        };
        let values = if dark {
            [
                0x202020, 0x262626, 0x2c2c2c, 0x424242, 0xbdbdbd, 0xf4f4f4, 0xd7d7d7, 0x404040,
                0xf4f4f4,
            ]
        } else {
            [
                0xf2f2f2, 0xffffff, 0xf8f8f8, 0xd2d2d2, 0x525252, 0x161616, 0x353535, 0xdedede,
                0x161616,
            ]
        }
        .map(rgb);
        let [
            frame,
            surface,
            layer,
            stroke,
            muted,
            ink,
            accent,
            selected,
            warning,
        ] = values;
        Self {
            dark,
            frame,
            surface,
            layer,
            stroke,
            control_line: rgb(if dark { 0x909090 } else { 0x777777 }),
            muted,
            ink,
            accent,
            selected,
            warning,
        }
    }

    pub fn apply(self, window: &mut Window, cx: &mut App) {
        let mode = if self.dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        Theme::change(mode, Some(window), cx);
        let theme = Theme::global_mut(cx);
        theme.colors.background = self.surface.into();
        theme.colors.foreground = self.ink.into();
        theme.colors.muted_foreground = self.muted.into();
        theme.colors.border = self.stroke.into();
        theme.colors.input = self.control_line.into();
        theme.colors.ring = if self.dark {
            rgb(0xf4f4f4)
        } else {
            rgb(0x171717)
        }
        .into();
        theme.colors.primary = if self.dark {
            rgb(0xd7d7d7)
        } else {
            rgb(0x353535)
        }
        .into();
        theme.colors.primary_foreground = action_ink(self, Partition::Primary, false).into();
        theme.colors.primary_hover = if self.dark {
            rgb(0xeeeeee)
        } else {
            rgb(0x191919)
        }
        .into();
        theme.colors.primary_active = if self.dark {
            rgb(0xa8a8a8)
        } else {
            rgb(0x525252)
        }
        .into();
        theme.radius = px(0.);
        theme.font_family = if cfg!(windows) {
            "Segoe UI".into()
        } else {
            ".SystemUIFont".into()
        };
    }

    pub fn ink(self) -> Hsla {
        self.ink.into()
    }
}
