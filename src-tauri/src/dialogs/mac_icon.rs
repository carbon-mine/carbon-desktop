// SPDX-License-Identifier: MPL-2.0
use carbon_profiles::icons::{catalog, IconDraft, InstanceIcon};
use objc2::{rc::Retained, MainThreadMarker};
use objc2_app_kit::{
    NSAccessibility, NSAlert, NSAlertFirstButtonReturn, NSAppearance, NSAppearanceCustomization,
    NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSCellImagePosition, NSImage, NSPopUpButton,
    NSWindow, NSWindowOrderingMode,
};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize, NSString};
use tauri::WebviewWindow;

fn template(icon: &InstanceIcon, mtm: MainThreadMarker) -> Result<Retained<NSImage>, String> {
    // Encode the small shared mask in memory. AppKit tints template images for its appearance.
    let mut pixels = Vec::with_capacity(12 * 12 * 2);
    for row in icon.rows {
        for x in 0..12 {
            pixels.extend_from_slice(&[0, if row & (1 << (11 - x)) == 0 { 0 } else { 255 }]);
        }
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 12, 12);
        encoder.set_color(png::ColorType::GrayscaleAlpha);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| "Could not render the icon.")?;
        writer
            .write_image_data(&pixels)
            .map_err(|_| "Could not render the icon.")?;
    }
    let image = NSImage::initWithData(mtm.alloc(), &NSData::with_bytes(&bytes))
        .ok_or("Could not display the icon.")?;
    image.setTemplate(true);
    image.setSize(NSSize::new(20., 20.));
    Ok(image)
}

pub(super) fn show_icon(
    owner: &WebviewWindow,
    original: &IconDraft,
    dark: bool,
) -> Result<Option<IconDraft>, String> {
    let mtm = MainThreadMarker::new().ok_or("The icon picker needs the main window thread.")?;
    let handle = owner
        .ns_window()
        .map_err(|_| "The owner window is unavailable.")?;
    // SAFETY: Tauri supplies the NSWindow and retains it for this main-thread callback.
    let parent =
        unsafe { handle.cast::<NSWindow>().as_ref() }.ok_or("The owner window is unavailable.")?;
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str("Choose instance icon"));
    let popup = NSPopUpButton::initWithFrame_pullsDown(
        mtm.alloc(),
        NSRect::new(NSPoint::new(0., 0.), NSSize::new(250., 32.)),
        false,
    );
    popup.setAccessibilityLabel(Some(&NSString::from_str("Instance icon")));
    popup.setImagePosition(NSCellImagePosition::ImageLeft);
    let mut automatic = original.clone();
    automatic.choose(0);
    for (i, (name, icon)) in std::iter::once(("Use default", automatic.preview()))
        .chain(catalog().iter().map(|icon| (icon.label.as_str(), icon)))
        .enumerate()
    {
        popup.addItemWithTitle(&NSString::from_str(name));
        if let Some(item) = popup.itemAtIndex(i as isize) {
            item.setImage(Some(&template(icon, mtm)?));
        }
    }
    let initial = if original.unavailable_choice() {
        popup.addItemWithTitle(&NSString::from_str("Unavailable icon (using default)"));
        catalog().len() + 1
    } else {
        original.selected_index()
    };
    popup.selectItemAtIndex(initial as isize);
    alert.setAccessoryView(Some(&popup));
    alert.addButtonWithTitle(&NSString::from_str("Save icon"));
    let cancel = alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let window = alert.window();
    // SAFETY: Built-in AppKit appearance names are process-lifetime constants.
    let name = unsafe {
        if dark {
            NSAppearanceNameDarkAqua
        } else {
            NSAppearanceNameAqua
        }
    };
    window.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
    window.setInitialFirstResponder(Some(&popup));
    // SAFETY: Both distinct NSWindows belong to this application and are live on the main thread.
    unsafe { parent.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };
    let response = alert.runModal();
    parent.removeChildWindow(&window);
    let selected = usize::try_from(popup.indexOfSelectedItem()).ok();
    let mut draft = original.clone();
    if response == NSAlertFirstButtonReturn {
        if let Some(index) = selected.filter(|i| *i <= catalog().len()) {
            draft.choose(index);
        }
    }
    Ok(draft.changed().then_some(draft))
}
