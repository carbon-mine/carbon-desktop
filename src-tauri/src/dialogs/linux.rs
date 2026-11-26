use super::{Form, Mode, ProfileAction, VanillaVersion};
use gtk::{
    prelude::*, ComboBoxText, Dialog, DialogFlags, Entry, Grid, Label, ResponseType, SpinButton,
};
use tauri::WebviewWindow;

// The provider is installed only while this native dialog exists. All selectors are scoped.
struct DialogStyle {
    screen: gtk::gdk::Screen,
    provider: gtk::CssProvider,
}
impl Drop for DialogStyle {
    fn drop(&mut self) {
        gtk::StyleContext::remove_provider_for_screen(&self.screen, &self.provider);
    }
}

fn style(dialog: &Dialog, dark: bool) -> Option<DialogStyle> {
    let context = dialog.style_context();
    context.add_class("carbon-dialog");
    context.add_class(if dark { "carbon-dark" } else { "carbon-light" });
    if let Some(button) = dialog.widget_for_response(ResponseType::Accept) {
        button.style_context().add_class("suggested-action");
        if let Some(row) = button
            .parent()
            .and_then(|p| p.downcast::<gtk::ButtonBox>().ok())
        {
            row.set_layout_style(gtk::ButtonBoxStyle::Expand);
            row.set_homogeneous(true);
            row.set_spacing(0);
        }
    }
    let settings = dialog.settings()?;
    if settings.gtk_theme_name().is_some_and(|theme| {
        theme
            .to_ascii_lowercase()
            .replace('-', "")
            .contains("highcontrast")
    }) {
        return None;
    }
    let screen = gtk::prelude::WidgetExt::screen(dialog)?;
    let provider = gtk::CssProvider::new();
    if provider.load_from_data(include_bytes!("gtk.css")).is_err() {
        return None;
    }
    gtk::StyleContext::add_provider_for_screen(
        &screen,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    Some(DialogStyle { screen, provider })
}

pub(super) fn message_dialog(
    parent: &WebviewWindow,
    title: &str,
    description: &str,
    accept: Option<&str>,
    dark: bool,
) -> Result<bool, String> {
    let owner = parent
        .gtk_window()
        .map_err(|_| "The owner window is unavailable.")?;
    let actions = match accept {
        Some(label) => vec![
            ("Cancel", ResponseType::Cancel),
            (label, ResponseType::Accept),
        ],
        None => vec![("Close", ResponseType::Accept)],
    };
    let dialog = Dialog::with_buttons(
        Some(title),
        Some(&owner),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &actions,
    );
    dialog.set_default_response(ResponseType::Accept);
    dialog.set_resizable(false);
    let message = Label::new(Some(description));
    message.set_xalign(0.0);
    message.set_selectable(true);
    message.set_line_wrap(true);
    message.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    message.set_width_chars(44);
    message.set_max_width_chars(54);
    message.set_margin_start(20);
    message.set_margin_end(20);
    message.set_margin_top(18);
    message.set_margin_bottom(12);
    if accept.is_some() {
        let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(210);
        scroll.add(&message);
        dialog.content_area().add(&scroll);
    } else {
        dialog.content_area().add(&message);
    }
    let _style = style(&dialog, dark);
    dialog.show_all();
    if let Some(close) = dialog.widget_for_response(ResponseType::Accept) {
        close.grab_focus();
    }
    let result = dialog.run();
    dialog.close();
    Ok(result == ResponseType::Accept)
}

pub(super) fn show(
    parent: &WebviewWindow,
    form: &Form,
    dark: bool,
) -> Result<Option<ProfileAction>, String> {
    let owner = parent
        .gtk_window()
        .map_err(|_| "The owner window is unavailable.")?;
    let dialog = Dialog::with_buttons(
        Some(form.mode.title()),
        Some(&owner),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Cancel", ResponseType::Cancel),
            (form.mode.accept(), ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    dialog.set_resizable(false);
    let grid = Grid::builder()
        .margin(20)
        .row_spacing(10)
        .column_spacing(16)
        .build();
    let name = Entry::builder()
        .text(&form.initial.name)
        .activates_default(true)
        .width_chars(30)
        .build();
    let row = |label: &str, control: &gtk::Widget, index: i32| {
        let title = Label::with_mnemonic(label);
        title.set_xalign(0.);
        title.set_mnemonic_widget(Some(control));
        grid.attach(&title, 0, index, 1, 1);
        grid.attach(control, 1, index, 1, 1);
    };
    row("Instance _name", name.upcast_ref(), 0);
    let versions = ComboBoxText::new();
    for v in VanillaVersion::ALL {
        versions.append(Some(v.id()), v.id());
    }
    versions.set_active_id(Some(form.initial.version.id()));
    versions.set_sensitive(form.initial.imported_pack.is_none());
    let numbers = [
        SpinButton::with_range(512., 16384., 1.),
        SpinButton::with_range(320., 7680., 1.),
        SpinButton::with_range(240., 4320., 1.),
    ];
    if form.mode != Mode::Clone {
        row("Minecraft _version", versions.upcast_ref(), 1);
        for (i, (label, n)) in [
            ("Maximum _memory (MiB)", form.initial.memory_mib),
            ("Window _width (px)", form.initial.window_width),
            ("Window _height (px)", form.initial.window_height),
        ]
        .iter()
        .enumerate()
        {
            numbers[i].set_value(f64::from(*n));
            numbers[i].set_numeric(true);
            numbers[i].set_activates_default(true);
            row(label, numbers[i].upcast_ref(), i as i32 + 2);
        }
    }
    let error = Label::new(None);
    error.set_line_wrap(true);
    error.set_lines(3);
    error.set_height_request(54);
    error.set_max_width_chars(40);
    error.style_context().add_class("validation");
    error.set_xalign(0.0);
    grid.attach(&error, 0, 5, 2, 1);
    dialog.content_area().add(&grid);
    let _style = style(&dialog, dark);
    dialog.show_all();
    name.grab_focus();
    let response = loop {
        if dialog.run() != ResponseType::Accept {
            break None;
        }
        let selected = versions.active_id();
        let version = VanillaVersion::ALL
            .into_iter()
            .find(|v| Some(v.id()) == selected.as_deref())
            .unwrap_or(form.initial.version);
        let values = if form.mode == Mode::Clone {
            form.numbers()
        } else {
            numbers.each_ref().map(|n| {
                n.update();
                n.value_as_int().to_string()
            })
        };
        match form.submit(name.text().to_string(), version, values) {
            Ok(action) => break Some(action),
            Err(reason) => {
                error.set_text(&reason);
                error.show();
            }
        }
    };
    dialog.close();
    Ok(response)
}
