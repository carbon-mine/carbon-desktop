// SPDX-License-Identifier: MPL-2.0
use super::{paint::Style, set, wide};
use crate::account::{Control, Dialog};
use std::{
    cell::{Cell, RefCell},
    ptr,
};
use tauri::WebviewWindow;
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::{GlobalFree, HWND, RECT},
    Graphics::Gdi::InvalidateRect,
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        LibraryLoader::GetModuleHandleW,
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        Ole::CF_UNICODETEXT,
    },
    UI::{
        Input::KeyboardAndMouse::{GetFocus, SetFocus},
        WindowsAndMessaging::*,
    },
};

const HEADING: i32 = 1300;
const MESSAGE: i32 = 1301;
const CODE: i32 = 1302;
const SAVED: i32 = 1303;
pub(super) const ACCOUNTS: i32 = 1304;
const BUTTONS: [i32; 5] = [IDOK, 1305, 1306, 1307, 1308];
const ALL_BUTTONS: [i32; 6] = [IDOK, IDCANCEL, 1305, 1306, 1307, 1308];
const TIMER: usize = 1;

struct State {
    account: RefCell<Dialog>,
    style: Cell<Style>,
    rows: RefCell<Vec<(Uuid, String)>>,
    controls: Cell<[Option<Control>; 5]>,
}

unsafe fn show(hwnd: HWND, id: i32, visible: bool) {
    let control = GetDlgItem(hwnd, id);
    if !visible && GetFocus() == control {
        SetFocus(GetDlgItem(hwnd, IDCANCEL));
    }
    ShowWindow(control, if visible { SW_SHOWNA } else { SW_HIDE });
}

unsafe fn render(hwnd: HWND, state: &State) {
    let account = state.account.borrow();
    set(hwnd, HEADING, account.heading());
    set(hwnd, MESSAGE, &account.message());
    show(hwnd, CODE, account.code().is_some());
    set(
        hwnd,
        CODE,
        &account.code().map_or_else(String::new, |code| {
            format!("Microsoft sign-in code: {code}")
        }),
    );
    let rows = account.rows();
    if rows != *state.rows.borrow() {
        SendDlgItemMessageW(hwnd, ACCOUNTS, CB_RESETCONTENT, 0, 0);
        for (_, text) in &rows {
            SendDlgItemMessageW(
                hwnd,
                ACCOUNTS,
                CB_ADDSTRING,
                0,
                wide(text).as_ptr() as isize,
            );
        }
        *state.rows.borrow_mut() = rows;
    }
    let rows = state.rows.borrow();
    show(hwnd, SAVED, !rows.is_empty());
    show(hwnd, ACCOUNTS, !rows.is_empty());
    let choice = rows
        .iter()
        .position(|(id, _)| Some(*id) == account.selected)
        .unwrap_or(usize::MAX);
    SendDlgItemMessageW(hwnd, ACCOUNTS, CB_SETCURSEL, choice, 0);
    let controls = account.controls().slots();
    state.controls.set(controls);
    for (id, control) in BUTTONS.into_iter().zip(controls) {
        if let Some(Control(label, _)) = control {
            set(hwnd, id, label);
        }
        show(hwnd, id, control.is_some());
    }
    set(
        hwnd,
        IDCANCEL,
        if account.confirming() {
            "Back"
        } else {
            "Close"
        },
    );
    let mut close = RECT {
        left: if controls[0].is_some() { 153 } else { 0 },
        top: 160,
        right: 306,
        bottom: 190,
    };
    MapDialogRect(hwnd, &mut close);
    MoveWindow(
        GetDlgItem(hwnd, IDCANCEL),
        close.left,
        close.top,
        close.right - close.left,
        close.bottom - close.top,
        1,
    );
    InvalidateRect(hwnd, ptr::null(), 1);
}

pub(super) unsafe fn copy_text(owner: HWND, code: &str) -> bool {
    let code = wide(code);
    let memory = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of_val(code.as_slice()));
    if memory.is_null() {
        return false;
    }
    let destination = GlobalLock(memory);
    if destination.is_null() {
        GlobalFree(memory);
        return false;
    }
    ptr::copy_nonoverlapping(code.as_ptr(), destination.cast(), code.len());
    GlobalUnlock(memory);
    let mut transferred = false;
    if OpenClipboard(owner) != 0 {
        transferred =
            EmptyClipboard() != 0 && !SetClipboardData(u32::from(CF_UNICODETEXT), memory).is_null();
        CloseClipboard();
    }
    if !transferred {
        GlobalFree(memory);
    }
    transferred
}

// DialogBoxParamW keeps State alive while the owner is modal. Reentrant paint messages read
// only Style, and input handlers release their RefCell mutation before repainting.
unsafe extern "system" fn dialog_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == WM_INITDIALOG {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam);
        let state = &*(lparam as *const State);
        state
            .style
            .get()
            .init(hwnd, &ALL_BUTTONS, &[], Some(ACCOUNTS), IDCANCEL);
        render(hwnd, state);
        if SetTimer(hwnd, TIMER, 250, None) == 0 {
            EndDialog(hwnd, -2);
        }
        SetFocus(GetDlgItem(hwnd, IDCANCEL));
        return 0;
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State;
    let Some(state) = pointer.as_ref() else {
        return 0;
    };
    if let Some(result) = state.style.get().message(hwnd, msg, wparam, lparam) {
        return result;
    }
    match msg {
        WM_TIMER if wparam == TIMER => {
            let changed = state.account.borrow_mut().refresh();
            if changed {
                render(hwnd, state);
            }
        }
        WM_SETTINGCHANGE => {
            let style = Style::new(state.style.get().dark);
            state.style.set(style);
            style.init(hwnd, &ALL_BUTTONS, &[], Some(ACCOUNTS), IDCANCEL);
            InvalidateRect(hwnd, ptr::null(), 1);
        }
        WM_CLOSE | WM_COMMAND if msg == WM_CLOSE || wparam & 0xffff == IDCANCEL as usize => {
            let dismiss = state.account.borrow_mut().dismiss();
            if dismiss {
                EndDialog(hwnd, IDCANCEL as isize);
            } else {
                render(hwnd, state);
            }
        }
        WM_COMMAND
            if wparam & 0xffff == ACCOUNTS as usize && (wparam >> 16) as u32 == CBN_SELCHANGE =>
        {
            let index = SendDlgItemMessageW(hwnd, ACCOUNTS, CB_GETCURSEL, 0, 0);
            let selected = state.rows.borrow().get(index as usize).map(|row| row.0);
            if let Some(id) = selected {
                state.account.borrow_mut().select(id);
                render(hwnd, state);
            }
        }
        WM_COMMAND if (wparam >> 16) as u32 == BN_CLICKED => {
            let control = BUTTONS
                .iter()
                .position(|id| *id as usize == wparam & 0xffff)
                .and_then(|index| state.controls.get()[index]);
            if let Some(Control(_, action)) = control {
                state
                    .account
                    .borrow_mut()
                    .perform(action, |code| copy_text(hwnd, code));
                render(hwnd, state);
            } else {
                return 0;
            }
        }
        WM_NCDESTROY => {
            KillTimer(hwnd, TIMER);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            return 0;
        }
        _ => return 0,
    }
    1
}

pub(in crate::dialogs) fn show_accounts(
    parent: &WebviewWindow,
    account: Dialog,
    dark: bool,
) -> Result<(), String> {
    let owner = parent
        .hwnd()
        .map_err(|_| "The owner window is unavailable.")?;
    let state = State {
        account: RefCell::new(account),
        style: Cell::new(Style::new(dark)),
        rows: RefCell::new(Vec::new()),
        controls: Cell::new([None; 5]),
    };
    let result = unsafe {
        DialogBoxParamW(
            GetModuleHandleW(ptr::null()),
            2104 as *const u16,
            owner.0,
            Some(dialog_proc),
            std::ptr::from_ref(&state) as isize,
        )
    };
    if result < 0 {
        return Err("Windows could not open the native account window.".into());
    }
    Ok(())
}
