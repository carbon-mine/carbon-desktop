use super::{Form, Mode, ProfileAction, ServerAddress, VanillaVersion};
use crate::account::{Control, Dialog as AccountDialog};
use gtk::{
    prelude::*, ComboBoxText, Dialog, DialogFlags, Entry, Grid, Label, ResponseType, SpinButton,
};

pub(super) fn edit_server(
    owner: &tauri::WebviewWindow,
    address: Option<&ServerAddress>,
    dark: bool,
) -> Result<Option<Option<ServerAddress>>, String> {
    let native = owner
        .gtk_window()
        .map_err(|_| "The owner window is unavailable.")?;
    let dialog = Dialog::with_buttons(
        Some("Edit server address"),
        Some(&native),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[
            ("Cancel", ResponseType::Cancel),
            ("Save changes", ResponseType::Accept),
        ],
    );
    dialog.set_default_response(ResponseType::Accept);
    dialog.set_default_size(430, -1);
    let input = Entry::builder()
        .text(address.map_or("", ServerAddress::as_str))
        .activates_default(true)
        .max_length(512)
        .build();
    let label = Label::with_mnemonic("Server _address");
    label.set_mnemonic_widget(Some(&input));
    let help = Label::new(Some(
        "Example: play.example.org:25565. Clear the field to remove the binding.",
    ));
    let error = Label::new(None);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_border_width(16);
    for description in [&label, &help, &error] {
        description.set_xalign(0.0);
        description.set_line_wrap(true);
    }
    error.style_context().add_class("error");
    content.add(&label);
    content.add(&input);
    content.add(&help);
    content.add(&error);
    dialog.content_area().add(&content);
    let _style = style(&dialog, dark);
    dialog.show_all();
    error.hide();
    input.grab_focus();
    let value = loop {
        if dialog.run() != ResponseType::Accept {
            break None;
        }
        match ServerAddress::parse_optional(input.text().as_str()) {
            Ok(value) => break Some(value),
            Err(reason) => {
                error.set_text(&reason);
                error.show();
                input.grab_focus();
            }
        }
    };
    dialog.close();
    Ok(value)
}
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
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
    let accept = dialog.widget_for_response(ResponseType::Accept);
    if let Some(button) = &accept {
        button.style_context().add_class("suggested-action");
    }
    if let Some(row) = accept
        .or_else(|| dialog.widget_for_response(ResponseType::Close))
        .and_then(|button| button.parent())
        .and_then(|p| p.downcast::<gtk::ButtonBox>().ok())
    {
        row.set_layout_style(gtk::ButtonBoxStyle::Expand);
        row.set_homogeneous(true);
        row.set_spacing(0);
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

struct AccountWidgets {
    heading: Label,
    message: Label,
    code: Label,
    saved: Label,
    accounts: ComboBoxText,
    buttons: [gtk::Button; 5],
    close: gtk::Button,
    rows: RefCell<Vec<(uuid::Uuid, String)>>,
    rendering: Cell<bool>,
}

impl AccountWidgets {
    fn render(&self, account: &AccountDialog) {
        self.rendering.set(true);
        self.heading.set_text(account.heading());
        self.message.set_text(&account.message());
        let code = account.code();
        self.code.set_visible(code.is_some());
        self.code.set_text(&code.map_or_else(String::new, |code| {
            format!("Microsoft sign-in code: {code}")
        }));
        let rows = account.rows();
        if rows != *self.rows.borrow() {
            self.accounts.remove_all();
            for (id, text) in &rows {
                self.accounts.append(Some(&id.to_string()), text);
            }
            *self.rows.borrow_mut() = rows;
        }
        let saved_accounts = !self.rows.borrow().is_empty();
        self.saved.set_visible(saved_accounts);
        self.accounts.set_visible(saved_accounts);
        self.accounts
            .set_active_id(account.selected.map(|id| id.to_string()).as_deref());
        self.close.set_label(if account.confirming() {
            "Back"
        } else {
            "Close"
        });
        for (button, control) in self.buttons.iter().zip(account.controls().slots()) {
            if let Some(Control(label, _)) = control {
                button.set_label(label);
                button.show();
            } else {
                if button.has_focus() {
                    self.close.grab_focus();
                }
                button.hide();
            }
        }
        self.rendering.set(false);
    }
}

pub(super) fn show_accounts(
    parent: &WebviewWindow,
    account: AccountDialog,
    dark: bool,
) -> Result<(), String> {
    let owner = parent
        .gtk_window()
        .map_err(|_| "The owner window is unavailable.")?;
    let dialog = Dialog::with_buttons(
        Some("Minecraft accounts"),
        Some(&owner),
        DialogFlags::MODAL | DialogFlags::DESTROY_WITH_PARENT,
        &[],
    );
    dialog.set_default_size(490, -1);
    dialog.set_resizable(false);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_start(20);
    content.set_margin_end(20);
    content.set_margin_top(18);
    content.set_margin_bottom(14);
    let heading = Label::new(None);
    heading.set_xalign(0.);
    heading.style_context().add_class("account-heading");
    let message = Label::new(None);
    message.set_xalign(0.);
    message.set_line_wrap(true);
    message.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    message.set_max_width_chars(50);
    let accounts = ComboBoxText::new();
    let saved = Label::with_mnemonic("Saved _accounts");
    saved.set_xalign(0.);
    saved.set_mnemonic_widget(Some(&accounts));
    let code = Label::new(None);
    code.set_xalign(0.);
    code.set_selectable(true);
    content.add(&heading);
    content.add(&message);
    content.add(&code);
    content.add(&saved);
    content.add(&accounts);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.add(&row);
    dialog.content_area().add(&content);
    let buttons = std::array::from_fn(|index| {
        let button = gtk::Button::new();
        let response = ResponseType::Other(index as u16 + 1);
        if index == 0 {
            dialog.add_action_widget(&button, response);
            button.style_context().add_class("suggested-action");
        } else {
            row.add(&button);
            let dialog = dialog.downgrade();
            button.connect_clicked(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.response(response);
                }
            });
        }
        button
    });
    let close = gtk::Button::with_label("Close");
    close.set_can_default(true);
    dialog.add_action_widget(&close, ResponseType::Close);
    dialog.set_default_response(ResponseType::Close);
    let _style = style(&dialog, dark);
    let widgets = Rc::new(AccountWidgets {
        heading,
        message,
        code,
        saved,
        accounts,
        buttons,
        close,
        rows: RefCell::new(Vec::new()),
        rendering: Cell::new(false),
    });
    dialog.show_all();
    let account = Rc::new(RefCell::new(account));
    widgets.render(&account.borrow());
    widgets.close.grab_focus();
    widgets.accounts.connect_changed({
        let widgets = Rc::downgrade(&widgets);
        let account = Rc::clone(&account);
        move |combo| {
            let Some(widgets) = widgets.upgrade() else {
                return;
            };
            if !widgets.rendering.get() {
                if let Some(id) = combo.active_id().and_then(|id| id.parse().ok()) {
                    let mut account = account.borrow_mut();
                    account.select(id);
                    widgets.render(&account);
                }
            }
        }
    });

    // GtkDialog runs the nested main loop. Only an open window polls native account state.
    let timer = gtk::glib::timeout_add_local(Duration::from_millis(250), {
        let account = Rc::clone(&account);
        let widgets = Rc::clone(&widgets);
        move || {
            let mut account = account.borrow_mut();
            if account.refresh() {
                widgets.render(&account);
            }
            gtk::glib::ControlFlow::Continue
        }
    });
    loop {
        let response = dialog.run();
        let mut account = account.borrow_mut();
        match response {
            ResponseType::Other(n @ 1..=5) => {
                if let Some(Control(_, action)) = account.controls().slots()[usize::from(n - 1)] {
                    account.perform(action, |code| {
                        gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).set_text(code);
                        true
                    });
                }
            }
            ResponseType::None => break,
            _ if account.dismiss() => break,
            _ => {}
        }
        widgets.render(&account);
    }
    timer.remove();
    dialog.close();
    Ok(())
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
