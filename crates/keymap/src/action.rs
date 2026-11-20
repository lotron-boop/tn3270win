/// A function a key can perform in the emulator.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    // Host (AID) keys: these send the screen to the host.
    Enter,
    Clear,
    Pf(u8),
    Pa(u8),
    SysReq,
    Attn,
    // Local keyboard functions.
    Reset,
    Tab,
    BackTab,
    NewLine,
    Home,
    EndOfField,
    Up,
    Down,
    Left,
    Right,
    WordLeft,
    WordRight,
    Backspace,
    Delete,
    Insert,
    EraseEof,
    EraseInput,
    FieldMark,
    Dup,
    Copy,
    Cut,
    Paste,
    PrintScreen,
    /// Type these characters, e.g. "¢" or "¬".
    Text(String),
    /// Run a named macro (macros are a later milestone).
    Macro(String),
}

/// Grouping used by the Keyboard Setup dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    HostKeys,
    ProgramFunction,
    Cursor,
    Editing,
    Clipboard,
    Characters,
    Macros,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::HostKeys => "Host keys",
            Category::ProgramFunction => "PF keys",
            Category::Cursor => "Cursor movement",
            Category::Editing => "Editing",
            Category::Clipboard => "Clipboard and print",
            Category::Characters => "Characters",
            Category::Macros => "Macros",
        }
    }
}

const SIMPLE: &[(Action, &str, &str)] = &[
    (Action::Enter, "Enter", "Enter"),
    (Action::Clear, "Clear", "Clear"),
    (Action::SysReq, "SysReq", "System Request"),
    (Action::Attn, "Attn", "Attention"),
    (Action::Reset, "Reset", "Reset (unlock keyboard)"),
    (Action::Tab, "Tab", "Next field (Tab)"),
    (Action::BackTab, "BackTab", "Previous field (Back Tab)"),
    (Action::NewLine, "NewLine", "New line"),
    (Action::Home, "Home", "Home (first input field)"),
    (Action::EndOfField, "EndOfField", "End of field"),
    (Action::Up, "Up", "Cursor up"),
    (Action::Down, "Down", "Cursor down"),
    (Action::Left, "Left", "Cursor left"),
    (Action::Right, "Right", "Cursor right"),
    (Action::WordLeft, "WordLeft", "Word left"),
    (Action::WordRight, "WordRight", "Word right"),
    (Action::Backspace, "Backspace", "Backspace"),
    (Action::Delete, "Delete", "Delete character"),
    (Action::Insert, "Insert", "Insert mode on/off"),
    (Action::EraseEof, "EraseEOF", "Erase to end of field"),
    (Action::EraseInput, "EraseInput", "Erase all input"),
    (Action::FieldMark, "FieldMark", "Field Mark"),
    (Action::Dup, "Dup", "Duplicate (Dup)"),
    (Action::Copy, "Copy", "Copy"),
    (Action::Cut, "Cut", "Cut"),
    (Action::Paste, "Paste", "Paste"),
    (Action::PrintScreen, "PrintScreen", "Print screen"),
];

/// Characters 3270 users commonly need on a PC keyboard.
const COMMON_CHARS: &[&str] = &["¢", "¬", "¦", "|"];

impl Action {
    /// True for keys that send data to the host (AID keys).
    pub fn is_aid(&self) -> bool {
        matches!(self, Action::Enter | Action::Clear | Action::Pf(_) | Action::Pa(_)
            | Action::SysReq | Action::Attn)
    }

    pub fn category(&self) -> Category {
        match self {
            Action::Pf(_) => Category::ProgramFunction,
            a if a.is_aid() => Category::HostKeys,
            Action::Reset | Action::Tab | Action::BackTab | Action::NewLine | Action::Home
            | Action::EndOfField | Action::Up | Action::Down | Action::Left | Action::Right
            | Action::WordLeft | Action::WordRight => Category::Cursor,
            Action::Copy | Action::Cut | Action::Paste | Action::PrintScreen => Category::Clipboard,
            Action::Text(_) => Category::Characters,
            Action::Macro(_) => Category::Macros,
            _ => Category::Editing,
        }
    }

    /// Name used in keymap files, e.g. `PF13`, `EraseEOF`, `Text "¢"`.
    pub fn token(&self) -> String {
        match self {
            Action::Pf(n) => format!("PF{n}"),
            Action::Pa(n) => format!("PA{n}"),
            Action::Text(s) => {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("Text \"{escaped}\"")
            }
            Action::Macro(name) => format!("Macro {name}"),
            other => SIMPLE.iter().find(|(a, _, _)| a == other).map(|(_, t, _)| t.to_string())
                .unwrap_or_default(),
        }
    }

    /// Human-readable name for menus and dialogs.
    pub fn description(&self) -> String {
        match self {
            Action::Pf(n) => format!("PF{n}"),
            Action::Pa(n) => format!("PA{n}"),
            Action::Text(s) => format!("Type \u{201C}{s}\u{201D}"),
            Action::Macro(name) => format!("Run macro \u{201C}{name}\u{201D}"),
            other => SIMPLE.iter().find(|(a, _, _)| a == other).map(|(_, _, d)| d.to_string())
                .unwrap_or_default(),
        }
    }

    /// Parse a token written by [`Action::token`]. Case-insensitive names.
    pub fn parse(token: &str) -> Option<Action> {
        let t = token.trim();
        if let Some(rest) = t.strip_prefix("Text ") {
            let inner = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
            let mut out = String::new();
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    out.push(chars.next()?);
                } else if c == '"' {
                    return None;
                } else {
                    out.push(c);
                }
            }
            return (!out.is_empty()).then_some(Action::Text(out));
        }
        if let Some(name) = t.strip_prefix("Macro ") {
            let name = name.trim();
            return (!name.is_empty()).then(|| Action::Macro(name.to_string()));
        }
        let upper = t.to_ascii_uppercase();
        for (prefix, max, make) in [("PF", 24u8, Action::Pf as fn(u8) -> Action), ("PA", 3, Action::Pa)] {
            if let Some(n) = upper.strip_prefix(prefix).and_then(|n| n.parse::<u8>().ok()) {
                return (1..=max).contains(&n).then(|| make(n));
            }
        }
        SIMPLE.iter().find(|(_, tok, _)| tok.eq_ignore_ascii_case(t)).map(|(a, _, _)| a.clone())
    }

    /// Every action offered in the Keyboard Setup dialog, in display order.
    pub fn all_bindable() -> Vec<Action> {
        let mut v = vec![Action::Enter, Action::Clear, Action::SysReq, Action::Attn];
        v.extend((1..=3).map(Action::Pa));
        v.extend((1..=24).map(Action::Pf));
        v.extend(SIMPLE.iter().map(|(a, _, _)| a.clone()).filter(|a| !a.is_aid()));
        v.extend(COMMON_CHARS.iter().map(|c| Action::Text(c.to_string())));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_round_trip() {
        for a in Action::all_bindable().into_iter()
            .chain([Action::Text("a \"q\" \\".into()), Action::Macro("logon".into())])
        {
            let tok = a.token();
            assert!(!tok.is_empty(), "{a:?}");
            assert_eq!(Action::parse(&tok), Some(a.clone()), "{tok}");
            assert!(!a.description().is_empty());
        }
    }

    #[test]
    fn parse_limits() {
        assert_eq!(Action::parse("pf24"), Some(Action::Pf(24)));
        assert_eq!(Action::parse("PF25"), None);
        assert_eq!(Action::parse("PA4"), None);
        assert_eq!(Action::parse("erase eof"), None);
        assert_eq!(Action::parse("eraseeof"), Some(Action::EraseEof));
        assert_eq!(Action::parse("Text \"\""), None);
    }

    #[test]
    fn bindable_has_no_duplicates() {
        let all = Action::all_bindable();
        let set: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(set.len(), all.len());
    }
}
