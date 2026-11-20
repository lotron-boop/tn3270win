use crate::action::Action;
use crate::key::KeyChord;
use crate::keymap::Keymap;

/// Turns raw key events into actions.
///
/// Ordinary keys fire on key-down. A mapping on a modifier key by itself
/// (e.g. "Right Ctrl = Enter", "Left Ctrl = Reset") fires on key-up, and only
/// if no other key was pressed in between, so Ctrl+C and Ctrl+Left still
/// work. AID keys (Enter, PFn, ...) never auto-repeat when held.
#[derive(Debug, Default)]
pub struct KeyDispatcher {
    pending: Option<KeyChord>,
}

impl KeyDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle a key-down. Returns the action to perform now, if any.
    pub fn key_down(&mut self, map: &Keymap, chord: KeyChord, is_repeat: bool) -> Option<Action> {
        if chord.is_modifier_key() {
            if !is_repeat {
                // A second modifier (or AltGr's fake Left Ctrl + Right Alt)
                // replaces any earlier pending one.
                self.pending = map.get(&chord).is_some().then_some(chord);
            }
            return None;
        }
        self.pending = None;
        let action = map.get(&chord)?;
        if is_repeat && action.is_aid() {
            return None;
        }
        Some(action.clone())
    }

    /// Handle a key-up. `chord` only needs the correct `vk` (left/right
    /// resolved). Returns a modifier-only action if one is due.
    pub fn key_up(&mut self, map: &Keymap, chord: KeyChord) -> Option<Action> {
        match self.pending {
            Some(p) if p.vk == chord.vk => {
                self.pending = None;
                map.get(&p).cloned()
            }
            _ => None,
        }
    }

    /// Forget a pending modifier (e.g. when the window loses focus).
    pub fn cancel(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{vk, Mods};
    use crate::keymap::Preset;

    fn classic() -> Keymap {
        Keymap::preset(Preset::Classic3270)
    }

    #[test]
    fn right_ctrl_fires_enter_on_release() {
        let km = classic();
        let mut d = KeyDispatcher::new();
        let rc = KeyChord::key(vk::RCONTROL);
        assert_eq!(d.key_down(&km, rc, false), None);
        assert_eq!(d.key_down(&km, rc, true), None); // held
        assert_eq!(d.key_up(&km, rc), Some(Action::Enter));
        assert_eq!(d.key_up(&km, rc), None);
    }

    #[test]
    fn ctrl_combo_cancels_modifier_action() {
        let km = classic();
        let mut d = KeyDispatcher::new();
        let lc = KeyChord::key(vk::LCONTROL);
        d.key_down(&km, lc, false);
        assert_eq!(d.key_down(&km, KeyChord::new(Mods::CTRL, b'C' as u16), false), Some(Action::Copy));
        assert_eq!(d.key_up(&km, lc), None, "Left Ctrl must not also fire Reset");
    }

    #[test]
    fn altgr_does_not_fire_left_ctrl() {
        let km = classic();
        let mut d = KeyDispatcher::new();
        d.key_down(&km, KeyChord::key(vk::LCONTROL), false);
        d.key_down(&km, KeyChord::new(Mods::CTRL, vk::RMENU), false);
        assert_eq!(d.key_up(&km, KeyChord::key(vk::LCONTROL)), None);
    }

    #[test]
    fn aid_keys_do_not_repeat_but_cursor_keys_do() {
        let km = classic();
        let mut d = KeyDispatcher::new();
        let f3 = KeyChord::key(vk::F1 + 2);
        assert_eq!(d.key_down(&km, f3, false), Some(Action::Pf(3)));
        assert_eq!(d.key_down(&km, f3, true), None);
        let left = KeyChord::key(vk::LEFT);
        assert_eq!(d.key_down(&km, left, true), Some(Action::Left));
    }

    #[test]
    fn unmapped_keys_pass_through() {
        let km = classic();
        let mut d = KeyDispatcher::new();
        assert_eq!(d.key_down(&km, KeyChord::key(b'A' as u16), false), None);
    }
}
