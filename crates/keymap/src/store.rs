//! User-saved keyboard maps: one `.kmp` file per named preset in a folder
//! (the app uses `%APPDATA%\tn3270win\keymaps`).
//!
//! The display name is stored inside the file; the file name is a
//! Windows-safe version of it. Names are compared case-insensitively, as
//! Windows compares file names.

use std::io;
use std::path::{Path, PathBuf};

use crate::keymap::{Keymap, Preset};

pub struct SavedKeymaps {
    dir: PathBuf,
}

/// Longest preset name accepted.
pub const MAX_NAME_LEN: usize = 60;

impl SavedKeymaps {
    pub fn new(dir: impl Into<PathBuf>) -> SavedKeymaps {
        SavedKeymaps { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Why `name` can't be used for a saved preset, if it can't.
    pub fn check_name(name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Please enter a name.".into());
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(format!("Please use a name of at most {MAX_NAME_LEN} characters."));
        }
        if name.eq_ignore_ascii_case("Custom")
            || Preset::ALL.iter().any(|p| p.label().eq_ignore_ascii_case(name))
        {
            return Err(format!("\u{201C}{name}\u{201D} is the name of a built-in preset. Please choose another name."));
        }
        Ok(())
    }

    /// Names of all saved presets, sorted alphabetically.
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.entries().into_iter().map(|(n, _)| n).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    pub fn exists(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    pub fn load(&self, name: &str) -> io::Result<Keymap> {
        let path = self.find(name)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no saved preset named {name}")))?;
        read(&path)
    }

    /// Save under `keymap.name`, replacing a preset of the same name.
    pub fn save(&self, keymap: &Keymap) -> io::Result<PathBuf> {
        Self::check_name(&keymap.name).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        std::fs::create_dir_all(&self.dir)?;
        if let Some(old) = self.find(&keymap.name) {
            std::fs::remove_file(old)?;
        }
        let path = self.free_path(&keymap.name);
        std::fs::write(&path, keymap.to_text())?;
        Ok(path)
    }

    pub fn delete(&self, name: &str) -> io::Result<()> {
        match self.find(name) {
            Some(p) => std::fs::remove_file(p),
            None => Ok(()),
        }
    }

    fn find(&self, name: &str) -> Option<PathBuf> {
        let name = name.trim();
        self.entries().into_iter().find(|(n, _)| n.to_lowercase() == name.to_lowercase()).map(|(_, p)| p)
    }

    /// (display name, path) for every readable `.kmp` file in the folder.
    fn entries(&self) -> Vec<(String, PathBuf)> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        rd.filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("kmp")))
            .filter_map(|p| read(&p).ok().map(|km| (km.name, p)))
            .collect()
    }

    /// A path for `name` that no other file is using.
    fn free_path(&self, name: &str) -> PathBuf {
        let stem = safe_file_stem(name);
        let mut path = self.dir.join(format!("{stem}.kmp"));
        let mut n = 2;
        while path.exists() {
            path = self.dir.join(format!("{stem} ({n}).kmp"));
            n += 1;
        }
        path
    }
}

fn read(path: &Path) -> io::Result<Keymap> {
    let text = std::fs::read_to_string(path)?;
    Keymap::from_text(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Turn a display name into a file name that is valid on Windows.
pub fn safe_file_stem(name: &str) -> String {
    let mut s: String = name.trim().chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect();
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if s.is_empty() {
        s = "keymap".into();
    }
    let base = s.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4 && (base.starts_with("COM") || base.starts_with("LPT"))
            && base.as_bytes()[3].is_ascii_digit());
    if reserved {
        s.insert(0, '_');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tn3270-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn named(name: &str) -> Keymap {
        let mut km = Keymap::preset(Preset::Classic3270);
        km.name = name.into();
        km
    }

    #[test]
    fn save_list_load_delete() {
        let dir = temp_dir("basic");
        let store = SavedKeymaps::new(&dir);
        assert!(store.list().is_empty());
        store.save(&named("Work")).unwrap();
        store.save(&named("alpha keys")).unwrap();
        assert_eq!(store.list(), vec!["alpha keys", "Work"]);
        assert_eq!(store.load("WORK").unwrap().name, "Work");
        store.delete("work").unwrap();
        assert_eq!(store.list(), vec!["alpha keys"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_same_name_replaces() {
        let dir = temp_dir("replace");
        let store = SavedKeymaps::new(&dir);
        store.save(&named("Mine")).unwrap();
        let mut km = named("MINE");
        km.assign(crate::KeyChord::key(crate::vk::F1), crate::Action::Clear);
        store.save(&km).unwrap();
        assert_eq!(store.list(), vec!["MINE"]);
        assert_eq!(store.load("mine").unwrap(), km);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn awkward_names_get_distinct_safe_files() {
        let dir = temp_dir("awkward");
        let store = SavedKeymaps::new(&dir);
        store.save(&named("a/b")).unwrap();
        store.save(&named("a:b")).unwrap(); // same safe stem "a_b"
        store.save(&named("CON")).unwrap();
        assert_eq!(store.list().len(), 3);
        assert!(dir.join("_CON.kmp").exists());
        assert!(dir.join("a_b (2).kmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn names_checked() {
        assert!(SavedKeymaps::check_name("  ").is_err());
        assert!(SavedKeymaps::check_name("custom").is_err());
        assert!(SavedKeymaps::check_name(Preset::PcStyle.label()).is_err());
        assert!(SavedKeymaps::check_name(&"x".repeat(61)).is_err());
        assert!(SavedKeymaps::check_name("My ISPF keys").is_ok());
    }

    #[test]
    fn safe_stems() {
        assert_eq!(safe_file_stem("lpt1.txt"), "_lpt1.txt");
        assert_eq!(safe_file_stem("COM10"), "COM10");
        assert_eq!(safe_file_stem("keys. "), "keys");
        assert_eq!(safe_file_stem("?"), "_");
    }
}
