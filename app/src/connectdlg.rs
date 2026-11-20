//! Communication > Connect... dialog: host, port, LU name, code page.

use std::cell::{Cell, RefCell};

use tn3270::CodePage;

use crate::settings::SessionConfig;
use crate::win32::*;

const CLASS: &str = "Tn3270Connect";
const IDC_HOST: i32 = 3001;
const IDC_PORT: i32 = 3002;
const IDC_LU: i32 = 3003;
const IDC_CODEPAGE: i32 = 3004;

struct Controls {
    owner: HWND,
    host: HWND,
    port: HWND,
    lu: HWND,
    codepage: HWND,
}

thread_local! {
    static DONE: Cell<bool> = const { Cell::new(false) };
    static CTRLS: RefCell<Option<Controls>> = const { RefCell::new(None) };
    static RESULT: RefCell<Option<SessionConfig>> = const { RefCell::new(None) };
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

/// Show the dialog; returns the settings if the user clicks Connect.
pub fn run(owner: HWND, cfg: &SessionConfig) -> Option<SessionConfig> {
    DONE.with(|d| d.set(false));
    RESULT.with(|r| *r.borrow_mut() = None);
    unsafe {
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex = WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT;
        let (x, y, w, h) = centered(owner, 380, 206, style, ex);
        let dlg = CLASS_W.with(|c| CreateWindowExW(ex, c.as_ptr(), wide("Connect to Host").as_ptr(), style,
            x, y, w, h, owner, null(), GetModuleHandleW(std::ptr::null()), std::ptr::null()));
        if dlg.is_null() {
            return None;
        }
        let font = ui_font();
        let label = |t: &str, y| child(dlg, "STATIC", t, SS_LEFT, 0, (12, y + 3, 100, 20), -1, font);
        let edit = |t: &str, y, w, id, extra| {
            child(dlg, "EDIT", t, ES_AUTOHSCROLL | WS_TABSTOP | extra, WS_EX_CLIENTEDGE, (116, y, w, 24), id, font)
        };
        label("&Host name:", 12);
        let host = edit(&cfg.host, 12, 252, IDC_HOST, 0);
        label("&Port:", 44);
        let port = edit(&cfg.port.to_string(), 44, 70, IDC_PORT, ES_NUMBER);
        child(dlg, "STATIC", "(23 is the usual unencrypted port)", SS_LEFT, 0, (194, 47, 180, 20), -1, font);
        label("&LU name:", 76);
        let lu = edit(&cfg.lu, 76, 140, IDC_LU, 0);
        child(dlg, "STATIC", "(optional)", SS_LEFT, 0, (264, 79, 100, 20), -1, font);
        label("&Code page:", 108);
        let codepage = child(dlg, "COMBOBOX", "", CBS_DROPDOWNLIST | WS_TABSTOP | WS_VSCROLL, 0,
            (116, 106, 252, 200), IDC_CODEPAGE, font);
        for cp in CodePage::ALL {
            SendMessageW(codepage, CB_ADDSTRING, 0, wide(cp.label()).as_ptr() as LPARAM);
        }
        let sel = CodePage::ALL.iter().position(|c| c.number() == cfg.codepage).unwrap_or(0);
        SendMessageW(codepage, CB_SETCURSEL, sel, 0);
        child(dlg, "BUTTON", "Connect", BS_DEFPUSHBUTTON | WS_TABSTOP, 0, (208, 166, 76, 26), IDOK, font);
        child(dlg, "BUTTON", "Cancel", BS_PUSHBUTTON | WS_TABSTOP, 0, (292, 166, 76, 26), IDCANCEL, font);
        SendMessageW(host, EM_SETSEL, 0, -1);
        CTRLS.with(|c| *c.borrow_mut() = Some(Controls { owner, host, port, lu, codepage }));

        EnableWindow(owner, 0);
        ShowWindow(dlg, SW_SHOW);
        SetFocus(host);
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

/// Read and check the fields; shows a message and returns None if invalid.
fn collect(dlg: HWND) -> Option<SessionConfig> {
    let (host, port, lu, cp) = CTRLS.with(|c| {
        let c = c.borrow();
        let c = c.as_ref()?;
        let cp = unsafe { SendMessageW(c.codepage, CB_GETCURSEL, 0, 0) };
        Some((window_text(c.host), window_text(c.port), window_text(c.lu), cp))
    })?;
    let host = host.trim().to_string();
    let focus = |id| CTRLS.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            unsafe { SetFocus(if id == IDC_HOST { c.host } else { c.port }) };
        }
    });
    if host.is_empty() {
        message_box(dlg, "Please enter the host name or IP address.", "Connect", MB_OK | MB_ICONWARNING);
        focus(IDC_HOST);
        return None;
    }
    let Some(port) = port.trim().parse::<u16>().ok().filter(|&p| p > 0) else {
        message_box(dlg, "Please enter a port number between 1 and 65535.", "Connect", MB_OK | MB_ICONWARNING);
        focus(IDC_PORT);
        return None;
    };
    let codepage = usize::try_from(cp).ok().and_then(|i| CodePage::ALL.get(i)).copied().unwrap_or_default();
    Some(SessionConfig { host, port, lu: lu.trim().to_uppercase(), codepage: codepage.number() })
}

fn close(dlg: HWND, result: Option<SessionConfig>) {
    RESULT.with(|r| *r.borrow_mut() = result);
    let owner = CTRLS.with(|c| c.borrow().as_ref().map(|c| c.owner));
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
            match loword(wp) as i32 {
                IDOK => {
                    if let Some(cfg) = collect(hwnd) {
                        close(hwnd, Some(cfg));
                    }
                }
                IDCANCEL => close(hwnd, None),
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
