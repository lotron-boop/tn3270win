//! Communication > Connect... dialog.
//!
//! Pick a saved session from the Session box, or fill in the fields for a
//! one-off connection. "Save As..." stores the fields as a named session;
//! "Delete" removes the selected one.

use std::cell::{Cell, RefCell};

use tn3270::{CodePage, Model};
use tn3270_keymap::Preset;

use crate::settings::{self, SessionConfig};
use crate::win32::*;
use crate::prompt;

const CLASS: &str = "Tn3270Connect";
const IDC_SESSION: i32 = 3000;
const IDC_HOST: i32 = 3001;
const IDC_PORT: i32 = 3002;
const IDC_LU: i32 = 3003;
const IDC_CODEPAGE: i32 = 3004;
const IDC_MODEL: i32 = 3005;
const IDC_KEYMAP: i32 = 3006;
const IDC_SAVE_AS: i32 = 3007;
const IDC_DELETE: i32 = 3008;
const IDC_TLS: i32 = 3009;
const BS_AUTOCHECKBOX: u32 = 3;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const CBN_SELCHANGE: u16 = 1;
const NEW_SESSION: &str = "(New session)";
const CURRENT_KEYMAP: &str = "(Current keyboard map)";
const MAX_NAME: usize = 60;

struct Controls {
    owner: HWND,
    session: HWND,
    host: HWND,
    port: HWND,
    tls: HWND,
    lu: HWND,
    codepage: HWND,
    model: HWND,
    keymap: HWND,
    delete: HWND,
    /// Names in the Session box after "(New session)".
    names: Vec<String>,
    /// Keyboard box values ("" = current map).
    keymaps: Vec<String>,
}

thread_local! {
    static DONE: Cell<bool> = const { Cell::new(false) };
    static CTRLS: RefCell<Option<Controls>> = const { RefCell::new(None) };
    static RESULT: RefCell<Option<SessionConfig>> = const { RefCell::new(None) };
    static CLASS_W: Vec<u16> = wide(CLASS);
}

fn with_ctrls<R>(f: impl FnOnce(&mut Controls) -> R) -> Option<R> {
    CTRLS.with(|c| c.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
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

/// Show the dialog; returns the settings if the user clicks Connect.
pub fn run(owner: HWND, last: &SessionConfig) -> Option<SessionConfig> {
    DONE.with(|d| d.set(false));
    RESULT.with(|r| *r.borrow_mut() = None);
    unsafe {
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex = WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT;
        let (x, y, w, h) = centered(owner, 460, 326, style, ex);
        let dlg = CLASS_W.with(|c| CreateWindowExW(ex, c.as_ptr(), wide("Connect to Host").as_ptr(), style,
            x, y, w, h, owner, null(), GetModuleHandleW(std::ptr::null()), std::ptr::null()));
        if dlg.is_null() {
            return None;
        }
        let font = ui_font();
        let label = |t: &str, x, y, w| child(dlg, "STATIC", t, SS_LEFT, 0, (x, y + 3, w, 20), -1, font);
        let combo = |x, y, w, id| child(dlg, "COMBOBOX", "", CBS_DROPDOWNLIST | WS_TABSTOP | WS_VSCROLL, 0,
            (x, y, w, 260), id, font);
        let edit = |y, w, id, extra| {
            child(dlg, "EDIT", "", ES_AUTOHSCROLL | WS_TABSTOP | extra, WS_EX_CLIENTEDGE, (124, y, w, 24), id, font)
        };
        let button = |t: &str, rect, id, extra| child(dlg, "BUTTON", t, WS_TABSTOP | extra, 0, rect, id, font);

        label("&Session:", 12, 12, 90);
        let session = combo(106, 11, 200, IDC_SESSION);
        button("Save &As...", (314, 10, 70, 25), IDC_SAVE_AS, BS_PUSHBUTTON);
        let delete = button("&Delete", (390, 10, 58, 25), IDC_DELETE, BS_PUSHBUTTON);

        child(dlg, "BUTTON", "Connection", BS_GROUPBOX, 0, (12, 46, 436, 226), -1, font);
        label("&Host name:", 24, 68, 96);
        let host = edit(68, 312, IDC_HOST, 0);
        label("&Port:", 24, 100, 96);
        let port = edit(100, 70, IDC_PORT, ES_NUMBER);
        let tls = child(dlg, "BUTTON", "Use &TLS encryption", BS_AUTOCHECKBOX | WS_TABSTOP, 0,
            (210, 100, 220, 24), IDC_TLS, font);
        label("&LU name:", 24, 132, 96);
        let lu = edit(132, 140, IDC_LU, 0);
        child(dlg, "STATIC", "(optional)", SS_LEFT, 0, (272, 135, 100, 20), -1, font);
        label("&Code page:", 24, 164, 96);
        let codepage = combo(124, 164, 312, IDC_CODEPAGE);
        for cp in CodePage::ALL {
            SendMessageW(codepage, CB_ADDSTRING, 0, wide(cp.label()).as_ptr() as LPARAM);
        }
        label("Screen si&ze:", 24, 196, 96);
        let model = combo(124, 196, 312, IDC_MODEL);
        for m in Model::ALL {
            SendMessageW(model, CB_ADDSTRING, 0, wide(&m.label()).as_ptr() as LPARAM);
        }
        label("&Keyboard:", 24, 228, 96);
        let keymap = combo(124, 228, 312, IDC_KEYMAP);
        let mut keymaps = vec![String::new()];
        keymaps.extend(Preset::ALL.iter().map(|p| p.label().to_string()));
        keymaps.extend(settings::saved_keymaps().list());
        for (i, k) in keymaps.iter().enumerate() {
            let text = if i == 0 { CURRENT_KEYMAP } else { k.as_str() };
            SendMessageW(keymap, CB_ADDSTRING, 0, wide(text).as_ptr() as LPARAM);
        }

        button("Connect", (288, 288, 76, 26), IDOK, BS_DEFPUSHBUTTON);
        button("Cancel", (372, 288, 76, 26), IDCANCEL, BS_PUSHBUTTON);

        CTRLS.with(|c| *c.borrow_mut() = Some(Controls {
            owner, session, host, port, tls, lu, codepage, model, keymap, delete, names: Vec::new(), keymaps,
        }));
        with_ctrls(|c| {
            fill_sessions(c, &last.name);
            fill_fields(c, last);
        });

        EnableWindow(owner, 0);
        ShowWindow(dlg, SW_SHOW);
        SetFocus(if last.host.is_empty() { host } else { session });
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
    CTRLS.with(|c| *c.borrow_mut() = None);
    RESULT.with(|r| r.borrow_mut().take())
}

fn combo_sel(h: HWND) -> usize {
    let i = unsafe { SendMessageW(h, CB_GETCURSEL, 0, 0) };
    usize::try_from(i).unwrap_or(0)
}

/// Refill the Session box and select `name` (or "(New session)").
fn fill_sessions(c: &mut Controls, name: &str) {
    c.names = settings::sessions().list().into_iter().map(|s| s.name).collect();
    let sel = c.names.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| i + 1).unwrap_or(0);
    unsafe {
        SendMessageW(c.session, CB_RESETCONTENT, 0, 0);
        SendMessageW(c.session, CB_ADDSTRING, 0, wide(NEW_SESSION).as_ptr() as LPARAM);
        for n in &c.names {
            SendMessageW(c.session, CB_ADDSTRING, 0, wide(n).as_ptr() as LPARAM);
        }
        SendMessageW(c.session, CB_SETCURSEL, sel, 0);
        EnableWindow(c.delete, (sel > 0) as BOOL);
    }
}

fn selected_name(c: &Controls) -> String {
    match combo_sel(c.session) {
        0 => String::new(),
        i => c.names.get(i - 1).cloned().unwrap_or_default(),
    }
}

fn fill_fields(c: &Controls, cfg: &SessionConfig) {
    unsafe {
        SetWindowTextW(c.host, wide(&cfg.host).as_ptr());
        SetWindowTextW(c.port, wide(&cfg.port.to_string()).as_ptr());
        SendMessageW(c.tls, BM_SETCHECK, cfg.tls as usize, 0);
        SetWindowTextW(c.lu, wide(&cfg.lu).as_ptr());
        let cp = CodePage::ALL.iter().position(|p| p.number() == cfg.codepage).unwrap_or(0);
        SendMessageW(c.codepage, CB_SETCURSEL, cp, 0);
        let m = Model::ALL.iter().position(|m| m.number() == cfg.model).unwrap_or(0);
        SendMessageW(c.model, CB_SETCURSEL, m, 0);
        let k = c.keymaps.iter().position(|k| k.eq_ignore_ascii_case(&cfg.keymap)).unwrap_or(0);
        SendMessageW(c.keymap, CB_SETCURSEL, k, 0);
    }
}

/// Read and check the fields. On a problem, says so and returns None.
fn read_fields(dlg: HWND) -> Option<SessionConfig> {
    let (cfg, port_text, host_h, port_h) = with_ctrls(|c| {
        let cfg = SessionConfig {
            name: selected_name(c),
            host: window_text(c.host).trim().to_string(),
            port: 0,
            lu: window_text(c.lu).trim().to_uppercase(),
            tls: unsafe { SendMessageW(c.tls, BM_GETCHECK, 0, 0) } == 1,
            codepage: CodePage::ALL.get(combo_sel(c.codepage)).copied().unwrap_or_default().number(),
            model: Model::ALL.get(combo_sel(c.model)).copied().unwrap_or_default().number(),
            keymap: c.keymaps.get(combo_sel(c.keymap)).cloned().unwrap_or_default(),
        };
        (cfg, window_text(c.port), c.host, c.port)
    })?;
    if cfg.host.is_empty() {
        message_box(dlg, "Please enter the host name or IP address.", "Connect", MB_OK | MB_ICONWARNING);
        unsafe { SetFocus(host_h) };
        return None;
    }
    let Some(port) = port_text.trim().parse::<u16>().ok().filter(|&p| p > 0) else {
        message_box(dlg, "Please enter a port number between 1 and 65535.", "Connect", MB_OK | MB_ICONWARNING);
        unsafe { SetFocus(port_h) };
        return None;
    };
    Some(SessionConfig { port, ..cfg })
}

fn on_session_changed() {
    with_ctrls(|c| {
        let name = selected_name(c);
        unsafe { EnableWindow(c.delete, (!name.is_empty()) as BOOL) };
        if !name.is_empty() {
            if let Some(cfg) = settings::sessions().list().into_iter().find(|s| s.name == name) {
                fill_fields(c, &cfg);
            }
        }
    });
}

/// Ticking TLS moves the port between the usual 23 and 992.
fn on_tls_clicked() {
    with_ctrls(|c| {
        let on = unsafe { SendMessageW(c.tls, BM_GETCHECK, 0, 0) } == 1;
        let port = window_text(c.port);
        let new_port = match (on, port.trim()) {
            (true, "23") | (true, "") => Some("992"),
            (false, "992") => Some("23"),
            _ => None,
        };
        if let Some(p) = new_port {
            unsafe { SetWindowTextW(c.port, wide(p).as_ptr()) };
        }
    });
}

fn on_save_as(dlg: HWND) {
    let Some(mut cfg) = read_fields(dlg) else { return };
    let mut suggestion = cfg.name.clone();
    loop {
        let Some(name) = prompt::ask(dlg, "Save Session", "Save these settings as a session named:",
            &suggestion, MAX_NAME) else { return };
        if name.is_empty() || name.eq_ignore_ascii_case(NEW_SESSION) {
            message_box(dlg, "Please enter a name.", "Save Session", MB_OK | MB_ICONWARNING);
            continue;
        }
        let store = settings::sessions();
        if store.exists(&name) {
            let q = format!("A session named \u{201C}{name}\u{201D} already exists.\n\nReplace it?");
            if message_box(dlg, &q, "Save Session", MB_YESNO | MB_ICONQUESTION) != IDYES {
                suggestion = name;
                continue;
            }
        }
        cfg.name = name.clone();
        match store.save(&cfg) {
            Ok(()) => {
                with_ctrls(|c| fill_sessions(c, &name));
            }
            Err(e) => {
                message_box(dlg, &format!("The session could not be saved:\n{e}"), "Save Session",
                    MB_OK | MB_ICONERROR);
            }
        }
        return;
    }
}

fn on_delete(dlg: HWND) {
    let Some(name) = with_ctrls(|c| selected_name(c)).filter(|n| !n.is_empty()) else { return };
    let q = format!("Delete the saved session \u{201C}{name}\u{201D}?");
    if message_box(dlg, &q, "Connect", MB_YESNO | MB_ICONWARNING) != IDYES {
        return;
    }
    if let Err(e) = settings::sessions().delete(&name) {
        message_box(dlg, &format!("The session could not be deleted:\n{e}"), "Connect", MB_OK | MB_ICONERROR);
        return;
    }
    with_ctrls(|c| fill_sessions(c, ""));
}

fn close(dlg: HWND, result: Option<SessionConfig>) {
    RESULT.with(|r| *r.borrow_mut() = result);
    let owner = with_ctrls(|c| c.owner);
    unsafe {
        if let Some(o) = owner {
            EnableWindow(o, 1);
        }
        DestroyWindow(dlg);
    }
    DONE.with(|d| d.set(true));
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match (loword(wp) as i32, hiword(wp)) {
                (IDOK, _) => {
                    if let Some(cfg) = read_fields(hwnd) {
                        close(hwnd, Some(cfg));
                    }
                }
                (IDCANCEL, _) => close(hwnd, None),
                (IDC_SESSION, CBN_SELCHANGE) => on_session_changed(),
                (IDC_SAVE_AS, BN_CLICKED) => on_save_as(hwnd),
                (IDC_DELETE, BN_CLICKED) => on_delete(hwnd),
                (IDC_TLS, BN_CLICKED) => on_tls_clicked(),
                _ => {}
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            SetBkColor(wp as HDC, GetSysColor(COLOR_BTNFACE));
            GetSysColorBrush(COLOR_BTNFACE) as LRESULT
        }
        WM_CLOSE => {
            close(hwnd, None);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
