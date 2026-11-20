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

/// One host session: what the Connect dialog edits and saves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionConfig {
    /// Saved session name; empty for an unsaved (quick) connection.
    pub name: String,
    pub host: String,
    pub port: u16,
    pub lu: String,
    pub codepage: u16,
    /// 3278 model: 2, 3, 4 or 5.
    pub model: u8,
    /// Keyboard preset to use; empty = the current keyboard map.
    pub keymap: String,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            name: String::new(), host: String::new(), port: 23, lu: String::new(),
            codepage: 37, model: 2, keymap: String::new(),
        }
    }
}

impl SessionConfig {
    pub fn to_text(&self) -> String {
        format!(
            "# tn3270win session\nname = {}\nhost = {}\nport = {}\nlu = {}\ncodepage = {}\nmodel = {}\nkeymap = {}\n",
            self.name, self.host, self.port, self.lu, self.codepage, self.model, self.keymap
        )
    }

    pub fn from_text(text: &str) -> SessionConfig {
        let mut cfg = SessionConfig::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "name" => cfg.name = v.to_string(),
                "host" => cfg.host = v.to_string(),
                "port" => cfg.port = v.parse().unwrap_or(23),
                "lu" => cfg.lu = v.to_string(),
                "codepage" => cfg.codepage = v.parse().unwrap_or(37),
                "model" => cfg.model = v.parse().ok().filter(|m| (2..=5).contains(m)).unwrap_or(2),
                "keymap" => cfg.keymap = v.to_string(),
                _ => {}
            }
        }
        cfg
    }
}

fn last_session_path() -> PathBuf {
    app_dir().join("session.ini")
}

/// What was in the Connect dialog last time.
pub fn load_last_session() -> SessionConfig {
    std::fs::read_to_string(last_session_path()).map(|t| SessionConfig::from_text(&t)).unwrap_or_default()
}

pub fn save_last_session(cfg: &SessionConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(app_dir())?;
    std::fs::write(last_session_path(), cfg.to_text())
}

/// Saved sessions: one `.ini` file each in %APPDATA%\tn3270win\sessions.
pub struct SessionStore {
    dir: PathBuf,
}

pub fn sessions() -> SessionStore {
    SessionStore { dir: app_dir().join("sessions") }
}

impl SessionStore {
    #[cfg(test)]
    fn at(dir: PathBuf) -> SessionStore {
        SessionStore { dir }
    }

    fn entries(&self) -> Vec<(SessionConfig, PathBuf)> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        rd.filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ini")))
            .filter_map(|p| {
                let cfg = SessionConfig::from_text(&std::fs::read_to_string(&p).ok()?);
                (!cfg.name.is_empty()).then_some((cfg, p))
            })
            .collect()
    }

    fn find(&self, name: &str) -> Option<(SessionConfig, PathBuf)> {
        let name = name.trim().to_lowercase();
        self.entries().into_iter().find(|(c, _)| c.name.to_lowercase() == name)
    }

    /// Saved sessions sorted by name.
    pub fn list(&self) -> Vec<SessionConfig> {
        let mut v: Vec<SessionConfig> = self.entries().into_iter().map(|(c, _)| c).collect();
        v.sort_by_key(|c| c.name.to_lowercase());
        v
    }

    pub fn exists(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// Save under `cfg.name`, replacing a session of the same name
    /// (names compare without regard to case, like Windows file names).
    pub fn save(&self, cfg: &SessionConfig) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        if let Some((_, old)) = self.find(&cfg.name) {
            std::fs::remove_file(old)?;
        }
        let stem = tn3270_keymap::safe_file_stem(&cfg.name);
        let mut path = self.dir.join(format!("{stem}.ini"));
        let mut n = 2;
        while path.exists() {
            path = self.dir.join(format!("{stem} ({n}).ini"));
            n += 1;
        }
        std::fs::write(path, cfg.to_text())
    }

    pub fn delete(&self, name: &str) -> std::io::Result<()> {
        match self.find(name) {
            Some((_, p)) => std::fs::remove_file(p),
            None => Ok(()),
        }
    }
}

/// View settings: View > Fit Font to Window, Larger/Smaller Font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewConfig {
    pub fit: bool,
    pub font_height: i32,
}

impl Default for ViewConfig {
    fn default() -> Self {
        ViewConfig { fit: true, font_height: 18 }
    }
}

fn view_path() -> PathBuf {
    app_dir().join("view.ini")
}

pub fn load_view() -> ViewConfig {
    let mut v = ViewConfig::default();
    let Ok(text) = std::fs::read_to_string(view_path()) else { return v };
    for line in text.lines() {
        let Some((k, val)) = line.split_once('=') else { continue };
        match k.trim() {
            "fit" => v.fit = val.trim() != "0",
            "font_height" => v.font_height = val.trim().parse().unwrap_or(18).clamp(8, 72),
            _ => {}
        }
    }
    v
}

pub fn save_view(v: &ViewConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(app_dir())?;
    std::fs::write(view_path(), format!("fit = {}\nfont_height = {}\n", v.fit as u8, v.font_height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_text_round_trip() {
        let cfg = SessionConfig {
            name: "TK5 at home".into(), host: "192.168.2.7".into(), port: 3270, lu: "CUU0C0".into(),
            codepage: 1140, model: 4, keymap: "My keys".into(),
        };
        assert_eq!(SessionConfig::from_text(&cfg.to_text()), cfg);
        assert_eq!(SessionConfig::from_text("model = 9").model, 2);
    }

    #[test]
    fn store_save_list_replace_delete() {
        let dir = std::env::temp_dir().join(format!("tn3270win-sessions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SessionStore::at(dir.clone());
        let mut a = SessionConfig { name: "Work: prod".into(), host: "a".into(), ..Default::default() };
        store.save(&a).unwrap();
        store.save(&SessionConfig { name: "alpha".into(), ..Default::default() }).unwrap();
        assert_eq!(store.list().iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["alpha", "Work: prod"]);
        a.host = "b".into();
        a.name = "WORK: PROD".into();
        store.save(&a).unwrap();
        let l = store.list();
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].host, "b");
        assert!(dir.join("WORK_ PROD.ini").exists(), "Windows-safe file name");
        store.delete("work: prod").unwrap();
        assert_eq!(store.list().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
