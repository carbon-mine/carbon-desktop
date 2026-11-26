use super::{wide, NAME, NUMBERS, VERSION};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::{
        Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE},
        Gdi::*,
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        Controls::*,
        Input::KeyboardAndMouse::GetFocus,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

const fn rgb(hex: u32) -> u32 {
    ((hex & 0xff) << 16) | (hex & 0xff00) | (hex >> 16)
}

#[derive(Clone, Copy)]
pub(super) struct Style {
    pub dark: bool,
    native: bool,
    background: u32,
    surface: u32,
    ink: u32,
    line: u32,
    focus: u32,
}
impl Style {
    pub fn new(dark: bool) -> Self {
        let mut hc = HIGHCONTRASTW {
            cbSize: size_of::<HIGHCONTRASTW>() as u32,
            ..Default::default()
        };
        let active = unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                hc.cbSize,
                std::ptr::from_mut(&mut hc).cast(),
                0,
            )
        };
        Self {
            dark,
            native: active == 0 || hc.dwFlags & HCF_HIGHCONTRASTON != 0,
            background: rgb(if dark { 0x202020 } else { 0xf2f2f2 }),
            surface: rgb(if dark { 0x262626 } else { 0xffffff }),
            ink: rgb(if dark { 0xf4f4f4 } else { 0x161616 }),
            line: rgb(if dark { 0x909090 } else { 0x777777 }),
            focus: rgb(if dark { 0xffffff } else { 0x171717 }),
        }
    }

    pub unsafe fn init(self, hwnd: HWND) {
        let dark = i32::from(self.dark && !self.native);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            std::ptr::from_ref(&dark).cast(),
            size_of::<i32>() as u32,
        );
        for id in [IDOK, IDCANCEL] {
            let button = GetDlgItem(hwnd, id);
            if button.is_null() {
                continue;
            }
            let kind = if self.native {
                if id == IDOK {
                    BS_DEFPUSHBUTTON
                } else {
                    BS_PUSHBUTTON
                }
            } else {
                BS_OWNERDRAW
            };
            let bits = GetWindowLongPtrW(button, GWL_STYLE);
            SetWindowLongPtrW(
                button,
                GWL_STYLE,
                (bits & !(BS_TYPEMASK as isize)) | kind as isize,
            );
        }
        SendMessageW(hwnd, DM_SETDEFID, IDOK as usize, 0);
        for id in [NAME, VERSION, NUMBERS[0], NUMBERS[1], NUMBERS[2]] {
            let control = GetDlgItem(hwnd, id);
            if control.is_null() {
                continue;
            }
            if self.dark && !self.native {
                SetWindowSubclass(control, Some(dark_field), id as usize, 0);
            } else {
                RemoveWindowSubclass(control, Some(dark_field), id as usize);
            }
            SetWindowPos(
                control,
                null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let combo = GetDlgItem(hwnd, VERSION);
        if !combo.is_null() {
            let mut height = RECT {
                bottom: 14,
                ..Default::default()
            };
            MapDialogRect(hwnd, &mut height);
            SendMessageW(combo, CB_SETITEMHEIGHT, usize::MAX, height.bottom as isize);
        }

        for id in [NAME, NUMBERS[0], NUMBERS[1], NUMBERS[2]] {
            let edit = GetDlgItem(hwnd, id);
            if !edit.is_null() {
                SendMessageW(
                    edit,
                    EM_SETMARGINS,
                    (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
                    0x00050005,
                );
            }
        }
    }

    /// The dialog owns its ordinary Win32 controls. Rendering customises only their colors and border.
    pub unsafe fn message(self, hwnd: HWND, msg: u32, w: usize, l: isize) -> Option<isize> {
        if msg == WM_DRAWITEM && l != 0 {
            let item = &*(l as *const DRAWITEMSTRUCT);
            if item.CtlType == ODT_COMBOBOX && item.CtlID == VERSION as u32 {
                self.combo(item);
                return Some(1);
            }
            if !self.native && item.CtlType == ODT_BUTTON {
                self.button(item);
                return Some(1);
            }
        }
        if self.native {
            return None;
        }
        let dc = w as HDC;
        match msg {
            WM_CTLCOLORDLG | WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let color = if matches!(msg, WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX) {
                    self.surface
                } else {
                    self.background
                };
                SetTextColor(dc, self.ink);
                SetBkColor(dc, color);
                SetDCBrushColor(dc, color);
                Some(GetStockObject(DC_BRUSH) as isize)
            }
            WM_COMMAND => {
                let notification = (w >> 16) as u32;
                if matches!(
                    notification,
                    EN_SETFOCUS | EN_KILLFOCUS | CBN_SETFOCUS | CBN_KILLFOCUS
                ) {
                    InvalidateRect(hwnd, null(), 1);
                }
                None
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                fill(dc, &paint.rcPaint, self.background);
                let focus = GetFocus();
                if !focus.is_null()
                    && GetParent(focus) == hwnd
                    && [NAME, VERSION, NUMBERS[0], NUMBERS[1], NUMBERS[2]]
                        .contains(&GetDlgCtrlID(focus))
                {
                    let mut rect = RECT::default();
                    GetWindowRect(focus, &mut rect);
                    MapWindowPoints(
                        null_mut(),
                        hwnd,
                        std::ptr::from_mut(&mut rect).cast::<POINT>(),
                        2,
                    );
                    InflateRect(&mut rect, 2, 2);
                    SetDCBrushColor(dc, self.focus);
                    FrameRect(dc, &rect, GetStockObject(DC_BRUSH));
                    InflateRect(&mut rect, -1, -1);
                    FrameRect(dc, &rect, GetStockObject(DC_BRUSH));
                }
                EndPaint(hwnd, &paint);
                Some(1)
            }
            _ => None,
        }
    }

    unsafe fn button(self, item: &DRAWITEMSTRUCT) {
        let dc = item.hDC;
        let saved = SaveDC(dc);
        let primary = item.CtlID == IDOK as u32;
        let pressed = item.itemState & ODS_SELECTED != 0;
        let color = match (primary, pressed) {
            (true, true) => rgb(if self.dark { 0xeeeeee } else { 0x191919 }),
            (true, false) => rgb(if self.dark { 0xd7d7d7 } else { 0x353535 }),
            (false, true) => rgb(if self.dark { 0x383838 } else { 0xe6e6e6 }),
            (false, false) => self.surface,
        };
        let ink = if primary {
            rgb(if self.dark { 0x181818 } else { 0xffffff })
        } else {
            self.ink
        };
        let rect = item.rcItem;
        fill(dc, &rect, self.background);
        SelectObject(dc, GetStockObject(DC_BRUSH));
        SetDCBrushColor(dc, color);
        SelectObject(dc, GetStockObject(DC_PEN));
        SetDCPenColor(dc, if primary { color } else { self.line });
        Rectangle(dc, rect.left, rect.top, rect.right, rect.bottom);
        let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0);
        if font != 0 {
            SelectObject(dc, font as HGDIOBJ);
        }
        let mut buf = [0u16; 128];
        let len = GetWindowTextW(item.hwndItem, buf.as_mut_ptr(), buf.len() as i32);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, ink);
        let mut inner = rect;
        DrawTextW(
            dc,
            buf.as_ptr(),
            len,
            &mut inner,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if item.itemState & ODS_FOCUS != 0 {
            InflateRect(&mut inner, -3, -3);
            SetDCBrushColor(dc, if primary { ink } else { self.focus });
            FrameRect(dc, &inner, GetStockObject(DC_BRUSH));
            InflateRect(&mut inner, -1, -1);
            FrameRect(dc, &inner, GetStockObject(DC_BRUSH));
        }
        RestoreDC(dc, saved);
    }

    unsafe fn combo(self, item: &DRAWITEMSTRUCT) {
        let active = item.itemState & ODS_SELECTED != 0;
        let (paper, ink) = if self.native {
            (
                GetSysColor(if active {
                    COLOR_HIGHLIGHT
                } else {
                    COLOR_WINDOW
                }),
                GetSysColor(if active {
                    COLOR_HIGHLIGHTTEXT
                } else {
                    COLOR_WINDOWTEXT
                }),
            )
        } else if active {
            (rgb(if self.dark { 0x404040 } else { 0xdedede }), self.ink)
        } else {
            (self.surface, self.ink)
        };
        let dc = item.hDC;
        let saved = SaveDC(dc);
        fill(dc, &item.rcItem, paper);
        if let Some(version) = super::VanillaVersion::ALL.get(item.itemID as usize) {
            let value = wide(version.id());
            let mut area = item.rcItem;
            area.left += 5;
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, ink);
            DrawTextW(
                dc,
                value.as_ptr(),
                -1,
                &mut area,
                DT_SINGLELINE | DT_VCENTER | DT_LEFT,
            );
        }
        if item.itemState & ODS_FOCUS != 0 {
            DrawFocusRect(dc, &item.rcItem);
        }
        RestoreDC(dc, saved);
    }
}
unsafe fn fill(dc: HDC, rect: &RECT, color: u32) {
    SetDCBrushColor(dc, color);
    FillRect(dc, rect, GetStockObject(DC_BRUSH));
}

// These subclasses replace paint only; the registered EDIT/COMBOBOX classes keep all input,
// UI Automation, popup-list, selection and keyboard navigation behavior.
unsafe extern "system" fn dark_field(
    hwnd: HWND,
    msg: u32,
    w: usize,
    l: isize,
    id: usize,
    _: usize,
) -> isize {
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(dark_field), id);
        return DefSubclassProc(hwnd, msg, w, l);
    }
    if id == VERSION as usize {
        match msg {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                closed_combo(hwnd, dc);
                EndPaint(hwnd, &paint);
                return 0;
            }
            WM_PRINTCLIENT | WM_PRINT => {
                closed_combo(hwnd, w as HDC);
                return 0;
            }
            WM_ERASEBKGND => return 1,
            _ => {}
        }
    } else if msg == WM_NCCALCSIZE && l != 0 {
        // A single-line editor computes its client from the dialog font. Reserve equal
        // top/bottom padding for the native caret and selection instead of a raised bevel.
        let rect = if w == 0 {
            &mut *(l as *mut RECT)
        } else {
            &mut (*(l as *mut NCCALCSIZE_PARAMS)).rgrc[0]
        };
        let dc = GetDC(hwnd);
        let font = SendMessageW(hwnd, WM_GETFONT, 0, 0);
        let previous = if font != 0 {
            SelectObject(dc, font as HGDIOBJ)
        } else {
            null_mut()
        };
        let mut text = TEXTMETRICW::default();
        GetTextMetricsW(dc, &mut text);
        if !previous.is_null() {
            SelectObject(dc, previous);
        }
        ReleaseDC(hwnd, dc);
        let pad = ((rect.bottom - rect.top - text.tmHeight) / 2).max(1);
        InflateRect(rect, -1, -pad);
        return 0;
    } else if msg == WM_NCPAINT {
        let dc = GetWindowDC(hwnd);
        dark_edit_border(hwnd, dc);
        ReleaseDC(hwnd, dc);
        return 0;
    } else if msg == WM_PRINT {
        let result = DefSubclassProc(hwnd, msg, w, l);
        if l & PRF_NONCLIENT as isize != 0 {
            dark_edit_border(hwnd, w as HDC);
        }
        return result;
    }
    let result = DefSubclassProc(hwnd, msg, w, l);
    if id == VERSION as usize
        && matches!(
            msg,
            CB_SETCURSEL | CB_SHOWDROPDOWN | WM_SETFOCUS | WM_KILLFOCUS | WM_KEYUP | WM_LBUTTONUP
        )
    {
        InvalidateRect(hwnd, null(), 0);
    }
    result
}

unsafe fn dark_edit_border(hwnd: HWND, dc: HDC) {
    let saved = SaveDC(dc);
    let mut window = RECT::default();
    let mut client = RECT::default();
    GetWindowRect(hwnd, &mut window);
    GetClientRect(hwnd, &mut client);
    MapWindowPoints(
        hwnd,
        null_mut(),
        std::ptr::from_mut(&mut client).cast::<POINT>(),
        2,
    );
    OffsetRect(&mut client, -window.left, -window.top);
    let origin = (window.left, window.top);
    OffsetRect(&mut window, -origin.0, -origin.1);
    ExcludeClipRect(dc, client.left, client.top, client.right, client.bottom);
    fill(dc, &window, rgb(0x262626));
    SetDCBrushColor(dc, rgb(0x909090));
    FrameRect(dc, &window, GetStockObject(DC_BRUSH));
    RestoreDC(dc, saved);
}

unsafe fn closed_combo(hwnd: HWND, dc: HDC) {
    let saved = SaveDC(dc);
    let mut rect = RECT::default();
    GetClientRect(hwnd, &mut rect);
    fill(dc, &rect, rgb(0x262626));
    SetDCBrushColor(dc, rgb(0x909090));
    FrameRect(dc, &rect, GetStockObject(DC_BRUSH));
    let font = SendMessageW(hwnd, WM_GETFONT, 0, 0);
    if font != 0 {
        SelectObject(dc, font as HGDIOBJ);
    }
    let index = SendMessageW(hwnd, CB_GETCURSEL, 0, 0);
    if let Some(version) = super::VanillaVersion::ALL.get(index as usize) {
        let mut text = rect;
        text.left += 7;
        text.right -= 24;
        SetTextColor(dc, rgb(0xf4f4f4));
        SetBkMode(dc, TRANSPARENT as i32);
        DrawTextW(
            dc,
            wide(version.id()).as_ptr(),
            -1,
            &mut text,
            DT_SINGLELINE | DT_LEFT | DT_VCENTER,
        );
    }
    SelectObject(dc, GetStockObject(DC_PEN));
    SetDCPenColor(dc, rgb(0xbdbdbd));
    let x = rect.right - 13;
    let y = (rect.top + rect.bottom) / 2 - 2;
    MoveToEx(dc, x - 4, y, null_mut());
    LineTo(dc, x, y + 4);
    LineTo(dc, x + 5, y - 1);
    RestoreDC(dc, saved);
}
