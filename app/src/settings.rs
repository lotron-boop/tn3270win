//! Where the keyboard map lives: %APPDATA%\tn3270win\keymap.kmp

use std::path::PathBuf;

use tn3270_keymap::{Keymap, SavedKeymaps};

fn app_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("tn3270win")
}

/// The keyboard map currently in use.
pub fn keymap_path() -> PathBuf {
    app_dir().join("keymap.kmp")
}

/// Named presets saved from Keyboard Setup: %APPDATA%\tn3270win\keymaps
pub fn saved_keymaps() -> SavedKeymaps {
    SavedKeymaps::new(app_dir().join("keymaps"))
}

/// Load the saved map. A missing file gives the default preset; a broken
/// file gives the default preset plus an error message to show the user.
pub fn load_keymap() -> (Keymap, Option<String>) {
    let path = keymap_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match Keymap::from_text(&text) {
            Ok(km) => (km, None),
            Err(e) => (Keymap::default(),
                Some(format!("{} could not be read ({e}). The default keyboard map is being used.", path.display()))),
        },
        Err(_) => (Keymap::default(), None),
    }
}

pub fn save_keymap(km: &Keymap) -> std::io::Result<()> {
    let path = keymap_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, km.to_text())
}

/// Connection settings remembered from the Connect dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionConfig {
    pub host: String,
    pub port: u16,
    pub lu: String,
    pub codepage: u16,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig { host: String::new(), port: 23, lu: String::new(), codepage: 37 }
    }
}

fn session_path() -> PathBuf {
    app_dir().join("session.ini")
}

pub fn load_session() -> SessionConfig {
    let mut cfg = SessionConfig::default();
    let Ok(text) = std::fs::read_to_string(session_path()) else { return cfg };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "host" => cfg.host = v.to_string(),
            "port" => cfg.port = v.parse().unwrap_or(23),
            "lu" => cfg.lu = v.to_string(),
            "codepage" => cfg.codepage = v.parse().unwrap_or(37),
            _ => {}
        }
    }
    cfg
}

pub fn save_session(cfg: &SessionConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(app_dir())?;
    let text = format!(
        "# tn3270win session\nhost = {}\nport = {}\nlu = {}\ncodepage = {}\n",
        cfg.host, cfg.port, cfg.lu, cfg.codepage
    );
    std::fs::write(session_path(), text)
}
