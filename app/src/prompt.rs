//! A small modal "enter a name" dialog with OK and Cancel.

use std::cell::{Cell, RefCell};

use crate::win32::*;

const CLASS: &str = "Tn3270Prompt";
const IDC_EDIT: i32 = 2001;

thread_local! {
    static DONE: Cell<bool> = const { Cell::new(false) };
    static EDIT: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    static OWNER: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
    static RESULT: RefCell<Option<String>> = const { RefCell::new(None) };
    static CLASS_W: Vec<u16> = wide(CLASS);
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
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            lpszMenuName: std::ptr::null(),
            lpszClassName: c.as_ptr(),
            hIconSm: null(),
        };
        RegisterClassExW(&wc);
    });
}

/// Ask for a line of text. Returns `None` if the user cancels.
pub fn ask(owner: HWND, title: &str, label: &str, initial: &str, max_len: usize) -> Option<String> {
    DONE.with(|d| d.set(false));
    RESULT.with(|r| *r.borrow_mut() = None);
    OWNER.with(|o| o.set(owner));
    unsafe {
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex = WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT;
        let (x, y, w, h) = centered(owner, 360, 112, style, ex);
        let dlg = CLASS_W.with(|c| CreateWindowExW(ex, c.as_ptr(), wide(title).as_ptr(), style,
            x, y, w, h, owner, null(), GetModuleHandleW(std::ptr::null()), std::ptr::null()));
        if dlg.is_null() {
            return None;
        }
        let font = ui_font();
        child(dlg, "STATIC", label, SS_LEFT, 0, (12, 12, 336, 18), -1, font);
        let edit = child(dlg, "EDIT", initial, ES_AUTOHSCROLL | WS_TABSTOP, WS_EX_CLIENTEDGE,
            (12, 32, 336, 24), IDC_EDIT, font);
        SendMessageW(edit, EM_LIMITTEXT, max_len, 0);
        SendMessageW(edit, EM_SETSEL, 0, -1);
        EDIT.with(|e| e.set(edit));
        child(dlg, "BUTTON", "OK", BS_DEFPUSHBUTTON | WS_TABSTOP, 0, (188, 74, 76, 26), IDOK, font);
        child(dlg, "BUTTON", "Cancel", BS_PUSHBUTTON | WS_TABSTOP, 0, (272, 74, 76, 26), IDCANCEL, font);

        EnableWindow(owner, 0);
        ShowWindow(dlg, SW_SHOW);
        SetFocus(edit);
        let mut msg: MSG = std::mem::zeroed();
        while !DONE.with(Cell::get) {
            if GetMessageW(&mut msg, null(), 0, 0) <= 0 {
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            if IsDialogMessageW(dlg, &mut msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    RESULT.with(|r| r.borrow_mut().take())
}

fn close(dlg: HWND, ok: bool) {
    if ok {
        let text = window_text(EDIT.with(Cell::get));
        RESULT.with(|r| *r.borrow_mut() = Some(text.trim().to_string()));
    }
    unsafe {
        EnableWindow(OWNER.with(Cell::get), 1);
        DestroyWindow(dlg);
    }
    DONE.with(|d| d.set(true));
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match loword(wp) as i32 {
                IDOK => close(hwnd, true),
                IDCANCEL => close(hwnd, false),
                _ => {}
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            SetBkColor(wp as HDC, GetSysColor(COLOR_BTNFACE));
            GetSysColorBrush(COLOR_BTNFACE) as LRESULT
        }
        WM_CLOSE => {
            close(hwnd, false);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
