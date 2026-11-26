// SPDX-License-Identifier: MPL-2.0
use super::{paint::Style, set, wide};
use carbon_profiles::icons::{catalog, IconDraft};
use std::ptr;
use tauri::WebviewWindow;
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        FillRect, GetStockObject, InvalidateRect, RestoreDC, SaveDC, SetDCBrushColor, DC_BRUSH, HDC,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*, WindowsAndMessaging::*},
};

const DIALOG: usize = 2112;
const PREVIEW: i32 = 1500;
pub(super) const CHOICE: i32 = 1501;
const RESET: i32 = 1502;

struct State {
    draft: IconDraft,
    style: Style,
    response: Option<IconDraft>,
}

pub(in crate::dialogs) fn show_icon(
    owner: &WebviewWindow,
    original: &IconDraft,
    dark: bool,
) -> Result<Option<IconDraft>, String> {
    let owner = owner
        .hwnd()
        .map_err(|_| "The owner window is unavailable.")?;

    // The dialog procedure only borrows this stack-owned state.
    let mut state = State {
        draft: original.clone(),
        style: Style::new(dark),
        response: None,
    };

    let result = present(owner.0, &mut state)?;
    if result == IDOK as isize {
        Ok(state.response)
    } else {
        Ok(None)
    }
}

fn present(owner: HWND, state: &mut State) -> Result<isize, String> {
    let result = unsafe {
        DialogBoxParamW(
            GetModuleHandleW(ptr::null()),
            DIALOG as *const u16,
            owner,
            Some(dialog_proc),
            std::ptr::from_mut(state) as isize,
        )
    };

    if result == -1 {
        Err("Windows could not create the native dialog.".into())
    } else {
        Ok(result)
    }
}

unsafe extern "system" fn dialog_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;

    if !state.is_null() {
        if msg == WM_SETTINGCHANGE {
            let style = Style::new((*state).style.dark);
            (*state).style = style;
            style.init(hwnd, &[RESET, IDOK, IDCANCEL], &[], Some(CHOICE), IDOK);
            InvalidateRect(hwnd, ptr::null(), 1);
            repaint_preview(hwnd);
        }

        if let Some(result) = (*state).style.message(hwnd, msg, wparam, lparam) {
            return result;
        }
    }

    match msg {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam);
            let state = &mut *(lparam as *mut State);
            state
                .style
                .init(hwnd, &[RESET, IDOK, IDCANCEL], &[], Some(CHOICE), IDOK);

            let automatic = wide("Automatic");
            SendDlgItemMessageW(hwnd, CHOICE, CB_ADDSTRING, 0, automatic.as_ptr() as isize);

            for icon in catalog() {
                let label = wide(&icon.label);
                SendDlgItemMessageW(hwnd, CHOICE, CB_ADDSTRING, 0, label.as_ptr() as isize);
            }

            let selected = state.draft.selected_index();
            SendDlgItemMessageW(hwnd, CHOICE, CB_SETCURSEL, selected, 0);

            set(hwnd, 1503, &state.draft.preview().label);
            repaint_preview(hwnd);
            1
        }

        WM_COMMAND => {
            if state.is_null() {
                return 0;
            }

            let state = &mut *state;
            let id = (wparam & 0xffff) as i32;
            let code = (wparam >> 16) as u32;

            match id {
                IDCANCEL => {
                    EndDialog(hwnd, IDCANCEL as isize);
                    1
                }

                IDOK if code == BN_CLICKED => {
                    // Save is never disabled. An unchanged draft, including an unknown
                    // future raw ID, is reported as no change.
                    if state.draft.changed() {
                        state.response = Some(state.draft.clone());
                    }
                    EndDialog(hwnd, IDOK as isize);
                    1
                }

                RESET if code == BN_CLICKED => {
                    state.draft.choose(0);
                    set(hwnd, 1503, &state.draft.preview().label);
                    SendDlgItemMessageW(hwnd, CHOICE, CB_SETCURSEL, 0, 0);
                    repaint_preview(hwnd);
                    1
                }

                CHOICE if code == CBN_SELCHANGE => {
                    let selected = SendDlgItemMessageW(hwnd, CHOICE, CB_GETCURSEL, 0, 0);
                    if selected >= 0 && selected as usize <= catalog().len() {
                        state.draft.choose(selected as usize);
                        set(hwnd, 1503, &state.draft.preview().label);
                        repaint_preview(hwnd);
                    }
                    1
                }

                _ => 0,
            }
        }

        WM_DRAWITEM if !state.is_null() && lparam != 0 => {
            let item = &*(lparam as *const DRAWITEMSTRUCT);
            if item.CtlID == PREVIEW as u32 && item.CtlType == ODT_STATIC {
                let rows = (*state).draft.preview().rows;
                draw_preview(item, &rows, (*state).style);
                1
            } else {
                0
            }
        }

        WM_SIZE => {
            if !state.is_null() {
                repaint_preview(hwnd);
            }
            0
        }

        WM_CLOSE => {
            EndDialog(hwnd, IDCANCEL as isize);
            1
        }

        _ => 0,
    }
}

unsafe fn repaint_preview(hwnd: HWND) {
    let preview = GetDlgItem(hwnd, PREVIEW);
    if !preview.is_null() {
        InvalidateRect(preview, ptr::null(), 1);
    }
}

unsafe fn fill(dc: HDC, rect: &RECT, color: u32) {
    SetDCBrushColor(dc, color);
    FillRect(dc, rect, GetStockObject(DC_BRUSH));
}

unsafe fn draw_preview(item: &DRAWITEMSTRUCT, rows: &[u16; 12], style: Style) {
    let dc = item.hDC;
    let area = item.rcItem;
    let width = area.right - area.left;
    let height = area.bottom - area.top;
    let cell = width.min(height) / 16;
    let left = area.left + (width - cell * 12) / 2;
    let top = area.top + (height - cell * 12) / 2;
    let (background, ink) = style.preview_colors();
    let saved = SaveDC(dc);
    fill(dc, &area, background);
    for (y, bits) in rows.iter().enumerate() {
        for x in 0..12 {
            if bits & (1 << (11 - x)) != 0 {
                let left = left + x * cell;
                let top = top + y as i32 * cell;
                fill(
                    dc,
                    &RECT {
                        left,
                        top,
                        right: left + cell,
                        bottom: top + cell,
                    },
                    ink,
                );
            }
        }
    }
    if saved != 0 {
        RestoreDC(dc, saved);
    }
}
