use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Emitter, Manager, Wry,
};

pub struct AppearanceItems(pub [CheckMenuItem<Wry>; 3]);

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let new = MenuItem::with_id(
        app,
        "new-profile",
        "New vanilla instance…",
        true,
        Some("CmdOrCtrl+N"),
    )?;
    let import = MenuItem::with_id(
        app,
        "import-pack",
        "Import pack…",
        true,
        Some("CmdOrCtrl+O"),
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let close = MenuItem::with_id(app, "close-main", "Close window", true, Some("CmdOrCtrl+W"))?;
    let file = Submenu::with_items(app, "&File", true, &[&new, &import, &separator, &close])?;
    let find = MenuItem::with_id(
        app,
        "focus-search",
        "Find instance",
        true,
        Some("CmdOrCtrl+F"),
    )?;
    let activity = MenuItem::with_id(app, "show-activity", "View activity", true, Some("F6"))?;
    let choices = [
        CheckMenuItem::with_id(
            app,
            "theme-system",
            "Use system setting",
            true,
            true,
            None::<&str>,
        )?,
        CheckMenuItem::with_id(app, "theme-light", "Light", true, false, None::<&str>)?,
        CheckMenuItem::with_id(app, "theme-dark", "Dark", true, false, None::<&str>)?,
    ];
    let appearance = Submenu::with_items(
        app,
        "Appearance",
        true,
        &[&choices[0], &choices[1], &choices[2]],
    )?;
    app.manage(AppearanceItems(choices));
    let view = Submenu::with_items(app, "&View", true, &[&find, &activity, &appearance])?;
    Menu::with_items(app, &[&file, &view])
}

pub fn dispatch(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    match event.id().as_ref() {
        "close-main" => {
            let _ = window.close();
        }
        action @ ("new-profile" | "import-pack" | "focus-search" | "show-activity"
        | "theme-system" | "theme-light" | "theme-dark") => {
            let _ = window.emit("desktop-menu", action);
        }
        _ => (),
    }
}
