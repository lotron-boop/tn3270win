//! Keyboard mapping for a TN3270 emulator.
//!
//! This crate is platform-independent (std only) so it can be unit-tested
//! anywhere. Key codes use Windows virtual-key numbering because the first
//! client is a Windows app, but nothing here calls Windows APIs.
//!
//! * [`KeyChord`]: a physical key plus modifiers, telling Left from Right Ctrl
//!   and the numeric-keypad Enter from the main Enter key.
//! * [`Action`]: a 3270 function (Enter, PF1-PF24, Reset, Erase EOF, ...).
//! * [`Keymap`]: chord-to-action table, with presets and a text file format.
//! * [`KeyDispatcher`]: turns key-down/key-up events into actions, firing
//!   modifier-only mappings such as "Right Ctrl = Enter" on release, so that
//!   Ctrl+C still works.

mod action;
mod dispatch;
mod key;
mod keymap;
mod store;

pub use action::{Action, Category};
pub use store::{safe_file_stem, SavedKeymaps, MAX_NAME_LEN as MAX_PRESET_NAME_LEN};
pub use dispatch::KeyDispatcher;
pub use key::{vk, KeyChord, Mods};
pub use keymap::{Keymap, ParseError, Preset};
