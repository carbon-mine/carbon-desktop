use super::{Form, Mode, ProfileAction, VanillaVersion};
use std::ptr;
use tauri::WebviewWindow;
use windows_sys::Win32::{
    Foundation::HWND, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*,
};

#[path = "windows_style.rs"]
mod paint;
use paint::Style;

const NAME: i32 = 1101;
const VERSION: i32 = 1102;
const NUMBERS: [i32; 3] = [1103, 1104, 1105];

struct State<'a> {
    body: Body<'a>,
    style: Style,
    response: Option<ProfileAction>,
}

enum Body<'a> {
    Form(&'a Form),
    Notice {
        title: &'a str,
        text: &'a str,
        accept: Option<&'a str>,
    },
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn set(hwnd: HWND, id: i32, s: &str) {
    unsafe {
        SetDlgItemTextW(hwnd, id, wide(s).as_ptr());
    }
}

fn read(hwnd: HWND, id: i32) -> Result<String, String> {
    let field = unsafe { GetDlgItem(hwnd, id) };
    let len = unsafe { GetWindowTextLengthW(field) };
    if len > 512 {
        return Err("The entered value is too long.".into());
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = unsafe { GetDlgItemTextW(hwnd, id, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16(&buf[..n as usize]).map_err(|_| "Enter valid text for this value.".into())
}

fn action(hwnd: HWND, form: &Form) -> Result<ProfileAction, String> {
    let name = read(hwnd, NAME)?;
    if form.mode == Mode::Clone {
        return form.submit(name, form.initial.version, form.numbers());
    }
    let index = unsafe { SendDlgItemMessageW(hwnd, VERSION, CB_GETCURSEL, 0, 0) };
    let version = VanillaVersion::ALL
        .get(index as usize)
        .copied()
        .ok_or("Select a Minecraft version.")?;
    form.submit(
        name,
        version,
        [
            read(hwnd, NUMBERS[0])?,
            read(hwnd, NUMBERS[1])?,
            read(hwnd, NUMBERS[2])?,
        ],
    )
}

// DialogBoxParamW keeps the stack-owned State alive until EndDialog returns.
// Only this window's procedure reads its user data, on its owning UI thread.
unsafe extern "system" fn dialog_proc(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if !pointer.is_null() {
        if msg == WM_SETTINGCHANGE {
            let updated = Style::new((*pointer).style.dark);
            (*pointer).style = updated;
            updated.init(hwnd);
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, ptr::null(), 1);
        }
        if let Some(result) = (*pointer).style.message(hwnd, msg, wparam, lparam) {
            return result;
        }
    }
    match msg {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam);
            let state = &*(lparam as *const State);
            state.style.init(hwnd);
            let form = match state.body {
                Body::Form(form) => form,
                Body::Notice {
                    title,
                    text,
                    accept,
                } => {
                    SetWindowTextW(hwnd, wide(title).as_ptr());
                    set(hwnd, 1200, &text.replace('\n', "\r\n"));
                    if let Some(label) = accept {
                        set(hwnd, IDOK, label);
                    }
                    return 1;
                }
            };
            SetWindowTextW(hwnd, wide(form.mode.title()).as_ptr());
            set(hwnd, IDOK, form.mode.accept());
            set(hwnd, NAME, &form.initial.name);
            if form.mode != Mode::Clone {
                for (i, v) in VanillaVersion::ALL.iter().enumerate() {
                    SendDlgItemMessageW(
                        hwnd,
                        VERSION,
                        CB_ADDSTRING,
                        0,
                        wide(v.id()).as_ptr() as isize,
                    );
                    if *v == form.initial.version {
                        SendDlgItemMessageW(hwnd, VERSION, CB_SETCURSEL, i, 0);
                    }
                }
                if form.initial.imported_pack.is_some() {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(
                        GetDlgItem(hwnd, VERSION),
                        0,
                    );
                }
                for (id, num) in NUMBERS.into_iter().zip(form.numbers()) {
                    set(hwnd, id, &num);
                }
            }
            1
        }
        WM_COMMAND if (wparam & 0xffff) == IDCANCEL as usize => {
            EndDialog(hwnd, IDCANCEL as isize);
            1
        }
        WM_CLOSE => {
            EndDialog(hwnd, IDCANCEL as isize);
            1
        }
        WM_COMMAND if (wparam & 0xffff) == IDOK as usize => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if state.is_null() {
                return 0;
            }
            let Body::Form(form) = (*state).body else {
                EndDialog(hwnd, IDOK as isize);
                return 1;
            };
            match action(hwnd, form) {
                Ok(value) => {
                    (*state).response = Some(value);
                    EndDialog(hwnd, IDOK as isize);
                }
                Err(error) => {
                    let mut child = State {
                        body: Body::Notice {
                            title: "Check instance settings",
                            text: &error,
                            accept: None,
                        },
                        style: (*state).style,
                        response: None,
                    };
                    // The editor stays open with its entered values while the warning is visible.
                    if present(hwnd, 2103, &mut child).is_err() {
                        MessageBoxW(
                            hwnd,
                            wide(&error).as_ptr(),
                            wide("Check instance settings").as_ptr(),
                            MB_OK | MB_ICONWARNING,
                        );
                    }
                }
            }
            1
        }
        _ => 0,
    }
}

fn present(owner: HWND, resource: usize, state: &mut State) -> Result<isize, String> {
    let result = unsafe {
        DialogBoxParamW(
            GetModuleHandleW(ptr::null()),
            resource as *const u16,
            owner,
            Some(dialog_proc),
            std::ptr::from_mut(state) as isize,
        )
    };
    if result == -1 {
        return Err("Windows could not create the native dialog.".into());
    }
    Ok(result)
}

pub(super) fn notice(
    parent: &WebviewWindow,
    title: &str,
    text: &str,
    dark: bool,
) -> Result<(), String> {
    let hwnd = parent
        .hwnd()
        .map_err(|_| "The owner window is unavailable.")?;
    let mut state = State {
        body: Body::Notice {
            title,
            text,
            accept: None,
        },
        style: Style::new(dark),
        response: None,
    };
    present(hwnd.0, 2103, &mut state).map(|_| ())
}

pub(super) fn confirm(
    parent: &WebviewWindow,
    title: &str,
    text: &str,
    accept: &str,
    dark: bool,
) -> Result<bool, String> {
    let hwnd = parent
        .hwnd()
        .map_err(|_| "The owner window is unavailable.")?;
    let mut state = State {
        body: Body::Notice {
            title,
            text,
            accept: Some(accept),
        },
        style: Style::new(dark),
        response: None,
    };
    Ok(present(hwnd.0, 2102, &mut state)? == IDOK as isize)
}

pub(super) fn show(
    parent: &WebviewWindow,
    form: &Form,
    dark: bool,
) -> Result<Option<ProfileAction>, String> {
    let hwnd = parent
        .hwnd()
        .map_err(|_| "The owner window is unavailable.")?;
    let mut state = State {
        body: Body::Form(form),
        style: Style::new(dark),
        response: None,
    };
    present(
        hwnd.0,
        if form.mode == Mode::Clone { 2101 } else { 2100 },
        &mut state,
    )?;
    Ok(state.response)
}
