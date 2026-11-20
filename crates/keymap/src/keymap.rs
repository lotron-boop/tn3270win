use std::collections::BTreeMap;
use std::fmt;

use crate::action::Action;
use crate::key::{vk, KeyChord, Mods};

/// Built-in starting layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Terminal-style: Right Ctrl and Numpad Enter send Enter, the main
    /// Enter key is New Line, Left Ctrl is Reset.
    Classic3270,
    /// PC-style: the Enter key sends Enter; Ctrl keys are left alone.
    PcStyle,
}

impl Preset {
    pub const ALL: [Preset; 2] = [Preset::Classic3270, Preset::PcStyle];

    pub fn id(self) -> &'static str {
        match self {
            Preset::Classic3270 => "classic3270",
            Preset::PcStyle => "pc",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Classic3270 => "Classic 3270 (Right Ctrl = Enter)",
            Preset::PcStyle => "PC style (Enter key = Enter)",
        }
    }
}

/// Chord-to-action table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    pub name: String,
    map: BTreeMap<KeyChord, Action>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

impl Default for Keymap {
    fn default() -> Self {
        Keymap::preset(Preset::Classic3270)
    }
}

impl Keymap {
    pub fn empty(name: &str) -> Keymap {
        Keymap { name: name.to_string(), map: BTreeMap::new() }
    }

    pub fn preset(p: Preset) -> Keymap {
        use Action::*;
        let mut km = Keymap::empty(p.label());
        let k = KeyChord::key;
        let ctrl = |v| KeyChord::new(Mods::CTRL, v);
        let shift = |v| KeyChord::new(Mods::SHIFT, v);
        let alt = |v| KeyChord::new(Mods::ALT, v);

        for n in 0..12u16 {
            km.assign(k(vk::F1 + n), Pf(n as u8 + 1));
            km.assign(shift(vk::F1 + n), Pf(n as u8 + 13));
        }
        for n in 0..3u16 {
            km.assign(alt(vk::F1 + n), Pa(n as u8 + 1));
        }
        km.assign(k(vk::PRIOR), Pf(7));
        km.assign(k(vk::NEXT), Pf(8));
        km.assign(k(vk::PAUSE), Clear);
        km.assign(k(vk::ESCAPE), Reset);
        km.assign(shift(vk::ESCAPE), Attn);
        km.assign(alt(vk::SNAPSHOT), SysReq);
        km.assign(k(vk::TAB), Tab);
        km.assign(shift(vk::TAB), BackTab);
        km.assign(k(vk::HOME), Home);
        km.assign(k(vk::END), EndOfField);
        km.assign(ctrl(vk::END), EraseEof);
        km.assign(ctrl(vk::DELETE), EraseEof);
        km.assign(alt(vk::DELETE), EraseInput);
        km.assign(k(vk::UP), Up);
        km.assign(k(vk::DOWN), Down);
        km.assign(k(vk::LEFT), Left);
        km.assign(k(vk::RIGHT), Right);
        km.assign(ctrl(vk::LEFT), WordLeft);
        km.assign(ctrl(vk::RIGHT), WordRight);
        km.assign(k(vk::BACK), Backspace);
        km.assign(k(vk::DELETE), Delete);
        km.assign(k(vk::INSERT), Insert);
        km.assign(ctrl(b'C' as u16), Copy);
        km.assign(ctrl(b'X' as u16), Cut);
        km.assign(ctrl(b'V' as u16), Paste);
        km.assign(shift(vk::INSERT), Paste);
        km.assign(ctrl(vk::INSERT), Copy);
        km.assign(ctrl(b'P' as u16), PrintScreen);

        match p {
            Preset::Classic3270 => {
                km.assign(k(vk::RCONTROL), Enter);
                km.assign(KeyChord::numpad_enter(Mods::NONE), Enter);
                km.assign(k(vk::RETURN), NewLine);
                km.assign(k(vk::LCONTROL), Reset);
            }
            Preset::PcStyle => {
                km.assign(k(vk::RETURN), Enter);
                km.assign(KeyChord::numpad_enter(Mods::NONE), Enter);
                km.assign(ctrl(vk::RETURN), NewLine);
                km.assign(shift(vk::RETURN), NewLine);
            }
        }
        km
    }

    /// Map `chord` to `action`, returning what it was mapped to before.
    pub fn assign(&mut self, chord: KeyChord, action: Action) -> Option<Action> {
        self.map.insert(chord, action)
    }

    pub fn remove(&mut self, chord: &KeyChord) -> Option<Action> {
        self.map.remove(chord)
    }

    pub fn get(&self, chord: &KeyChord) -> Option<&Action> {
        self.map.get(chord)
    }

    /// All chords mapped to `action`, sorted.
    pub fn keys_for(&self, action: &Action) -> Vec<KeyChord> {
        self.map.iter().filter(|(_, a)| *a == action).map(|(c, _)| *c).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&KeyChord, &Action)> {
        self.map.iter()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Serialise to the `.kmp` text format.
    pub fn to_text(&self) -> String {
        let mut s = String::from("# tn3270win keyboard map\n");
        s.push_str(&format!("name = {}\n\n[keys]\n", self.name));
        for (chord, action) in &self.map {
            s.push_str(&format!("{chord} = {}\n", action.token()));
        }
        s
    }

    /// Parse the `.kmp` text format. Unknown keys or actions are errors so a
    /// typo is reported instead of silently dropped.
    pub fn from_text(text: &str) -> Result<Keymap, ParseError> {
        let mut km = Keymap::empty("Custom");
        let mut in_keys = false;
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            let err = |message: String| ParseError { line: i + 1, message };
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if line.starts_with('[') {
                in_keys = line.eq_ignore_ascii_case("[keys]");
                if !in_keys {
                    return Err(err(format!("unknown section {line}")));
                }
                continue;
            }
            // Split on " = " (key names such as "=" contain the sign itself).
            let (left, right) = line.split_once(" = ")
                .ok_or_else(|| err("expected `key = action`".into()))?;
            let (left, right) = (left.trim(), right.trim());
            if !in_keys {
                if left.eq_ignore_ascii_case("name") {
                    km.name = right.to_string();
                    continue;
                }
                return Err(err(format!("unknown setting `{left}`")));
            }
            let chord = KeyChord::parse(left).ok_or_else(|| err(format!("unknown key `{left}`")))?;
            let action = Action::parse(right).ok_or_else(|| err(format!("unknown action `{right}`")))?;
            km.assign(chord, action);
        }
        Ok(km)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip_through_text() {
        for p in Preset::ALL {
            let km = Keymap::preset(p);
            let back = Keymap::from_text(&km.to_text()).unwrap();
            assert_eq!(back, km);
        }
    }

    #[test]
    fn classic_puts_enter_on_right_ctrl() {
        let km = Keymap::preset(Preset::Classic3270);
        assert_eq!(km.get(&KeyChord::key(vk::RCONTROL)), Some(&Action::Enter));
        assert_eq!(km.get(&KeyChord::key(vk::RETURN)), Some(&Action::NewLine));
        let pc = Keymap::preset(Preset::PcStyle);
        assert_eq!(pc.get(&KeyChord::key(vk::RETURN)), Some(&Action::Enter));
        assert_eq!(pc.get(&KeyChord::key(vk::RCONTROL)), None);
    }

    #[test]
    fn assign_reports_previous() {
        let mut km = Keymap::preset(Preset::Classic3270);
        let prev = km.assign(KeyChord::key(vk::RETURN), Action::Enter);
        assert_eq!(prev, Some(Action::NewLine));
        assert_eq!(km.keys_for(&Action::Enter).len(), 3);
    }

    #[test]
    fn readable_file() {
        let text = "# mine\nname = Bob's keys\n\n[keys]\nRight Ctrl = Enter\n= = Text \"¬\"\nShift+6 = Text \"¢\"\nalt+f1 = pa1\n";
        let km = Keymap::from_text(text).unwrap();
        assert_eq!(km.name, "Bob's keys");
        assert_eq!(km.len(), 4);
        assert_eq!(km.get(&KeyChord::key(vk::OEM_PLUS)), Some(&Action::Text("¬".into())));
    }

    #[test]
    fn errors_name_the_line() {
        let e = Keymap::from_text("[keys]\nRight Ctrl = Enter\nF99 = PF1\n").unwrap_err();
        assert_eq!(e.line, 3);
        assert!(e.message.contains("F99"));
        assert!(Keymap::from_text("[keys]\nF1 = Launch\n").is_err());
    }
}
