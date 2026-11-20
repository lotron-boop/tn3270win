//! tn3270win: a TN3270 terminal emulator for Windows.
//!
//! Communication > Connect... opens a session (saved or one-off). Keys go
//! through the keyboard map (Settings > Keyboard..., or the session's own
//! preset). When not connected, the window is in keyboard test mode: the
//! status line shows what each key is mapped to.
//!
//! Mouse: a click places the cursor. Edit > Select Text switches to
//! selecting: drag a box, then Edit > Copy.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod capture;
mod connectdlg;
mod keydlg;
mod prompt;
mod settings;
mod tls;
mod win32;

use std::cell::RefCell;

use tn3270::net::{Connection, NetEvent, TrustAnswer, TrustPrompt};
use tn3270::screen::{Aid, Color, InputError, Lock};
use tn3270::{CodePage, Model, Session, SessionState};
use tn3270_keymap::{Action, KeyDispatcher, Keymap, Preset};

use settings::ViewConfig;

use win32::*;

const ID_EXIT: usize = 100;
const ID_COPY: usize = 110;
const ID_PASTE: usize = 111;
const ID_SELECT: usize = 112;
const ID_COPY_SCREEN: usize = 113;
const ID_FIT: usize = 300;
const ID_LARGER: usize = 301;
const ID_SMALLER: usize = 302;
const ID_CONNECT: usize = 120;
const ID_DISCONNECT: usize = 121;
const ID_KEYBOARD: usize = 200;
const ID_SESSION: usize = 201;
const ID_APPEARANCE: usize = 202;
const ID_ABOUT: usize = 900;
const WM_APP_NET: u32 = WM_APP + 1;

/// Size shown when not connected.
const IDLE_DIMS: (i32, i32) = (24, 80);

type RowCol = (i32, i32);

struct App {
    hwnd: HWND,
    menu: HMENU,
    keymap: Keymap,
    dispatcher: KeyDispatcher,
    suppress_char: bool,
    // Key test mode display
    last_key: String,
    last_action: String,
    // Session
    session: Option<Session>,
    conn: Option<Connection>,
    status: String,
    host_label: String,
    /// "TLS" once the connection is encrypted.
    security: Option<String>,
    /// The session's keyboard preset, used instead of `keymap` while connected.
    session_keymap: Option<Keymap>,
    // Selection (Edit > Select Text)
    select_mode: bool,
    selection: Option<(RowCol, RowCol)>,
    dragging: bool,
    // Layout
    view: ViewConfig,
    font: HFONT,
    font_height: i32,
    /// Screen size the font was fitted for.
    fitted_dims: RowCol,
    cell: (i32, i32),
    origin: (i32, i32),
}

impl App {
    fn active_keymap(&self) -> &Keymap {
        self.session_keymap.as_ref().unwrap_or(&self.keymap)
    }

    /// (rows, cols) of what's on screen.
    fn dims(&self) -> RowCol {
        self.session.as_ref().map(|s| (s.screen.rows as i32, s.screen.cols as i32)).unwrap_or(IDLE_DIMS)
    }

    fn clear_selection(&mut self) {
        self.selection = None;
        self.select_mode = false;
        self.dragging = false;
    }

    /// The cell under a client-area point, clamped to the screen.
    fn cell_at(&self, x: i32, y: i32) -> RowCol {
        let (rows, cols) = self.dims();
        let (cw, ch) = (self.cell.0.max(1), self.cell.1.max(1));
        let col = ((x - self.origin.0).div_euclid(cw)).clamp(0, cols - 1);
        let row = ((y - self.origin.1).div_euclid(ch)).clamp(0, rows - 1);
        (row, col)
    }

    /// Text of the selected block, or the whole screen.
    fn copy_text(&self) -> Option<String> {
        let s = self.session.as_ref()?;
        let Some(((r0, c0), (r1, c1))) = self.selection else { return Some(s.screen.text()) };
        let (top, bottom, left, right) = (r0.min(r1), r0.max(r1), c0.min(c1), c0.max(c1));
        let glyphs = s.screen.glyphs();
        let cols = s.screen.cols;
        let lines: Vec<String> = (top..=bottom)
            .map(|r| {
                (left..=right).map(|c| glyphs[r as usize * cols + c as usize].ch).collect::<String>()
                    .trim_end().to_string()
            })
            .collect();
        Some(lines.join("\r\n"))
    }
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
    prompt::register();
    connectdlg::register();

    let (keymap, load_error) = settings::load_keymap();
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

    let menu = build_menu();
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            hwnd: null(),
            menu,
            keymap,
            dispatcher: KeyDispatcher::new(),
            suppress_char: false,
            last_key: String::new(),
            last_action: String::new(),
            session: None,
            conn: None,
            status: "Not connected".into(),
            host_label: String::new(),
            security: None,
            session_keymap: None,
            select_mode: false,
            selection: None,
            dragging: false,
            view: settings::load_view(),
            font: null(),
            font_height: 0,
            fitted_dims: (0, 0),
            cell: (9, 18),
            origin: (0, 0),
        })
    });

    let hwnd = CreateWindowExW(0, class.as_ptr(), wide("tn3270win").as_ptr(),
        WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, 1000, 660,
        null(), menu, instance, std::ptr::null());
    with_app(|a| a.hwnd = hwnd);
    fit_font(hwnd);
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
    popup("&Edit", &[
        (ID_SELECT, "&Select Text", true),
        (0, "", true),
        (ID_COPY, "&Copy", true),
        (ID_COPY_SCREEN, "Copy Scree&n", true),
        (ID_PASTE, "&Paste", true),
    ]);
    popup("&View", &[
        (ID_FIT, "&Fit Font to Window", true),
        (0, "", true),
        (ID_LARGER, "&Larger Font", true),
        (ID_SMALLER, "&Smaller Font", true),
    ]);
    popup("&Communication", &[(ID_CONNECT, "&Connect...", true), (ID_DISCONNECT, "&Disconnect", false)]);
    popup("&Settings", &[
        (ID_KEYBOARD, "&Keyboard...", true),
        (0, "", true),
        (ID_SESSION, "&Session...", false),
        (ID_APPEARANCE, "&Appearance...", false),
    ]);
    popup("&Help", &[(ID_ABOUT, "&About tn3270win", true)]);
    bar
}

fn invalidate(hwnd: HWND) {
    unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
}

// ---- session ---------------------------------------------------------------

fn connect(hwnd: HWND) {
    let last = settings::load_last_session();
    let Some(cfg) = connectdlg::run(hwnd, &last) else { return };
    let _ = settings::save_last_session(&cfg);
    let codepage = CodePage::from_number(cfg.codepage).unwrap_or_default();
    let model = Model::from_number(cfg.model).unwrap_or_default();
    let lu = (!cfg.lu.is_empty()).then(|| cfg.lu.clone());

    // The session's keyboard preset, if it names one.
    let session_keymap = if cfg.keymap.is_empty() {
        None
    } else if let Some(p) = Preset::ALL.iter().find(|p| p.label() == cfg.keymap) {
        Some(Keymap::preset(*p))
    } else {
        match settings::saved_keymaps().load(&cfg.keymap) {
            Ok(km) => Some(km),
            Err(e) => {
                message_box(hwnd, &format!("The keyboard preset \u{201C}{}\u{201D} could not be loaded ({e}).\nThe current keyboard map will be used.", cfg.keymap),
                    "Connect", MB_OK | MB_ICONWARNING);
                None
            }
        }
    };

    let notify_hwnd = hwnd as isize;
    let wrapper = cfg.tls.then(|| tls::wrapper(cfg.host.clone(), cfg.port));
    let conn = Connection::open_with(&cfg.host, cfg.port, wrapper, move || unsafe {
        PostMessageW(notify_hwnd as HWND, WM_APP_NET, 0, 0);
    });
    let title = if cfg.name.is_empty() { cfg.host.clone() } else { cfg.name.clone() };
    with_app(|a| {
        a.conn = Some(conn); // drops (closes) any previous connection
        a.session = Some(Session::with_model(model, codepage, lu));
        a.session_keymap = session_keymap;
        a.dispatcher.cancel();
        a.clear_selection();
        a.host_label = format!("{}:{}", cfg.host, cfg.port);
        a.security = None;
        a.status = format!("Connecting to {}...", a.host_label);
    });
    unsafe { SetWindowTextW(hwnd, wide(&format!("tn3270win - {title}")).as_ptr()) };
    fit_font(hwnd);
    invalidate(hwnd);
}

fn disconnect(hwnd: HWND, status: String) {
    with_app(|a| {
        if let Some(c) = a.conn.take() {
            c.close();
        }
        a.session = None;
        a.session_keymap = None;
        a.security = None;
        a.dispatcher.cancel();
        a.clear_selection();
        a.status = status;
    });
    unsafe { SetWindowTextW(hwnd, wide("tn3270win").as_ptr()) };
    fit_font(hwnd);
    invalidate(hwnd);
}

/// Send whatever the session has queued.
fn flush(a: &mut App) {
    let (Some(s), Some(c)) = (a.session.as_mut(), a.conn.as_ref()) else { return };
    let out = s.take_output();
    if let Err(e) = c.send(&out) {
        a.status = format!("Send failed: {e}");
    }
}

fn on_net(hwnd: HWND) {
    let mut closed: Option<Option<String>> = None;
    let mut beep = false;
    let mut prompt: Option<TrustPrompt> = None;
    with_app(|a| {
        while let Some(ev) = a.conn.as_ref().and_then(|c| c.try_recv()) {
            match ev {
                NetEvent::Connected => a.status = format!("Connected to {}, negotiating...", a.host_label),
                NetEvent::Secured(how) => {
                    a.security = Some(how);
                    a.status = format!("Connected to {}, negotiating...", a.host_label);
                }
                NetEvent::Prompt(p) => {
                    a.status = "Waiting for you to check the host's certificate...".into();
                    prompt = Some(p);
                    break; // ask outside this borrow
                }
                NetEvent::Data(d) => {
                    if let Some(s) = a.session.as_mut() {
                        s.receive(&d);
                        if let SessionState::Ready { tn3270e, device_type, lu } = &s.state {
                            a.status = format!("{}{} {}{}",
                                a.security.as_deref().map(|t| format!("{t} ")).unwrap_or_default(),
                                if *tn3270e { "TN3270E" } else { "TN3270" },
                                device_type, lu.as_ref().map(|l| format!("  LU {l}")).unwrap_or_default());
                        }
                        beep |= std::mem::take(&mut s.screen.alarm);
                    }
                    flush(a);
                }
                NetEvent::Closed(err) => {
                    closed = Some(err);
                    break;
                }
            }
        }
    });
    if beep {
        unsafe { MessageBeep(MB_BEEP) };
    }
    if let Some(p) = prompt {
        let answer = ask_trust(hwnd, &p);
        let _ = p.reply.send(answer);
        invalidate(hwnd);
        return;
    }
    // Erase/Write Alternate may have changed the screen size.
    if with_app(|a| a.dims() != a.fitted_dims).unwrap_or(false) {
        fit_font(hwnd);
    }
    match closed {
        Some(None) => disconnect(hwnd, "Disconnected by host".into()),
        Some(Some(err)) => {
            disconnect(hwnd, "Not connected".into());
            message_box(hwnd, &err, "Connect", MB_OK | MB_ICONWARNING);
        }
        None => invalidate(hwnd),
    }
}

/// Ask whether to trust a certificate Windows doesn't.
fn ask_trust(hwnd: HWND, p: &TrustPrompt) -> TrustAnswer {
    let headline = if p.changed {
        format!("WARNING: the certificate for {}:{} has CHANGED since you last trusted it.\n\n\
            If you didn't replace the host's certificate yourself, someone may be \
            intercepting the connection. Don't connect unless you know why it changed.", p.host, p.port)
    } else {
        format!("Windows doesn't trust the certificate from {}:{}.", p.host, p.port)
    };
    let text = format!(
        "{headline}\n\n{}\n\nIssued to:\t{}\nIssued by:\t{}\nExpires:\t{}\n\nSHA-256 fingerprint:\n{}\n\n\
        To check it, run this on the host and compare:\n\
        openssl x509 -in tk5-cert.pem -noout -fingerprint -sha256\n\n\
        Yes = trust this certificate from now on\nNo = connect this time only\nCancel = don't connect",
        p.problem, p.subject, p.issuer, p.expires, p.fingerprint);
    let icon = if p.changed { MB_ICONWARNING } else { MB_ICONQUESTION };
    match message_box(hwnd, &text, "Check Host Certificate", MB_YESNOCANCEL | icon | MB_DEFBUTTON3) {
        IDYES => TrustAnswer::Always,
        IDNO => TrustAnswer::Once,
        _ => TrustAnswer::Reject,
    }
}

/// Do a mapped action on the live session.
fn perform(a: &mut App, action: &Action) -> Result<(), InputError> {
    let hwnd = a.hwnd;
    if matches!(action, Action::Copy | Action::Cut) {
        copy(a);
        return Ok(());
    }
    a.clear_selection();
    let Some(s) = a.session.as_mut() else { return Ok(()) };
    let r = match action {
        Action::Enter => s.aid(Aid::Enter),
        Action::Clear => s.aid(Aid::Clear),
        Action::Pf(n) => s.aid(Aid::Pf(*n)),
        Action::Pa(n) => s.aid(Aid::Pa(*n)),
        Action::SysReq => {
            a.status = "SysReq is not supported yet".into();
            Ok(())
        }
        Action::Attn => {
            s.attn();
            Ok(())
        }
        Action::Reset => {
            s.screen.reset();
            Ok(())
        }
        Action::Tab => { s.screen.tab(); Ok(()) }
        Action::BackTab => { s.screen.back_tab(); Ok(()) }
        Action::NewLine => { s.screen.new_line(); Ok(()) }
        Action::Home => { s.screen.home(); Ok(()) }
        Action::EndOfField => { s.screen.end_of_field(); Ok(()) }
        Action::Up => { s.screen.move_cursor(-1, 0); Ok(()) }
        Action::Down => { s.screen.move_cursor(1, 0); Ok(()) }
        Action::Left => { s.screen.move_cursor(0, -1); Ok(()) }
        Action::Right => { s.screen.move_cursor(0, 1); Ok(()) }
        Action::WordLeft => { s.screen.word_left(); Ok(()) }
        Action::WordRight => { s.screen.word_right(); Ok(()) }
        Action::Backspace => s.screen.backspace(),
        Action::Delete => s.screen.delete(),
        Action::Insert => { s.screen.toggle_insert(); Ok(()) }
        Action::EraseEof => s.screen.erase_eof(),
        Action::EraseInput => s.screen.erase_input(),
        Action::FieldMark => s.screen.field_mark(),
        Action::Dup => s.screen.dup(),
        Action::Copy | Action::Cut => Ok(()), // handled above
        Action::Paste => paste(hwnd, s),
        Action::PrintScreen => {
            a.status = "Printing is not available yet".into();
            Ok(())
        }
        Action::Text(t) => t.chars().try_for_each(|c| s.screen.type_char(c)),
        Action::Macro(_) => {
            a.status = "Macros are not available yet".into();
            Ok(())
        }
    };
    flush(a);
    r
}

/// Copy the selection (or the whole screen) and leave select mode.
fn copy(a: &mut App) {
    if let Some(text) = a.copy_text() {
        set_clipboard(a.hwnd, &text);
    }
    a.clear_selection();
}

fn paste(hwnd: HWND, s: &mut Session) -> Result<(), InputError> {
    let Some(text) = get_clipboard(hwnd) else { return Ok(()) };
    for c in text.chars() {
        match c {
            '\r' => {}
            '\n' => s.screen.new_line(),
            '\t' => s.screen.tab(),
            c if c.is_control() => {}
            c => s.screen.type_char(c)?,
        }
    }
    Ok(())
}

fn connected(a: &App) -> bool {
    a.session.is_some()
}

/// A key or character was handled: repaint, and beep on operator errors.
fn after_input(hwnd: HWND, r: Result<(), InputError>) {
    if matches!(r, Err(e) if e != InputError::Locked) {
        unsafe { MessageBeep(MB_BEEP) };
    }
    invalidate(hwnd);
}

// ---- window ------------------------------------------------------------------

unsafe extern "system" fn main_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let chord = chord_from_msg(wp, lp);
            let action = with_app(|a| {
                a.suppress_char = false;
                let km = a.session_keymap.as_ref().unwrap_or(&a.keymap);
                let action = a.dispatcher.key_down(km, chord, is_repeat(lp));
                a.suppress_char = action.is_some();
                action
            }).flatten();
            match action {
                Some(action) => {
                    let r = with_app(|a| {
                        if connected(a) {
                            perform(a, &action)
                        } else {
                            a.last_key = chord.to_string();
                            a.last_action = action.description();
                            Ok(())
                        }
                    }).unwrap_or(Ok(()));
                    after_input(hwnd, r);
                    0
                }
                None => {
                    if !chord.is_modifier_key() {
                        with_app(|a| {
                            if !connected(a) {
                                a.last_key = chord.to_string();
                                a.last_action = "(not mapped - types normally)".into();
                            }
                        });
                        invalidate(hwnd);
                    }
                    if msg == WM_SYSKEYDOWN { DefWindowProcW(hwnd, msg, wp, lp) } else { 0 }
                }
            }
        }
        WM_KEYUP | WM_SYSKEYUP => {
            let chord = chord_from_msg(wp, lp);
            match with_app(|a| {
                let km = a.session_keymap.as_ref().unwrap_or(&a.keymap);
                a.dispatcher.key_up(km, chord)
            }).flatten() {
                Some(action) => {
                    let r = with_app(|a| {
                        if connected(a) {
                            perform(a, &action)
                        } else {
                            a.last_key = chord.to_string();
                            a.last_action = action.description();
                            Ok(())
                        }
                    }).unwrap_or(Ok(()));
                    after_input(hwnd, r);
                    0 // also stops a lone Alt from opening the menu bar
                }
                None if msg == WM_SYSKEYUP => DefWindowProcW(hwnd, msg, wp, lp),
                None => 0,
            }
        }
        WM_CHAR | WM_SYSCHAR => {
            let suppress = with_app(|a| std::mem::take(&mut a.suppress_char)).unwrap_or(false);
            if suppress {
                return 0;
            }
            if msg == WM_SYSCHAR {
                return DefWindowProcW(hwnd, msg, wp, lp); // menu mnemonics such as Alt+S
            }
            if let Some(c) = char::from_u32(wp as u32).filter(|c| !c.is_control()) {
                let r = with_app(|a| {
                    a.clear_selection();
                    match a.session.as_mut() {
                        Some(s) => s.screen.type_char(c),
                        None => Ok(()),
                    }
                }).unwrap_or(Ok(()));
                after_input(hwnd, r);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
            let capture = with_app(|a| {
                if a.session.is_none() {
                    return false;
                }
                let rc = a.cell_at(x, y);
                if a.select_mode {
                    a.selection = Some((rc, rc));
                    a.dragging = true;
                    true
                } else {
                    a.selection = None;
                    let cols = a.dims().1;
                    if let Some(s) = a.session.as_mut() {
                        s.screen.cursor = (rc.0 * cols + rc.1) as usize;
                    }
                    false
                }
            }).unwrap_or(false);
            if capture {
                SetCapture(hwnd);
            }
            invalidate(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32);
            let moved = with_app(|a| {
                if !a.dragging {
                    return false;
                }
                let rc = a.cell_at(x, y);
                match a.selection.as_mut() {
                    Some(sel) if sel.1 != rc => {
                        sel.1 = rc;
                        true
                    }
                    _ => false,
                }
            }).unwrap_or(false);
            if moved {
                invalidate(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            if with_app(|a| std::mem::take(&mut a.dragging)).unwrap_or(false) {
                ReleaseCapture();
            }
            0
        }
        WM_SETCURSOR => {
            let select = with_app(|a| a.select_mode).unwrap_or(false);
            if select && loword(lp as usize) == HTCLIENT {
                SetCursor(LoadCursorW(null(), IDC_CROSS));
                return 1;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_ACTIVATE => {
            with_app(|a| a.dispatcher.cancel());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_APP_NET => {
            on_net(hwnd);
            0
        }
        WM_INITMENUPOPUP => {
            if let Some((menu, on, select, fit)) = with_app(|a| (a.menu, a.conn.is_some(), a.select_mode, a.view.fit)) {
                let enable = |id: usize, yes: bool| EnableMenuItem(menu, id as u32, MF_BYCOMMAND | if yes { MF_ENABLED } else { MF_GRAYED });
                let check = |id: usize, yes: bool| CheckMenuItem(menu, id as u32, MF_BYCOMMAND | if yes { MF_CHECKED } else { MF_UNCHECKED });
                enable(ID_DISCONNECT, on);
                enable(ID_SELECT, on);
                enable(ID_COPY_SCREEN, on);
                check(ID_SELECT, select);
                check(ID_FIT, fit);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_COMMAND => {
            match loword(wp) as usize {
                ID_EXIT => { DestroyWindow(hwnd); }
                ID_CONNECT => connect(hwnd),
                ID_DISCONNECT => disconnect(hwnd, "Not connected".into()),
                ID_COPY => {
                    with_app(copy);
                    invalidate(hwnd);
                }
                ID_COPY_SCREEN => {
                    with_app(|a| {
                        if let Some(s) = a.session.as_ref() {
                            set_clipboard(hwnd, &s.screen.text());
                        }
                    });
                }
                ID_SELECT => {
                    with_app(|a| {
                        if a.select_mode {
                            a.clear_selection();
                        } else if a.session.is_some() {
                            a.select_mode = true;
                            a.selection = None;
                        }
                    });
                    invalidate(hwnd);
                }
                ID_FIT | ID_LARGER | ID_SMALLER => {
                    let id = loword(wp) as usize;
                    with_app(|a| {
                        match id {
                            ID_FIT => a.view.fit = !a.view.fit,
                            _ => {
                                // Start from the size on screen, then step.
                                let step = if id == ID_LARGER { 1 } else { -1 };
                                a.view.fit = false;
                                a.view.font_height = (a.font_height + step).clamp(8, 72);
                            }
                        }
                        let _ = settings::save_view(&a.view);
                        a.font_height = 0; // force the font to be rebuilt
                    });
                    fit_font(hwnd);
                    invalidate(hwnd);
                }
                ID_PASTE => {
                    let r = with_app(|a| {
                        let r = match a.session.as_mut() { Some(s) => paste(hwnd, s), None => Ok(()) };
                        flush(a);
                        r
                    }).unwrap_or(Ok(()));
                    after_input(hwnd, r);
                }
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
        WM_SIZE => {
            fit_font(hwnd);
            invalidate(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_DESTROY => {
            with_app(|a| a.conn.take());
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn open_keyboard_setup(hwnd: HWND) {
    let Some(current) = with_app(|app| app.active_keymap().clone()) else { return };
    if let Some(new_map) = keydlg::run(hwnd, current) {
        if let Err(e) = settings::save_keymap(&new_map) {
            message_box(hwnd, &format!("The keyboard map could not be saved:\n{e}"),
                "tn3270win", MB_OK | MB_ICONERROR);
        }
        with_app(|app| {
            app.keymap = new_map;
            app.session_keymap = None; // what you just set up is what's in effect
            app.dispatcher.cancel();
            app.last_key.clear();
            app.last_action = "Keyboard map updated".into();
        });
        invalidate(hwnd);
    }
}

// ---- drawing -------------------------------------------------------------------

fn make_font(height: i32) -> HFONT {
    unsafe {
        CreateFontW(height, 0, 0, 0, FW_NORMAL, 0, 0, 0, DEFAULT_CHARSET, 0, 0,
            CLEARTYPE_QUALITY, FIXED_PITCH, wide("Consolas").as_ptr())
    }
}

fn measure(hdc: HDC, font: HFONT) -> (i32, i32) {
    unsafe {
        let old = SelectObject(hdc, font);
        let mut sz = SIZE::default();
        let m: Vec<u16> = "M".encode_utf16().collect();
        GetTextExtentPoint32W(hdc, m.as_ptr(), 1, &mut sz);
        SelectObject(hdc, old);
        (sz.cx, sz.cy)
    }
}

/// Choose the screen font: with View > Fit Font to Window, the largest
/// Consolas size at which the whole screen and status line fit; otherwise
/// the size chosen with Larger/Smaller Font.
fn fit_font(hwnd: HWND) {
    let Some((view, (rows, cols))) = with_app(|a| (a.view, a.dims())) else { return };
    unsafe {
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - 16, rc.bottom - 8);
        let hdc = GetDC(hwnd);
        let mut chosen: Option<(HFONT, i32, (i32, i32))> = None;
        if view.fit && w > 0 && h > 0 {
            let mut height = (h / (rows + 2)).clamp(8, 72);
            while height >= 8 {
                let font = make_font(height);
                let cell = measure(hdc, font);
                if cell.0 * cols <= w && cell.1 * (rows + 2) <= h {
                    chosen = Some((font, height, cell));
                    break;
                }
                DeleteObject(font);
                height -= 1;
            }
        }
        if chosen.is_none() {
            let height = if view.fit { 8 } else { view.font_height };
            let font = make_font(height);
            chosen = Some((font, height, measure(hdc, font)));
        }
        ReleaseDC(hwnd, hdc);
        if let Some((font, height, cell)) = chosen {
            with_app(|a| {
                a.fitted_dims = (rows, cols);
                if a.font_height == height && !a.font.is_null() {
                    DeleteObject(font);
                    return;
                }
                if !a.font.is_null() {
                    DeleteObject(a.font);
                }
                a.font = font;
                a.font_height = height;
                a.cell = cell;
            });
        }
    }
}

fn rgb_of(c: Color) -> u32 {
    match c {
        Color::Blue => rgb(80, 160, 255),
        Color::Red => rgb(255, 80, 80),
        Color::Pink => rgb(255, 120, 255),
        Color::Green => rgb(80, 255, 80),
        Color::Turquoise => rgb(64, 224, 224),
        Color::Yellow => rgb(255, 255, 80),
        Color::White => rgb(255, 255, 255),
        Color::Black => rgb(0, 0, 0),
        Color::DeepBlue => rgb(64, 96, 255),
        Color::Orange => rgb(255, 160, 64),
        Color::Purple => rgb(176, 96, 255),
        Color::PaleGreen => rgb(160, 255, 160),
        Color::PaleTurquoise => rgb(160, 255, 255),
        Color::Grey => rgb(160, 160, 160),
    }
}

unsafe fn fill(hdc: HDC, r: RECT, color: u32) {
    let b = CreateSolidBrush(color);
    FillRect(hdc, &r, b);
    DeleteObject(b);
}

unsafe fn text_at(hdc: HDC, x: i32, y: i32, color: u32, s: &str) {
    SetTextColor(hdc, color);
    let w: Vec<u16> = s.encode_utf16().collect();
    TextOutW(hdc, x, y, w.as_ptr(), w.len() as i32);
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    GetClientRect(hwnd, &mut rc);
    // Draw off-screen, then copy in one go (no flicker).
    let mem = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, rc.right.max(1), rc.bottom.max(1));
    let old_bmp = SelectObject(mem, bmp);
    fill(mem, rc, rgb(0, 0, 0));
    with_app(|a| draw(a, mem, rc));
    BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
    SelectObject(mem, old_bmp);
    DeleteObject(bmp);
    DeleteDC(mem);
    EndPaint(hwnd, &ps);
}

unsafe fn draw(a: &mut App, hdc: HDC, rc: RECT) {
    let old_font = SelectObject(hdc, a.font);
    let (cw, ch) = a.cell;
    let (rows, cols) = a.dims();
    let x0 = ((rc.right - cw * cols) / 2).max(4);
    let y0 = ((rc.bottom - ch * (rows + 2)) / 2).max(4);
    a.origin = (x0, y0);
    SetBkMode(hdc, TRANSPARENT);
    let white = rgb(240, 240, 240);
    let oia_y = y0 + rows * ch + ch / 2 + 2;

    match a.session.as_ref() {
        Some(s) => {
            let scr = &s.screen;
            let glyphs = scr.glyphs();
            for row in 0..rows as usize {
                let y = y0 + row as i32 * ch;
                let mut col = 0;
                while col < cols as usize {
                    let g = glyphs[row * cols as usize + col];
                    let mut run = String::new();
                    let start = col;
                    while col < cols as usize {
                        let h = glyphs[row * cols as usize + col];
                        if (h.color, h.reverse, h.underline) != (g.color, g.reverse, g.underline) {
                            break;
                        }
                        run.push(h.ch);
                        col += 1;
                    }
                    let x = x0 + start as i32 * cw;
                    let width = (col - start) as i32 * cw;
                    let color = rgb_of(g.color);
                    if g.reverse {
                        fill(hdc, RECT { left: x, top: y, right: x + width, bottom: y + ch }, color);
                        text_at(hdc, x, y, rgb(0, 0, 0), &run);
                    } else if !run.trim().is_empty() {
                        text_at(hdc, x, y, color, &run);
                    }
                    if g.underline {
                        fill(hdc, RECT { left: x, top: y + ch - 2, right: x + width, bottom: y + ch - 1 }, color);
                    }
                }
            }
            // Cursor: underline, or a half block in insert mode.
            let (crow, ccol) = ((scr.cursor / cols as usize) as i32, (scr.cursor % cols as usize) as i32);
            let cx = x0 + ccol * cw;
            let cy = y0 + crow * ch;
            let top = if scr.insert { cy + ch / 2 } else { cy + ch - (ch / 6).max(2) };
            InvertRect(hdc, &RECT { left: cx, top, right: cx + cw, bottom: cy + ch });

            // Status line
            let kb = match scr.keyboard {
                Lock::Unlocked => String::new(),
                Lock::Wait => "X SYSTEM".into(),
                Lock::Error(e) => e.oia_text().into(),
            };
            text_at(hdc, x0, oia_y, rgb(0, 160, 255), "4A");
            text_at(hdc, x0 + 4 * cw, oia_y, white, &a.status);
            text_at(hdc, x0 + 46 * cw, oia_y, rgb(255, 255, 80), &kb);
            if scr.insert {
                text_at(hdc, x0 + 64 * cw, oia_y, white, "INS");
            }
            text_at(hdc, x0 + (cols - 8) * cw, oia_y, white, &format!("{:03}/{:03}", crow + 1, ccol + 1));
            if a.select_mode {
                text_at(hdc, x0 + 56 * cw, oia_y, rgb(255, 160, 64), "SELECT");
            }
            // Selected block, shown inverted.
            if let Some(((r0, c0), (r1, c1))) = a.selection {
                let sel = RECT {
                    left: x0 + c0.min(c1) * cw,
                    top: y0 + r0.min(r1) * ch,
                    right: x0 + (c0.max(c1) + 1) * cw,
                    bottom: y0 + (r0.max(r1) + 1) * ch,
                };
                InvertRect(hdc, &sel);
            }
        }
        None => {
            let green = rgb(0, 255, 0);
            let turquoise = rgb(64, 224, 208);
            let line = |row: i32, col: i32, color: u32, s: &str| text_at(hdc, x0 + col * cw, y0 + row * ch, color, s);
            line(0, 1, turquoise, "tn3270win");
            line(0, 60, turquoise, "NOT CONNECTED");
            line(2, 1, green, "Communication > Connect... to open a session.");
            line(4, 1, white, "Keyboard test mode: press any key or key combination and the status");
            line(5, 1, white, "line shows which 3270 function it is mapped to.");
            line(7, 1, white, "To change mappings: Settings > Keyboard...");
            line(9, 1, green, "Current keyboard map:");
            line(9, 23, turquoise, &a.keymap.name);
            text_at(hdc, x0, oia_y, rgb(0, 160, 255), "4A");
            if a.last_key.is_empty() {
                text_at(hdc, x0 + 4 * cw, oia_y, white, &a.status);
            } else {
                text_at(hdc, x0 + 4 * cw, oia_y, white, &format!("Key: {}", a.last_key));
            }
            text_at(hdc, x0 + 34 * cw, oia_y, white, &a.last_action);
        }
    }
    fill(hdc, RECT { left: x0, top: oia_y - ch / 4 - 2, right: x0 + cw * cols, bottom: oia_y - ch / 4 - 1 }, rgb(0, 120, 215));
    SelectObject(hdc, old_font);
}
