//! tn3270win: a TN3270 terminal emulator for Windows.
//!
//! Milestone 1 is keyboard mapping. The main window is a 24x80 terminal
//! screen (not connected yet) whose status line shows each key you press
//! and the 3270 function it is mapped to. Settings > Keyboard... opens the
//! Keyboard Setup dialog.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod capture;
mod keydlg;
mod settings;
mod win32;

use std::cell::{Cell, RefCell};

use tn3270_keymap::{KeyDispatcher, Keymap};

use win32::*;

const ID_EXIT: usize = 100;
const ID_COPY: usize = 110;
const ID_PASTE: usize = 111;
const ID_CONNECT: usize = 120;
const ID_KEYBOARD: usize = 200;
const ID_SESSION: usize = 201;
const ID_APPEARANCE: usize = 202;
const ID_ABOUT: usize = 900;

const ROWS: i32 = 24;
const COLS: i32 = 80;

struct App {
    keymap: Keymap,
    dispatcher: KeyDispatcher,
    last_key: String,
    last_action: String,
    suppress_char: bool,
    screen_font: HFONT,
    cell: Cell<(i32, i32)>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

#[cfg(not(windows))]
fn main() {
    eprintln!("tn3270win is a Windows application. Build and run it on Windows.");
}

#[cfg(windows)]
fn main() {
    unsafe { run() }
}

#[allow(dead_code)]
unsafe fn run() {
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_LISTVIEW_CLASSES | ICC_STANDARD_CLASSES,
    };
    InitCommonControlsEx(&icc);
    capture::register();
    keydlg::register();

    let (keymap, load_error) = settings::load_keymap();
    let screen_font = CreateFontW(-18, 0, 0, 0, FW_NORMAL, 0, 0, 0, DEFAULT_CHARSET, 0, 0,
        CLEARTYPE_QUALITY, FIXED_PITCH, wide("Consolas").as_ptr());
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            keymap,
            dispatcher: KeyDispatcher::new(),
            last_key: String::new(),
            last_action: String::new(),
            suppress_char: false,
            screen_font,
            cell: Cell::new((0, 0)),
        })
    });

    let instance = GetModuleHandleW(std::ptr::null());
    let class = wide("Tn3270WinMain");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(main_wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: null(),
        hCursor: LoadCursorW(null(), IDC_ARROW),
        hbrBackground: null(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class.as_ptr(),
        hIconSm: null(),
    };
    RegisterClassExW(&wc);

    let hwnd = CreateWindowExW(0, class.as_ptr(), wide("tn3270win - Session A").as_ptr(),
        WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, 940, 640,
        null(), build_menu(), instance, std::ptr::null());
    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);
    if let Some(msg) = load_error {
        message_box(hwnd, &msg, "tn3270win", MB_OK | MB_ICONWARNING);
    }

    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, null(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe fn build_menu() -> HMENU {
    let bar = CreateMenu();
    let popup = |title: &str, items: &[(usize, &str, bool)]| {
        let m = CreatePopupMenu();
        for &(id, text, enabled) in items {
            if text.is_empty() {
                AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
            } else {
                let flags = MF_STRING | if enabled { 0 } else { MF_GRAYED };
                AppendMenuW(m, flags, id, wide(text).as_ptr());
            }
        }
        AppendMenuW(bar, MF_POPUP, m as usize, wide(title).as_ptr());
    };
    popup("&File", &[(ID_EXIT, "E&xit", true)]);
    popup("&Edit", &[(ID_COPY, "&Copy\tCtrl+C", false), (ID_PASTE, "&Paste\tCtrl+V", false)]);
    popup("&Communication", &[(ID_CONNECT, "&Connect...", false)]);
    popup("&Settings", &[
        (ID_KEYBOARD, "&Keyboard...", true),
        (0, "", true),
        (ID_SESSION, "&Session...", false),
        (ID_APPEARANCE, "&Appearance...", false),
    ]);
    popup("&Help", &[(ID_ABOUT, "&About tn3270win", true)]);
    bar
}

/// Record a key and what it did, for the status line.
fn show(hwnd: HWND, key: String, action: String) {
    with_app(|app| {
        app.last_key = key;
        app.last_action = action;
    });
    unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
}

unsafe extern "system" fn main_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let chord = chord_from_msg(wp, lp);
            let handled = with_app(|app| {
                app.suppress_char = false;
                let action = app.dispatcher.key_down(&app.keymap, chord, is_repeat(lp));
                if action.is_some() {
                    app.suppress_char = true;
                }
                action
            }).flatten();
            match handled {
                Some(action) => {
                    // TODO(milestone 3): send to the 3270 session.
                    show(hwnd, chord.to_string(), action.description());
                    0
                }
                None => {
                    if !chord.is_modifier_key() {
                        show(hwnd, chord.to_string(), "(not mapped - types normally)".into());
                    }
                    if msg == WM_SYSKEYDOWN { DefWindowProcW(hwnd, msg, wp, lp) } else { 0 }
                }
            }
        }
        WM_KEYUP | WM_SYSKEYUP => {
            let chord = chord_from_msg(wp, lp);
            match with_app(|app| app.dispatcher.key_up(&app.keymap, chord)).flatten() {
                Some(action) => {
                    show(hwnd, chord.to_string(), action.description());
                    0 // also stops a lone Alt from opening the menu bar
                }
                None if msg == WM_SYSKEYUP => DefWindowProcW(hwnd, msg, wp, lp),
                None => 0,
            }
        }
        WM_CHAR | WM_SYSCHAR => {
            let suppress = with_app(|app| std::mem::take(&mut app.suppress_char)).unwrap_or(false);
            if suppress {
                0
            } else if msg == WM_SYSCHAR {
                DefWindowProcW(hwnd, msg, wp, lp) // menu mnemonics such as Alt+S
            } else {
                0
            }
        }
        WM_ACTIVATE => {
            with_app(|app| app.dispatcher.cancel());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_COMMAND => {
            match loword(wp) as usize {
                ID_EXIT => { DestroyWindow(hwnd); }
                ID_KEYBOARD => open_keyboard_setup(hwnd),
                ID_ABOUT => {
                    message_box(hwnd,
                        &format!("tn3270win {}\nA TN3270 terminal emulator for Windows.\n\nKeyboard map: {}",
                            env!("CARGO_PKG_VERSION"), settings::keymap_path().display()),
                        "About tn3270win", MB_OK | MB_ICONINFORMATION);
                }
                _ => {}
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn open_keyboard_setup(hwnd: HWND) {
    let Some(current) = with_app(|app| app.keymap.clone()) else { return };
    if let Some(new_map) = keydlg::run(hwnd, current) {
        if let Err(e) = settings::save_keymap(&new_map) {
            message_box(hwnd, &format!("The keyboard map could not be saved:\n{e}"),
                "tn3270win", MB_OK | MB_ICONERROR);
        }
        with_app(|app| {
            app.keymap = new_map;
            app.dispatcher.cancel();
        });
        show(hwnd, String::new(), "Keyboard map updated".into());
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    let bg = CreateSolidBrush(rgb(0, 0, 0));
    FillRect(hdc, &rc, bg);
    DeleteObject(bg);

    let (font, cell, key, action, map_name) = match with_app(|a| {
        (a.screen_font, a.cell.get(), a.last_key.clone(), a.last_action.clone(), a.keymap.name.clone())
    }) {
        Some(v) => v,
        None => { EndPaint(hwnd, &ps); return; }
    };
    let old = SelectObject(hdc, font);
    let (cw, ch) = if cell.0 == 0 {
        let mut sz = SIZE::default();
        let m: Vec<u16> = "M".encode_utf16().collect();
        GetTextExtentPoint32W(hdc, m.as_ptr(), 1, &mut sz);
        with_app(|a| a.cell.set((sz.cx, sz.cy)));
        (sz.cx, sz.cy)
    } else {
        cell
    };
    // Centre the 80x24 screen in the window.
    let x0 = ((rc.right - cw * COLS) / 2).max(4);
    let y0 = ((rc.bottom - ch * (ROWS + 2)) / 2).max(4);
    SetBkMode(hdc, TRANSPARENT);

    let text = |row: i32, col: i32, color: u32, s: &str| {
        SetTextColor(hdc, color);
        let w: Vec<u16> = s.encode_utf16().collect();
        TextOutW(hdc, x0 + col * cw, y0 + row * ch, w.as_ptr(), w.len() as i32);
    };
    let green = rgb(0, 255, 0);
    let turquoise = rgb(64, 224, 208);
    let white = rgb(240, 240, 240);
    text(0, 1, turquoise, "tn3270win");
    text(0, 60, turquoise, "NOT CONNECTED");
    text(2, 1, green, "Keyboard test mode");
    text(4, 1, white, "Press any key or key combination. The status line below shows");
    text(5, 1, white, "which 3270 function it is mapped to.");
    text(7, 1, white, "To change mappings: Settings > Keyboard...");
    text(9, 1, green, "Current keyboard map:");
    text(9, 23, turquoise, &map_name);

    // Operator information area (status line) under the screen.
    let line = RECT { left: x0, top: y0 + ROWS * ch + ch / 2, right: x0 + cw * COLS, bottom: y0 + ROWS * ch + ch / 2 + 1 };
    let blue = CreateSolidBrush(rgb(0, 120, 215));
    FillRect(hdc, &line, blue);
    DeleteObject(blue);
    text(ROWS + 1, 0, rgb(0, 160, 255), "4A");
    if !key.is_empty() {
        text(ROWS + 1, 4, white, &format!("Key: {key}"));
    }
    text(ROWS + 1, 34, white, &action);
    SelectObject(hdc, old);
    EndPaint(hwnd, &ps);
}
