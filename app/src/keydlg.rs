//! Settings > Keyboard... dialog.
//!
//! Pick a function on the left, see which keys run it, then click the
//! "New key" box, press a key and click Assign. Conflicts and keys Windows
//! itself uses are flagged before anything changes. Nothing is saved until OK.
//!
//! The preset box lists the built-in presets and the user's own saved ones.
//! "Save as..." stores the current keys under a name; "Delete" removes a
//! saved preset (built-ins can't be deleted).

use std::cell::{Cell, RefCell};

use tn3270_keymap::{Action, KeyChord, Keymap, Preset, SavedKeymaps};

use crate::win32::*;
use crate::{capture, prompt, settings};

const CLASS: &str = "Tn3270KeyboardSetup";
const IDC_PRESET: i32 = 1001;
const IDC_LOAD_PRESET: i32 = 1002;
const IDC_SAVE_AS: i32 = 1009;
const IDC_DELETE_PRESET: i32 = 1010;
const CBN_SELCHANGE: u16 = 1;

/// An entry in the preset box.
#[derive(Clone, PartialEq)]
enum Choice {
    BuiltIn(Preset),
    Saved(String),
}

impl Choice {
    fn label(&self) -> String {
        match self {
            Choice::BuiltIn(p) => p.label().to_string(),
            Choice::Saved(name) => name.clone(),
        }
    }
}
const IDC_LIST: i32 = 1003;
const IDC_KEYS: i32 = 1004;
const IDC_REMOVE: i32 = 1005;
const IDC_CAPTURE: i32 = 1006;
const IDC_CONFLICT: i32 = 1007;
const IDC_ASSIGN: i32 = 1008;
const CLIENT_W: i32 = 700;
const CLIENT_H: i32 = 452;

struct State {
    owner: HWND,
    list: HWND,
    keys_lb: HWND,
    capture: HWND,
    conflict: HWND,
    preset_cb: HWND,
    delete_btn: HWND,
    choices: Vec<Choice>,
    store: SavedKeymaps,
    keymap: Keymap,
    actions: Vec<Action>,
    selected: usize,
    keys_shown: Vec<KeyChord>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static DONE: Cell<bool> = const { Cell::new(false) };
    static RESULT: RefCell<Option<Keymap>> = const { RefCell::new(None) };
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

/// Show the dialog modally. Returns the edited keymap if the user clicks OK.
pub fn run(owner: HWND, keymap: Keymap) -> Option<Keymap> {
    DONE.with(|d| d.set(false));
    RESULT.with(|r| *r.borrow_mut() = None);
    unsafe {
        let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
        let ex = WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT;
        let mut rc = RECT { left: 0, top: 0, right: CLIENT_W, bottom: CLIENT_H };
        AdjustWindowRectEx(&mut rc, style, 0, ex);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let mut orc = RECT::default();
        GetWindowRect(owner, &mut orc);
        let x = orc.left + ((orc.right - orc.left) - w) / 2;
        let y = orc.top + ((orc.bottom - orc.top) - h) / 2;

        let dlg = CLASS_W.with(|c| {
            CreateWindowExW(ex, c.as_ptr(), wide("Keyboard Setup").as_ptr(), style,
                x, y.max(0), w, h, owner, null(), GetModuleHandleW(std::ptr::null()), std::ptr::null())
        });
        if dlg.is_null() {
            return None;
        }
        build(dlg, owner, keymap);
        EnableWindow(owner, 0);
        ShowWindow(dlg, SW_SHOW);

        let capture_hwnd = STATE.with(|s| s.borrow().as_ref().map(|s| s.capture)).unwrap_or(null());
        let mut msg: MSG = std::mem::zeroed();
        while !DONE.with(Cell::get) {
            if GetMessageW(&mut msg, null(), 0, 0) <= 0 {
                PostQuitMessage(msg.wParam as i32);
                break;
            }
            // The capture box must see every key, including Tab, Enter, Esc
            // and Alt+letter, so skip dialog navigation for it.
            if msg.hwnd == capture_hwnd || IsDialogMessageW(dlg, &mut msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    STATE.with(|s| *s.borrow_mut() = None);
    RESULT.with(|r| r.borrow_mut().take())
}

unsafe fn build(dlg: HWND, owner: HWND, keymap: Keymap) {
    let font = ui_font();
    let label = |text: &str, rect| child(dlg, "STATIC", text, SS_LEFT, 0, rect, -1, font);
    let button = |text: &str, rect, id, extra| {
        child(dlg, "BUTTON", text, WS_TABSTOP | extra, 0, rect, id, font)
    };

    label("&Preset:", (12, 15, 60, 20));
    let preset_cb = child(dlg, "COMBOBOX", "", CBS_DROPDOWNLIST | WS_TABSTOP | WS_VSCROLL, 0,
        (72, 11, 260, 240), IDC_PRESET, font);
    button("&Load", (340, 10, 70, 25), IDC_LOAD_PRESET, BS_PUSHBUTTON);
    button("&Save as...", (416, 10, 84, 25), IDC_SAVE_AS, BS_PUSHBUTTON);
    let delete_btn = button("&Delete", (506, 10, 70, 25), IDC_DELETE_PRESET, BS_PUSHBUTTON);

    label("&Functions:", (12, 48, 200, 18));
    let list = child(dlg, "SysListView32", "",
        LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS | WS_TABSTOP, WS_EX_CLIENTEDGE,
        (12, 66, 440, 334), IDC_LIST, font);
    SetWindowTheme(list, wide("Explorer").as_ptr(), std::ptr::null());
    SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, 0, (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as LPARAM);
    for (i, (title, width)) in [("Function", 190), ("Group", 105), ("Keys", 140)].iter().enumerate() {
        let mut text = wide(title);
        let col = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH | LVCF_SUBITEM,
            fmt: 0, cx: *width, pszText: text.as_mut_ptr(), cchTextMax: 0,
            iSubItem: i as i32, iImage: 0, iOrder: 0, cxMin: 0, cxDefault: 0, cxIdeal: 0,
        };
        SendMessageW(list, LVM_INSERTCOLUMNW, i, &col as *const _ as LPARAM);
    }

    label("&Keys for this function:", (466, 48, 222, 18));
    let keys_lb = child(dlg, "LISTBOX", "", LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | WS_TABSTOP | WS_VSCROLL,
        WS_EX_CLIENTEDGE, (466, 66, 222, 96), IDC_KEYS, font);
    button("&Remove key", (466, 166, 110, 25), IDC_REMOVE, BS_PUSHBUTTON);

    label("&New key:", (466, 206, 222, 18));
    let capture = child(dlg, capture::CLASS, "", WS_TABSTOP, WS_EX_CLIENTEDGE,
        (466, 224, 222, 28), IDC_CAPTURE, font);
    capture::clear(capture);
    let conflict = child(dlg, "STATIC", "", SS_LEFT, 0, (466, 258, 222, 46), IDC_CONFLICT, font);
    button("&Assign", (466, 306, 110, 25), IDC_ASSIGN, BS_PUSHBUTTON);

    label("Tip: Ctrl, Alt and Shift keys can be assigned on their own, e.g. Right Ctrl = Enter.",
        (12, 414, 480, 32));
    button("OK", (528, 414, 76, 26), IDOK, BS_DEFPUSHBUTTON);
    button("Cancel", (612, 414, 76, 26), IDCANCEL, BS_PUSHBUTTON);

    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            owner, list, keys_lb, capture, conflict, preset_cb, delete_btn,
            choices: Vec::new(), store: settings::saved_keymaps(), keymap,
            actions: Action::all_bindable(), selected: 0, keys_shown: Vec::new(),
        });
    });
    with_state(|st| {
        let current = st.keymap.name.clone();
        fill_presets(st, &current);
        fill_list(st);
        select(st, 0);
        show_conflict(st);
    });
    SetFocus(list);
}

/// Run `f` with the dialog state. Re-entrant calls (e.g. a list-view
/// notification sent while we are filling the list) are ignored.
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

fn keys_text(km: &Keymap, a: &Action) -> String {
    km.keys_for(a).iter().map(|c| c.to_string()).collect::<Vec<_>>().join(", ")
}

unsafe fn set_item_text(list: HWND, row: usize, col: i32, text: &str) {
    let mut w = wide(text);
    let item = LVITEMW {
        mask: LVIF_TEXT, iItem: row as i32, iSubItem: col, state: 0, stateMask: 0,
        pszText: w.as_mut_ptr(), cchTextMax: 0, iImage: 0, lParam: 0, iIndent: 0,
        iGroupId: 0, cColumns: 0, puColumns: std::ptr::null_mut(),
        piColFmt: std::ptr::null_mut(), iGroup: 0,
    };
    SendMessageW(list, LVM_SETITEMTEXTW, row, &item as *const _ as LPARAM);
}

fn fill_list(st: &mut State) {
    unsafe {
        SendMessageW(st.list, LVM_DELETEALLITEMS, 0, 0);
        for (i, a) in st.actions.iter().enumerate() {
            let mut w = wide(&a.description());
            let item = LVITEMW {
                mask: LVIF_TEXT | LVIF_PARAM, iItem: i as i32, iSubItem: 0, state: 0, stateMask: 0,
                pszText: w.as_mut_ptr(), cchTextMax: 0, iImage: 0, lParam: i as LPARAM,
                iIndent: 0, iGroupId: 0, cColumns: 0, puColumns: std::ptr::null_mut(),
                piColFmt: std::ptr::null_mut(), iGroup: 0,
            };
            SendMessageW(st.list, LVM_INSERTITEMW, 0, &item as *const _ as LPARAM);
            set_item_text(st.list, i, 1, a.category().label());
            set_item_text(st.list, i, 2, &keys_text(&st.keymap, a));
        }
    }
}

fn refresh_row(st: &State, row: usize) {
    unsafe { set_item_text(st.list, row, 2, &keys_text(&st.keymap, &st.actions[row])) };
}

/// Select a row in the function list and show its keys.
fn select(st: &mut State, row: usize) {
    st.selected = row.min(st.actions.len().saturating_sub(1));
    unsafe {
        let mut item: LVITEMW = std::mem::zeroed();
        item.stateMask = LVIS_SELECTED | LVIS_FOCUSED;
        item.state = LVIS_SELECTED | LVIS_FOCUSED;
        SendMessageW(st.list, LVM_SETITEMSTATE, st.selected, &item as *const _ as LPARAM);
        SendMessageW(st.list, LVM_ENSUREVISIBLE, st.selected, 0);
    }
    show_keys(st);
}

fn show_keys(st: &mut State) {
    st.keys_shown = st.keymap.keys_for(&st.actions[st.selected]);
    unsafe {
        SendMessageW(st.keys_lb, LB_RESETCONTENT, 0, 0);
        if st.keys_shown.is_empty() {
            SendMessageW(st.keys_lb, LB_ADDSTRING, 0, wide("(none)").as_ptr() as LPARAM);
        }
        for c in &st.keys_shown {
            SendMessageW(st.keys_lb, LB_ADDSTRING, 0, wide(&c.to_string()).as_ptr() as LPARAM);
        }
        SendMessageW(st.keys_lb, LB_SETCURSEL, 0, 0);
    }
}

fn show_conflict(st: &State) {
    let text = match capture::captured() {
        None => String::new(),
        Some(c) => match (st.keymap.get(&c), c.reserved_reason()) {
            (Some(a), _) if *a == st.actions[st.selected] => "Already assigned to this function.".into(),
            (Some(a), _) => format!("Currently assigned to: {}", a.description()),
            (None, Some(why)) => format!("Warning: {why}"),
            (None, None) => "Not assigned to anything.".into(),
        },
    };
    unsafe { SetWindowTextW(st.conflict, wide(&text).as_ptr()) };
}

fn on_assign(dlg: HWND) {
    let Some(chord) = capture::captured() else {
        message_box(dlg, "Click the New key box and press the key you want first.",
            "Keyboard Setup", MB_OK | MB_ICONINFORMATION);
        return;
    };
    let Some((target, existing)) = with_state(|st| {
        (st.actions[st.selected].clone(), st.keymap.get(&chord).cloned())
    }) else { return };

    if existing.as_ref() == Some(&target) {
        return;
    }
    if let Some(old) = &existing {
        let q = format!("{chord} is currently assigned to \u{201C}{}\u{201D}.\n\nAssign it to \u{201C}{}\u{201D} instead?",
            old.description(), target.description());
        if message_box(dlg, &q, "Keyboard Setup", MB_YESNO | MB_ICONQUESTION) != IDYES {
            return;
        }
    } else if let Some(why) = chord.reserved_reason() {
        let q = format!("{why}\n\nAssign {chord} to \u{201C}{}\u{201D} anyway?", target.description());
        if message_box(dlg, &q, "Keyboard Setup", MB_YESNO | MB_ICONWARNING) != IDYES {
            return;
        }
    }
    with_state(|st| {
        st.keymap.assign(chord, target);
        mark_edited(st);
        if let Some(old) = existing {
            if let Some(row) = st.actions.iter().position(|a| *a == old) {
                refresh_row(st, row);
            }
        }
        refresh_row(st, st.selected);
        show_keys(st);
        show_conflict(st);
    });
}

fn on_remove() {
    with_state(|st| unsafe {
        let i = SendMessageW(st.keys_lb, LB_GETCURSEL, 0, 0);
        if let Some(chord) = usize::try_from(i).ok().and_then(|i| st.keys_shown.get(i)).copied() {
            st.keymap.remove(&chord);
            mark_edited(st);
            refresh_row(st, st.selected);
            show_keys(st);
            show_conflict(st);
        }
    });
}

/// After an edit, a built-in preset's name no longer describes the keys.
/// A saved preset keeps its name, so "Save as..." offers to update it.
fn mark_edited(st: &mut State) {
    if Preset::ALL.iter().any(|p| p.label() == st.keymap.name) {
        st.keymap.name = "Custom".into();
    }
}

/// Fill the preset box (built-ins, then saved presets A-Z) and select `name`.
fn fill_presets(st: &mut State, name: &str) {
    st.choices = Preset::ALL.iter().map(|p| Choice::BuiltIn(*p))
        .chain(st.store.list().into_iter().map(Choice::Saved))
        .collect();
    let sel = st.choices.iter().position(|c| c.label().eq_ignore_ascii_case(name)).unwrap_or(0);
    unsafe {
        SendMessageW(st.preset_cb, CB_RESETCONTENT, 0, 0);
        for c in &st.choices {
            SendMessageW(st.preset_cb, CB_ADDSTRING, 0, wide(&c.label()).as_ptr() as LPARAM);
        }
        SendMessageW(st.preset_cb, CB_SETCURSEL, sel, 0);
    }
    update_delete_button(st);
}

fn chosen(st: &State) -> Option<Choice> {
    let i = unsafe { SendMessageW(st.preset_cb, CB_GETCURSEL, 0, 0) };
    usize::try_from(i).ok().and_then(|i| st.choices.get(i)).cloned()
}

fn update_delete_button(st: &State) {
    let saved = matches!(chosen(st), Some(Choice::Saved(_)));
    unsafe { EnableWindow(st.delete_btn, saved as BOOL) };
}

fn on_load_preset(dlg: HWND) {
    let Some(Some(choice)) = with_state(|st| chosen(st)) else { return };
    let q = format!("Replace all key assignments with the \u{201C}{}\u{201D} preset?", choice.label());
    if message_box(dlg, &q, "Keyboard Setup", MB_YESNO | MB_ICONQUESTION) != IDYES {
        return;
    }
    let loaded = match &choice {
        Choice::BuiltIn(p) => Ok(Keymap::preset(*p)),
        Choice::Saved(name) => settings::saved_keymaps().load(name),
    };
    match loaded {
        Ok(km) => {
            with_state(|st| {
                st.keymap = km;
                let row = st.selected;
                fill_list(st);
                select(st, row);
                show_conflict(st);
            });
        }
        Err(e) => {
            message_box(dlg, &format!("The preset could not be loaded:\n{e}"), "Keyboard Setup",
                MB_OK | MB_ICONERROR);
        }
    }
}

fn on_save_as(dlg: HWND) {
    let Some(current) = with_state(|st| st.keymap.name.clone()) else { return };
    let mut suggestion = if SavedKeymaps::check_name(&current).is_ok() { current } else { String::new() };
    loop {
        let Some(name) = prompt::ask(dlg, "Save Preset", "Save the current keys as a preset named:",
            &suggestion, tn3270_keymap::MAX_PRESET_NAME_LEN) else { return };
        if let Err(why) = SavedKeymaps::check_name(&name) {
            message_box(dlg, &why, "Save Preset", MB_OK | MB_ICONWARNING);
            suggestion = name;
            continue;
        }
        let store = settings::saved_keymaps();
        if store.exists(&name) {
            let q = format!("A preset named \u{201C}{name}\u{201D} already exists.\n\nReplace it?");
            if message_box(dlg, &q, "Save Preset", MB_YESNO | MB_ICONQUESTION) != IDYES {
                suggestion = name;
                continue;
            }
        }
        let Some(mut km) = with_state(|st| st.keymap.clone()) else { return };
        km.name = name.clone();
        match store.save(&km) {
            Ok(_) => {
                with_state(|st| {
                    st.keymap.name = name.clone();
                    fill_presets(st, &name);
                });
            }
            Err(e) => {
                message_box(dlg, &format!("The preset could not be saved:\n{e}"), "Save Preset",
                    MB_OK | MB_ICONERROR);
            }
        }
        return;
    }
}

fn on_delete_preset(dlg: HWND) {
    let Some(Some(Choice::Saved(name))) = with_state(|st| chosen(st)) else { return };
    let q = format!("Delete the saved preset \u{201C}{name}\u{201D}?\n\nThe keys currently in this dialog are not changed.");
    if message_box(dlg, &q, "Keyboard Setup", MB_YESNO | MB_ICONWARNING) != IDYES {
        return;
    }
    if let Err(e) = settings::saved_keymaps().delete(&name) {
        message_box(dlg, &format!("The preset could not be deleted:\n{e}"), "Keyboard Setup",
            MB_OK | MB_ICONERROR);
        return;
    }
    with_state(|st| {
        if st.keymap.name.eq_ignore_ascii_case(&name) {
            st.keymap.name = "Custom".into();
        }
        fill_presets(st, "");
    });
}

fn close(dlg: HWND, ok: bool) {
    let owner = with_state(|st| {
        if ok {
            RESULT.with(|r| *r.borrow_mut() = Some(st.keymap.clone()));
        }
        st.owner
    });
    unsafe {
        if let Some(owner) = owner {
            EnableWindow(owner, 1);
            SetForegroundWindow(owner);
        }
        DestroyWindow(dlg);
    }
    DONE.with(|d| d.set(true));
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let (id, code) = (loword(wp) as i32, hiword(wp));
            match (id, code) {
                (IDOK, _) => close(hwnd, true),
                (IDCANCEL, _) => close(hwnd, false),
                (IDC_ASSIGN, BN_CLICKED) => on_assign(hwnd),
                (IDC_REMOVE, BN_CLICKED) => on_remove(),
                (IDC_LOAD_PRESET, BN_CLICKED) => on_load_preset(hwnd),
                (IDC_SAVE_AS, BN_CLICKED) => on_save_as(hwnd),
                (IDC_DELETE_PRESET, BN_CLICKED) => on_delete_preset(hwnd),
                (IDC_PRESET, CBN_SELCHANGE) => {
                    with_state(|st| update_delete_button(st));
                }
                (IDC_CAPTURE, capture::CN_CAPTURED) => {
                    with_state(|st| show_conflict(st));
                }
                _ => {}
            }
            0
        }
        WM_NOTIFY => {
            let hdr = &*(lp as *const NMHDR);
            if hdr.idFrom == IDC_LIST as usize && hdr.code == LVN_ITEMCHANGED {
                let nm = &*(lp as *const NMLISTVIEW);
                if nm.uChanged & LVIF_STATE != 0 && nm.uNewState & LVIS_SELECTED != 0 {
                    let row = nm.lParam as usize;
                    with_state(|st| {
                        if row < st.actions.len() {
                            st.selected = row;
                            show_keys(st);
                            show_conflict(st);
                        }
                    });
                }
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
