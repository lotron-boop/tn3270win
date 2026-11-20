use std::fmt;

/// Windows virtual-key codes used by the keymap.
pub mod vk {
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const MENU: u16 = 0x12; // Alt
    pub const PAUSE: u16 = 0x13;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const PRIOR: u16 = 0x21; // Page Up
    pub const NEXT: u16 = 0x22; // Page Down
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const SNAPSHOT: u16 = 0x2C; // Print Screen
    pub const INSERT: u16 = 0x2D;
    pub const DELETE: u16 = 0x2E;
    pub const LWIN: u16 = 0x5B;
    pub const RWIN: u16 = 0x5C;
    pub const APPS: u16 = 0x5D;
    pub const NUMPAD0: u16 = 0x60;
    pub const MULTIPLY: u16 = 0x6A;
    pub const ADD: u16 = 0x6B;
    pub const SUBTRACT: u16 = 0x6D;
    pub const DECIMAL: u16 = 0x6E;
    pub const DIVIDE: u16 = 0x6F;
    pub const F1: u16 = 0x70;
    pub const F24: u16 = 0x87;
    pub const NUMLOCK: u16 = 0x90;
    pub const SCROLL: u16 = 0x91;
    pub const LSHIFT: u16 = 0xA0;
    pub const RSHIFT: u16 = 0xA1;
    pub const LCONTROL: u16 = 0xA2;
    pub const RCONTROL: u16 = 0xA3;
    pub const LMENU: u16 = 0xA4;
    pub const RMENU: u16 = 0xA5;
    pub const OEM_1: u16 = 0xBA; // ;
    pub const OEM_PLUS: u16 = 0xBB; // =
    pub const OEM_COMMA: u16 = 0xBC;
    pub const OEM_MINUS: u16 = 0xBD;
    pub const OEM_PERIOD: u16 = 0xBE;
    pub const OEM_2: u16 = 0xBF; // /
    pub const OEM_3: u16 = 0xC0; // `
    pub const OEM_4: u16 = 0xDB; // [
    pub const OEM_5: u16 = 0xDC; // \
    pub const OEM_6: u16 = 0xDD; // ]
    pub const OEM_7: u16 = 0xDE; // '
    pub const OEM_102: u16 = 0xE2; // < > on ISO keyboards
}

/// Modifier keys held with a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const CTRL: Mods = Mods(1);
    pub const ALT: Mods = Mods(2);
    pub const SHIFT: Mods = Mods(4);

    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn without(self, other: Mods) -> Mods {
        Mods(self.0 & !other.0)
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, rhs: Mods) -> Mods {
        Mods(self.0 | rhs.0)
    }
}

/// A physical key with modifiers, e.g. `Ctrl+Shift+F1` or `Right Ctrl`.
///
/// Left/right Ctrl, Alt and Shift are distinct keys. The keypad Enter is
/// distinguished from the main Enter by `numpad_enter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyChord {
    pub vk: u16,
    pub numpad_enter: bool,
    pub mods: Mods,
}

impl KeyChord {
    pub const fn new(mods: Mods, vk: u16) -> KeyChord {
        KeyChord { vk, numpad_enter: false, mods }
    }

    pub const fn key(vk: u16) -> KeyChord {
        KeyChord::new(Mods::NONE, vk)
    }

    pub const fn numpad_enter(mods: Mods) -> KeyChord {
        KeyChord { vk: vk::RETURN, numpad_enter: true, mods }
    }

    /// Build a chord from a Windows `WM_KEYDOWN`/`WM_KEYUP` message.
    ///
    /// `lparam` is the message's lParam (bit 24 = extended key, bits 16-23 =
    /// scan code); `shift`/`ctrl`/`alt` are the current modifier states.
    /// Generic Ctrl/Alt/Shift codes are resolved to their left/right forms,
    /// and the key's own modifier is dropped, so pressing Right Ctrl alone
    /// gives `Right Ctrl` rather than `Ctrl+Right Ctrl`.
    pub fn from_win32(vk_code: u16, lparam: isize, shift: bool, ctrl: bool, alt: bool) -> KeyChord {
        let extended = (lparam >> 24) & 1 == 1;
        let scan = ((lparam >> 16) & 0xFF) as u8;
        let vk_code = match vk_code {
            vk::CONTROL => if extended { vk::RCONTROL } else { vk::LCONTROL },
            vk::MENU => if extended { vk::RMENU } else { vk::LMENU },
            vk::SHIFT => if scan == 0x36 { vk::RSHIFT } else { vk::LSHIFT },
            other => other,
        };
        let mut mods = Mods::NONE;
        if ctrl { mods = mods | Mods::CTRL; }
        if alt { mods = mods | Mods::ALT; }
        if shift { mods = mods | Mods::SHIFT; }
        let mut chord = KeyChord {
            vk: vk_code,
            numpad_enter: vk_code == vk::RETURN && extended,
            mods,
        };
        if let Some(own) = chord.own_modifier() {
            chord.mods = chord.mods.without(own);
        }
        chord
    }

    /// If this key is itself a modifier, the modifier flag it sets.
    fn own_modifier(&self) -> Option<Mods> {
        match self.vk {
            vk::LCONTROL | vk::RCONTROL | vk::CONTROL => Some(Mods::CTRL),
            vk::LMENU | vk::RMENU | vk::MENU => Some(Mods::ALT),
            vk::LSHIFT | vk::RSHIFT | vk::SHIFT => Some(Mods::SHIFT),
            _ => None,
        }
    }

    /// True for Ctrl, Alt and Shift keys (either side).
    pub fn is_modifier_key(&self) -> bool {
        self.own_modifier().is_some()
    }

    /// Display/config name of the key without modifiers, e.g. "Right Ctrl".
    pub fn key_name(&self) -> String {
        if self.numpad_enter {
            return "Numpad Enter".into();
        }
        key_name(self.vk).unwrap_or_else(|| format!("Key 0x{:02X}", self.vk))
    }

    /// Parse text such as `Ctrl+Shift+F1`, `Right Ctrl` or `Numpad Enter`.
    pub fn parse(text: &str) -> Option<KeyChord> {
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let (key_part, mod_parts) = parts.split_last()?;
        let mut mods = Mods::NONE;
        for m in mod_parts {
            mods = mods | match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => Mods::CTRL,
                "alt" => Mods::ALT,
                "shift" => Mods::SHIFT,
                _ => return None,
            };
        }
        if key_part.eq_ignore_ascii_case("Numpad Enter") {
            return Some(KeyChord::numpad_enter(mods));
        }
        if let Some(hex) = key_part.strip_prefix("Key 0x") {
            return u16::from_str_radix(hex, 16).ok().map(|v| KeyChord::new(mods, v));
        }
        (0u16..=0xFF)
            .find(|&v| key_name(v).is_some_and(|n| n.eq_ignore_ascii_case(key_part)))
            .map(|v| KeyChord::new(mods, v))
    }

    /// Why a chord is a bad idea to map (Windows uses it), if it is.
    pub fn reserved_reason(&self) -> Option<&'static str> {
        let alt = Mods::ALT;
        match (self.mods, self.vk) {
            (m, v) if m == alt && v == vk::F1 + 3 => Some("Alt+F4 closes the window."),
            (m, vk::TAB) if m.contains(alt) => Some("Alt+Tab switches windows."),
            (m, vk::SPACE) if m == alt => Some("Alt+Space opens the window menu."),
            (m, vk::ESCAPE) if m.contains(Mods::CTRL) || m.contains(alt) => {
                Some("Windows uses this key combination.")
            }
            (_, vk::LWIN) | (_, vk::RWIN) => Some("The Windows key is reserved by Windows."),
            _ => None,
        }
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.contains(Mods::CTRL) { f.write_str("Ctrl+")?; }
        if self.mods.contains(Mods::ALT) { f.write_str("Alt+")?; }
        if self.mods.contains(Mods::SHIFT) { f.write_str("Shift+")?; }
        f.write_str(&self.key_name())
    }
}

fn key_name(v: u16) -> Option<String> {
    let s = match v {
        0x30..=0x39 | 0x41..=0x5A => return Some((v as u8 as char).to_string()),
        vk::F1..=vk::F24 => return Some(format!("F{}", v - vk::F1 + 1)),
        0x60..=0x69 => return Some(format!("Numpad {}", v - vk::NUMPAD0)),
        vk::BACK => "Backspace",
        vk::TAB => "Tab",
        vk::RETURN => "Enter",
        vk::PAUSE => "Pause",
        vk::ESCAPE => "Esc",
        vk::SPACE => "Space",
        vk::PRIOR => "Page Up",
        vk::NEXT => "Page Down",
        vk::END => "End",
        vk::HOME => "Home",
        vk::LEFT => "Left",
        vk::UP => "Up",
        vk::RIGHT => "Right",
        vk::DOWN => "Down",
        vk::SNAPSHOT => "Print Screen",
        vk::INSERT => "Insert",
        vk::DELETE => "Delete",
        vk::LWIN => "Left Windows",
        vk::RWIN => "Right Windows",
        vk::APPS => "Menu",
        vk::MULTIPLY => "Numpad Multiply",
        vk::ADD => "Numpad Add",
        vk::SUBTRACT => "Numpad Subtract",
        vk::DECIMAL => "Numpad Decimal",
        vk::DIVIDE => "Numpad Divide",
        vk::NUMLOCK => "Num Lock",
        vk::SCROLL => "Scroll Lock",
        vk::LSHIFT => "Left Shift",
        vk::RSHIFT => "Right Shift",
        vk::LCONTROL => "Left Ctrl",
        vk::RCONTROL => "Right Ctrl",
        vk::LMENU => "Left Alt",
        vk::RMENU => "Right Alt",
        vk::OEM_1 => ";",
        vk::OEM_PLUS => "=",
        vk::OEM_COMMA => ",",
        vk::OEM_MINUS => "-",
        vk::OEM_PERIOD => ".",
        vk::OEM_2 => "/",
        vk::OEM_3 => "`",
        vk::OEM_4 => "[",
        vk::OEM_5 => "\\",
        vk::OEM_6 => "]",
        vk::OEM_7 => "'",
        vk::OEM_102 => "<>",
        _ => return None,
    };
    Some(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXT: isize = 1 << 24;

    #[test]
    fn right_ctrl_alone_is_its_own_key() {
        let c = KeyChord::from_win32(vk::CONTROL, EXT, false, true, false);
        assert_eq!(c, KeyChord::key(vk::RCONTROL));
        assert_eq!(c.to_string(), "Right Ctrl");
        let l = KeyChord::from_win32(vk::CONTROL, 0, false, true, false);
        assert_eq!(l.to_string(), "Left Ctrl");
    }

    #[test]
    fn numpad_enter_differs_from_enter() {
        let main = KeyChord::from_win32(vk::RETURN, 0, false, false, false);
        let pad = KeyChord::from_win32(vk::RETURN, EXT, false, false, false);
        assert_ne!(main, pad);
        assert_eq!(pad.to_string(), "Numpad Enter");
    }

    #[test]
    fn right_shift_by_scan_code() {
        let r = KeyChord::from_win32(vk::SHIFT, 0x36 << 16, true, false, false);
        assert_eq!(r.to_string(), "Right Shift");
    }

    #[test]
    fn names_round_trip() {
        for text in ["Ctrl+Shift+F1", "Right Ctrl", "Numpad Enter", "Alt+Numpad Enter",
                     "Shift+Tab", "Ctrl+Alt+\\", "Page Down", "Alt+1", "Ctrl+-", "Key 0xE5"] {
            let c = KeyChord::parse(text).unwrap_or_else(|| panic!("parse {text}"));
            assert_eq!(c.to_string(), text);
        }
        assert_eq!(KeyChord::parse("shift+f13"), Some(KeyChord::new(Mods::SHIFT, vk::F1 + 12)));
        assert!(KeyChord::parse("Hyper+F1").is_none());
        assert!(KeyChord::parse("Nope").is_none());
    }

    #[test]
    fn reserved() {
        assert!(KeyChord::new(Mods::ALT, vk::F1 + 3).reserved_reason().is_some());
        assert!(KeyChord::key(vk::F1 + 3).reserved_reason().is_none());
    }
}
