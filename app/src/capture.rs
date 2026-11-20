//! "Press a key" box for the Keyboard Setup dialog.
//!
//! A small custom control: click it (or tab to it), press any key or key
//! combination, and it shows the chord, e.g. "Right Ctrl" or "Shift+F5".
//! A modifier key on its own is captured when it is released, so Right Ctrl
//! by itself can be assigned. It tells the parent with
//! `WM_COMMAND(id, CN_CAPTURED)`; read the chord with [`captured`].

use std::cell::{Cell, RefCell};

use tn3270_keymap::KeyChord;

use crate::win32::*;

pub const CLASS: &str = "Tn3270KeyCapture";
pub const CN_CAPTURED: u16 = 1;

thread_local! {
    static CAPTURED: Cell<Option<KeyChord>> = const { Cell::new(None) };
    static PENDING_MOD: Cell<Option<KeyChord>> = const { Cell::new(None) };
    static FOCUSED: Cell<bool> = const { Cell::new(false) };
    static FONT: Cell<HFONT> = const { Cell::new(std::ptr::null_mut()) };
    static CLASS_W: RefCell<Vec<u16>> = RefCell::new(wide(CLASS));
}

pub fn register() {
    CLASS_W.with(|c| unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(std::ptr::null()),
            hIcon: null(),
            hCursor: LoadCursorW(null(), IDC_ARROW),
            hbrBackground: null(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: c.borrow().as_ptr(),
            hIconSm: null(),
        };
        RegisterClassExW(&wc);
    });
}

pub fn captured() -> Option<KeyChord> {
    CAPTURED.with(Cell::get)
}

pub fn clear(hwnd: HWND) {
    CAPTURED.with(|c| c.set(None));
    PENDING_MOD.with(|c| c.set(None));
    unsafe { InvalidateRect(hwnd, std::ptr::null(), 1) };
}

fn capture(hwnd: HWND, chord: KeyChord) {
    CAPTURED.with(|c| c.set(Some(chord)));
    unsafe {
        InvalidateRect(hwnd, std::ptr::null(), 1);
        let id = GetDlgCtrlID(hwnd) as usize;
        SendMessageW(GetParent(hwnd), WM_COMMAND, id | ((CN_CAPTURED as usize) << 16), hwnd as LPARAM);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_GETDLGCODE => DLGC_WANTALLKEYS | DLGC_WANTCHARS,
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let chord = chord_from_msg(wp, lp);
            if chord.is_modifier_key() {
                if !is_repeat(lp) {
                    PENDING_MOD.with(|p| p.set(Some(chord)));
                }
            } else {
                PENDING_MOD.with(|p| p.set(None));
                capture(hwnd, chord);
            }
            0
        }
        WM_KEYUP | WM_SYSKEYUP => {
            let up = chord_from_msg(wp, lp);
            if let Some(p) = PENDING_MOD.with(|p| p.get()) {
                if p.vk == up.vk {
                    PENDING_MOD.with(|c| c.set(None));
                    capture(hwnd, p);
                }
            }
            0
        }
        WM_CHAR | WM_SYSCHAR => 0,
        WM_SETFOCUS | WM_KILLFOCUS => {
            FOCUSED.with(|f| f.set(msg == WM_SETFOCUS));
            PENDING_MOD.with(|p| p.set(None));
            InvalidateRect(hwnd, std::ptr::null(), 1);
            0
        }
        WM_LBUTTONDOWN => {
            SetFocus(hwnd);
            0
        }
        WM_SETFONT => {
            FONT.with(|f| f.set(wp as HFONT));
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc);
            FillRect(hdc, &rc, GetSysColorBrush(COLOR_WINDOW));
            let old = SelectObject(hdc, FONT.with(Cell::get));
            SetBkMode(hdc, TRANSPARENT);
            let (text, color) = match captured() {
                Some(c) => (c.to_string(), GetSysColor(COLOR_WINDOWTEXT)),
                None if FOCUSED.with(Cell::get) => ("Press a key now...".into(), GetSysColor(COLOR_GRAYTEXT)),
                None => ("Click here, then press a key".into(), GetSysColor(COLOR_GRAYTEXT)),
            };
            SetTextColor(hdc, color);
            let w: Vec<u16> = text.encode_utf16().collect();
            TextOutW(hdc, 6, (rc.bottom - 15) / 2, w.as_ptr(), w.len() as i32);
            if FOCUSED.with(Cell::get) {
                let focus = RECT { left: 2, top: 2, right: rc.right - 2, bottom: rc.bottom - 2 };
                DrawFocusRect(hdc, &focus);
            }
            SelectObject(hdc, old);
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
