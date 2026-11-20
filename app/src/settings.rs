//! Where the keyboard map lives: %APPDATA%\tn3270win\keymap.kmp

use std::path::PathBuf;

use tn3270_keymap::Keymap;

pub fn keymap_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("tn3270win").join("keymap.kmp")
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
