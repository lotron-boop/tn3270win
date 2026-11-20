//! Minimal hand-written Win32 bindings (no external crates).
//!
//! Only what the app uses. Declarations follow the Windows SDK headers;
//! they type-check on any host, and link against user32/gdi32/... on Windows.
#![allow(non_snake_case, non_camel_case_types, dead_code, clippy::upper_case_acronyms)]

use std::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type HWND = HANDLE;
pub type HMENU = HANDLE;
pub type HINSTANCE = HANDLE;
pub type HFONT = HANDLE;
pub type HDC = HANDLE;
pub type HBRUSH = HANDLE;
pub type HGDIOBJ = HANDLE;
pub type HICON = HANDLE;
pub type HCURSOR = HANDLE;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type BOOL = i32;
pub type ATOM = u16;
pub type WNDPROC = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>;

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: WNDPROC,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HICON,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SIZE {
    pub cx: i32,
    pub cy: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
}

#[repr(C)]
pub struct PAINTSTRUCT {
    pub hdc: HDC,
    pub fErase: BOOL,
    pub rcPaint: RECT,
    pub fRestore: BOOL,
    pub fIncUpdate: BOOL,
    pub rgbReserved: [u8; 32],
}

#[repr(C)]
pub struct NMHDR {
    pub hwndFrom: HWND,
    pub idFrom: usize,
    pub code: u32,
}

#[repr(C)]
pub struct NMLISTVIEW {
    pub hdr: NMHDR,
    pub iItem: i32,
    pub iSubItem: i32,
    pub uNewState: u32,
    pub uOldState: u32,
    pub uChanged: u32,
    pub ptAction: POINT,
    pub lParam: LPARAM,
}

#[repr(C)]
pub struct LVCOLUMNW {
    pub mask: u32,
    pub fmt: i32,
    pub cx: i32,
    pub pszText: *mut u16,
    pub cchTextMax: i32,
    pub iSubItem: i32,
    pub iImage: i32,
    pub iOrder: i32,
    pub cxMin: i32,
    pub cxDefault: i32,
    pub cxIdeal: i32,
}

#[repr(C)]
pub struct LVITEMW {
    pub mask: u32,
    pub iItem: i32,
    pub iSubItem: i32,
    pub state: u32,
    pub stateMask: u32,
    pub pszText: *mut u16,
    pub cchTextMax: i32,
    pub iImage: i32,
    pub lParam: LPARAM,
    pub iIndent: i32,
    pub iGroupId: i32,
    pub cColumns: u32,
    pub puColumns: *mut u32,
    pub piColFmt: *mut i32,
    pub iGroup: i32,
}

#[repr(C)]
pub struct INITCOMMONCONTROLSEX {
    pub dwSize: u32,
    pub dwICC: u32,
}

// Window styles
pub const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
pub const WS_POPUP: u32 = 0x8000_0000;
pub const WS_CHILD: u32 = 0x4000_0000;
pub const WS_VISIBLE: u32 = 0x1000_0000;
pub const WS_CAPTION: u32 = 0x00C0_0000;
pub const WS_BORDER: u32 = 0x0080_0000;
pub const WS_SYSMENU: u32 = 0x0008_0000;
pub const WS_VSCROLL: u32 = 0x0020_0000;
pub const WS_GROUP: u32 = 0x0002_0000;
pub const WS_TABSTOP: u32 = 0x0001_0000;
pub const WS_EX_DLGMODALFRAME: u32 = 0x0001;
pub const WS_EX_CLIENTEDGE: u32 = 0x0200;
pub const WS_EX_CONTROLPARENT: u32 = 0x0001_0000;
pub const CW_USEDEFAULT: i32 = 0x8000_0000_u32 as i32;
pub const SW_SHOW: i32 = 5;

// Control styles
pub const BS_PUSHBUTTON: u32 = 0;
pub const BS_DEFPUSHBUTTON: u32 = 1;
pub const BS_GROUPBOX: u32 = 7;
pub const LBS_NOTIFY: u32 = 1;
pub const LBS_NOINTEGRALHEIGHT: u32 = 0x0100;
pub const CBS_DROPDOWNLIST: u32 = 3;
pub const SS_LEFT: u32 = 0;
pub const LVS_REPORT: u32 = 0x0001;
pub const LVS_SINGLESEL: u32 = 0x0004;
pub const LVS_SHOWSELALWAYS: u32 = 0x0008;
pub const LVS_EX_FULLROWSELECT: usize = 0x0020;
pub const LVS_EX_DOUBLEBUFFER: usize = 0x0001_0000;

// Messages
pub const WM_CREATE: u32 = 0x0001;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_SETFOCUS: u32 = 0x0007;
pub const WM_KILLFOCUS: u32 = 0x0008;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_ERASEBKGND: u32 = 0x0014;
pub const WM_SETFONT: u32 = 0x0030;
pub const WM_NOTIFY: u32 = 0x004E;
pub const WM_GETDLGCODE: u32 = 0x0087;
pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_KEYUP: u32 = 0x0101;
pub const WM_CHAR: u32 = 0x0102;
pub const WM_SYSKEYDOWN: u32 = 0x0104;
pub const WM_SYSKEYUP: u32 = 0x0105;
pub const WM_SYSCHAR: u32 = 0x0106;
pub const WM_COMMAND: u32 = 0x0111;
pub const WM_CTLCOLORSTATIC: u32 = 0x0138;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_ACTIVATE: u32 = 0x0006;
pub const WM_SIZE: u32 = 0x0005;
pub const WM_INITMENUPOPUP: u32 = 0x0117;
pub const WM_APP: u32 = 0x8000;
pub const ES_NUMBER: u32 = 0x2000;
pub const SRCCOPY: u32 = 0x00CC_0020;
pub const CF_UNICODETEXT: u32 = 13;
pub const GMEM_MOVEABLE: u32 = 0x0002;
pub const MF_BYCOMMAND: u32 = 0x0000;
pub const MF_ENABLED: u32 = 0x0000;
pub const MB_BEEP: u32 = 0xFFFF_FFFF;
pub const MF_CHECKED: u32 = 0x0008;
pub const MF_UNCHECKED: u32 = 0x0000;
pub const WM_SETCURSOR: u32 = 0x0020;
pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const HTCLIENT: u16 = 1;
pub const IDC_CROSS: *const u16 = 32515 as *const u16;
pub const DLGC_WANTALLKEYS: LRESULT = 0x0004;
pub const DLGC_WANTCHARS: LRESULT = 0x0080;

pub const LB_ADDSTRING: u32 = 0x0180;
pub const LB_RESETCONTENT: u32 = 0x0184;
pub const LB_SETCURSEL: u32 = 0x0186;
pub const LB_GETCURSEL: u32 = 0x0188;
pub const EM_SETSEL: u32 = 0x00B1;
pub const EM_LIMITTEXT: u32 = 0x00C5;
pub const ES_AUTOHSCROLL: u32 = 0x0080;
pub const CB_ADDSTRING: u32 = 0x0143;
pub const CB_RESETCONTENT: u32 = 0x014B;
pub const CB_GETCURSEL: u32 = 0x0147;
pub const CB_SETCURSEL: u32 = 0x014E;
pub const BN_CLICKED: u16 = 0;

pub const LVM_FIRST: u32 = 0x1000;
pub const LVM_DELETEALLITEMS: u32 = LVM_FIRST + 9;
pub const LVM_GETNEXTITEM: u32 = LVM_FIRST + 12;
pub const LVM_ENSUREVISIBLE: u32 = LVM_FIRST + 19;
pub const LVM_SETITEMSTATE: u32 = LVM_FIRST + 43;
pub const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = LVM_FIRST + 54;
pub const LVM_INSERTITEMW: u32 = LVM_FIRST + 77;
pub const LVM_INSERTCOLUMNW: u32 = LVM_FIRST + 97;
pub const LVM_SETITEMTEXTW: u32 = LVM_FIRST + 116;
pub const LVIF_TEXT: u32 = 0x0001;
pub const LVIF_PARAM: u32 = 0x0004;
pub const LVIF_STATE: u32 = 0x0008;
pub const LVIS_FOCUSED: u32 = 0x0001;
pub const LVIS_SELECTED: u32 = 0x0002;
pub const LVCF_WIDTH: u32 = 0x0002;
pub const LVCF_TEXT: u32 = 0x0004;
pub const LVCF_SUBITEM: u32 = 0x0008;
pub const LVN_ITEMCHANGED: u32 = (-101i32) as u32;
pub const ICC_LISTVIEW_CLASSES: u32 = 0x0001;
pub const ICC_STANDARD_CLASSES: u32 = 0x4000;

// Menus
pub const MF_STRING: u32 = 0x0000;
pub const MF_GRAYED: u32 = 0x0001;
pub const MF_POPUP: u32 = 0x0010;
pub const MF_SEPARATOR: u32 = 0x0800;

// Message boxes
pub const MB_OK: u32 = 0x0000;
pub const MB_YESNO: u32 = 0x0004;
pub const MB_YESNOCANCEL: u32 = 0x0003;
pub const MB_DEFBUTTON3: u32 = 0x0200;
pub const IDNO: i32 = 7;
pub const MB_ICONERROR: u32 = 0x0010;
pub const MB_ICONQUESTION: u32 = 0x0020;
pub const MB_ICONWARNING: u32 = 0x0030;
pub const MB_ICONINFORMATION: u32 = 0x0040;
pub const IDOK: i32 = 1;
pub const IDCANCEL: i32 = 2;
pub const IDYES: i32 = 6;

// Colours, GDI
pub const COLOR_WINDOW: i32 = 5;
pub const COLOR_WINDOWTEXT: i32 = 8;
pub const COLOR_BTNFACE: i32 = 15;
pub const COLOR_GRAYTEXT: i32 = 17;
pub const TRANSPARENT: i32 = 1;
pub const DEFAULT_CHARSET: u32 = 1;
pub const CLEARTYPE_QUALITY: u32 = 5;
pub const FIXED_PITCH: u32 = 1;
pub const FW_NORMAL: i32 = 400;
pub const IDC_ARROW: *const u16 = 32512 as *const u16;

// Virtual keys used for modifier state
pub const VK_SHIFT: i32 = 0x10;
pub const VK_CONTROL: i32 = 0x11;
pub const VK_MENU: i32 = 0x12;

#[cfg_attr(windows, link(name = "user32"))]
extern "system" {
    pub fn RegisterClassExW(wc: *const WNDCLASSEXW) -> ATOM;
    pub fn CreateWindowExW(
        ex_style: u32, class: *const u16, name: *const u16, style: u32,
        x: i32, y: i32, w: i32, h: i32,
        parent: HWND, menu: HMENU, instance: HINSTANCE, param: *const c_void,
    ) -> HWND;
    pub fn DefWindowProcW(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT;
    pub fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> BOOL;
    pub fn TranslateMessage(msg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    pub fn IsDialogMessageW(dlg: HWND, msg: *mut MSG) -> BOOL;
    pub fn PostQuitMessage(code: i32);
    pub fn ShowWindow(hwnd: HWND, cmd: i32) -> BOOL;
    pub fn UpdateWindow(hwnd: HWND) -> BOOL;
    pub fn DestroyWindow(hwnd: HWND) -> BOOL;
    pub fn SendMessageW(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT;
    pub fn EnableWindow(hwnd: HWND, enable: BOOL) -> BOOL;
    pub fn SetForegroundWindow(hwnd: HWND) -> BOOL;
    pub fn SetFocus(hwnd: HWND) -> HWND;
    pub fn GetParent(hwnd: HWND) -> HWND;
    pub fn GetDlgCtrlID(hwnd: HWND) -> i32;
    pub fn GetKeyState(vk: i32) -> i16;
    pub fn InvalidateRect(hwnd: HWND, rect: *const RECT, erase: BOOL) -> BOOL;
    pub fn BeginPaint(hwnd: HWND, ps: *mut PAINTSTRUCT) -> HDC;
    pub fn EndPaint(hwnd: HWND, ps: *const PAINTSTRUCT) -> BOOL;
    pub fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> BOOL;
    pub fn GetWindowRect(hwnd: HWND, rect: *mut RECT) -> BOOL;
    pub fn AdjustWindowRectEx(rect: *mut RECT, style: u32, menu: BOOL, ex_style: u32) -> BOOL;
    pub fn FillRect(hdc: HDC, rect: *const RECT, brush: HBRUSH) -> i32;
    pub fn DrawFocusRect(hdc: HDC, rect: *const RECT) -> BOOL;
    pub fn CreateMenu() -> HMENU;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn AppendMenuW(menu: HMENU, flags: u32, id: usize, text: *const u16) -> BOOL;
    pub fn MessageBoxW(hwnd: HWND, text: *const u16, caption: *const u16, kind: u32) -> i32;
    pub fn LoadCursorW(instance: HINSTANCE, name: *const u16) -> HCURSOR;
    pub fn SetWindowTextW(hwnd: HWND, text: *const u16) -> BOOL;
    pub fn GetWindowTextW(hwnd: HWND, buf: *mut u16, max: i32) -> i32;
    pub fn GetWindowTextLengthW(hwnd: HWND) -> i32;
    pub fn GetSysColor(index: i32) -> u32;
    pub fn GetSysColorBrush(index: i32) -> HBRUSH;
    pub fn PostMessageW(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> BOOL;
    pub fn GetDC(hwnd: HWND) -> HDC;
    pub fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
    pub fn InvertRect(hdc: HDC, rect: *const RECT) -> BOOL;
    pub fn MessageBeep(kind: u32) -> BOOL;
    pub fn EnableMenuItem(menu: HMENU, id: u32, flags: u32) -> BOOL;
    pub fn CheckMenuItem(menu: HMENU, id: u32, flags: u32) -> u32;
    pub fn SetCapture(hwnd: HWND) -> HWND;
    pub fn ReleaseCapture() -> BOOL;
    pub fn SetCursor(cursor: HCURSOR) -> HCURSOR;
    pub fn OpenClipboard(owner: HWND) -> BOOL;
    pub fn CloseClipboard() -> BOOL;
    pub fn EmptyClipboard() -> BOOL;
    pub fn SetClipboardData(format: u32, mem: HANDLE) -> HANDLE;
    pub fn GetClipboardData(format: u32) -> HANDLE;
}

#[cfg_attr(windows, link(name = "gdi32"))]
extern "system" {
    pub fn CreateFontW(
        height: i32, width: i32, escapement: i32, orientation: i32, weight: i32,
        italic: u32, underline: u32, strike: u32, charset: u32, out_precision: u32,
        clip_precision: u32, quality: u32, pitch_family: u32, face: *const u16,
    ) -> HFONT;
    pub fn SelectObject(hdc: HDC, obj: HGDIOBJ) -> HGDIOBJ;
    pub fn DeleteObject(obj: HGDIOBJ) -> BOOL;
    pub fn SetTextColor(hdc: HDC, color: u32) -> u32;
    pub fn SetBkColor(hdc: HDC, color: u32) -> u32;
    pub fn SetBkMode(hdc: HDC, mode: i32) -> i32;
    pub fn TextOutW(hdc: HDC, x: i32, y: i32, text: *const u16, len: i32) -> BOOL;
    pub fn GetTextExtentPoint32W(hdc: HDC, text: *const u16, len: i32, size: *mut SIZE) -> BOOL;
    pub fn CreateSolidBrush(color: u32) -> HBRUSH;
    pub fn CreateCompatibleDC(hdc: HDC) -> HDC;
    pub fn CreateCompatibleBitmap(hdc: HDC, w: i32, h: i32) -> HANDLE;
    pub fn BitBlt(dst: HDC, x: i32, y: i32, w: i32, h: i32, src: HDC, sx: i32, sy: i32, rop: u32) -> BOOL;
    pub fn DeleteDC(hdc: HDC) -> BOOL;
}

#[cfg_attr(windows, link(name = "kernel32"))]
extern "system" {
    pub fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    pub fn GlobalAlloc(flags: u32, bytes: usize) -> HANDLE;
    pub fn GlobalLock(mem: HANDLE) -> *mut c_void;
    pub fn GlobalUnlock(mem: HANDLE) -> BOOL;
    pub fn GlobalFree(mem: HANDLE) -> HANDLE;
}

#[cfg_attr(windows, link(name = "comctl32"))]
extern "system" {
    pub fn InitCommonControlsEx(icc: *const INITCOMMONCONTROLSEX) -> BOOL;
}

#[cfg_attr(windows, link(name = "uxtheme"))]
extern "system" {
    pub fn SetWindowTheme(hwnd: HWND, app_name: *const u16, id_list: *const u16) -> i32;
}

// ---- small helpers -------------------------------------------------------

/// NUL-terminated UTF-16 for passing to Win32.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn loword(x: usize) -> u16 {
    (x & 0xFFFF) as u16
}

pub fn hiword(x: usize) -> u16 {
    ((x >> 16) & 0xFFFF) as u16
}

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

pub fn null() -> HANDLE {
    std::ptr::null_mut()
}

pub fn message_box(owner: HWND, text: &str, caption: &str, kind: u32) -> i32 {
    unsafe { MessageBoxW(owner, wide(text).as_ptr(), wide(caption).as_ptr(), kind) }
}

/// Create a child control and give it `font`.
#[allow(clippy::too_many_arguments)]
pub fn child(
    parent: HWND, class: &str, text: &str, style: u32, ex_style: u32,
    (x, y, w, h): (i32, i32, i32, i32), id: i32, font: HFONT,
) -> HWND {
    unsafe {
        let hwnd = CreateWindowExW(
            ex_style, wide(class).as_ptr(), wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style, x, y, w, h,
            parent, id as usize as HMENU, GetModuleHandleW(std::ptr::null()), std::ptr::null(),
        );
        SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
        hwnd
    }
}

/// The standard Windows UI font (Segoe UI 9pt at 96 DPI).
pub fn ui_font() -> HFONT {
    unsafe {
        CreateFontW(-12, 0, 0, 0, FW_NORMAL, 0, 0, 0, DEFAULT_CHARSET, 0, 0,
            CLEARTYPE_QUALITY, 0, wide("Segoe UI").as_ptr())
    }
}

/// Current modifier state, for building a [`tn3270_keymap::KeyChord`].
pub fn chord_from_msg(wp: WPARAM, lp: LPARAM) -> tn3270_keymap::KeyChord {
    unsafe {
        let down = |vk| GetKeyState(vk) < 0;
        tn3270_keymap::KeyChord::from_win32(
            wp as u16, lp, down(VK_SHIFT), down(VK_CONTROL), down(VK_MENU),
        )
    }
}

/// Read a window's text (e.g. an edit box).
pub fn window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&buf[..n])
    }
}

/// Size and centre a popup over `owner`; returns (x, y, w, h).
pub fn centered(owner: HWND, client_w: i32, client_h: i32, style: u32, ex: u32) -> (i32, i32, i32, i32) {
    unsafe {
        let mut rc = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
        AdjustWindowRectEx(&mut rc, style, 0, ex);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let mut orc = RECT::default();
        GetWindowRect(owner, &mut orc);
        let x = orc.left + ((orc.right - orc.left) - w) / 2;
        let y = orc.top + ((orc.bottom - orc.top) - h) / 2;
        (x, y.max(0), w, h)
    }
}

/// Put text on the clipboard.
pub fn set_clipboard(owner: HWND, text: &str) -> bool {
    let w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(owner) == 0 {
            return false;
        }
        EmptyClipboard();
        let mem = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2);
        let mut ok = false;
        if !mem.is_null() {
            let p = GlobalLock(mem) as *mut u16;
            if !p.is_null() {
                std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
                GlobalUnlock(mem);
                ok = !SetClipboardData(CF_UNICODETEXT, mem).is_null();
            }
            if !ok {
                GlobalFree(mem);
            }
        }
        CloseClipboard();
        ok
    }
}

/// Text on the clipboard, if any.
pub fn get_clipboard(owner: HWND) -> Option<String> {
    unsafe {
        if OpenClipboard(owner) == 0 {
            return None;
        }
        let mem = GetClipboardData(CF_UNICODETEXT);
        let mut out = None;
        if !mem.is_null() {
            let p = GlobalLock(mem) as *const u16;
            if !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(mem);
            }
        }
        CloseClipboard();
        out
    }
}

/// lParam bit 30: the key was already down (auto-repeat).
pub fn is_repeat(lp: LPARAM) -> bool {
    (lp >> 30) & 1 == 1
}

#[cfg(all(test, target_pointer_width = "64"))]
mod layout_tests {
    use super::*;
    use std::mem::size_of;

    /// Sizes from the 64-bit Windows SDK headers.
    #[test]
    fn struct_sizes_match_windows_sdk() {
        assert_eq!(size_of::<WNDCLASSEXW>(), 80);
        assert_eq!(size_of::<MSG>(), 48);
        assert_eq!(size_of::<PAINTSTRUCT>(), 72);
        assert_eq!(size_of::<NMHDR>(), 24);
        assert_eq!(size_of::<NMLISTVIEW>(), 64);
        assert_eq!(size_of::<LVITEMW>(), 88);
        assert_eq!(size_of::<LVCOLUMNW>(), 56);
    }
}
