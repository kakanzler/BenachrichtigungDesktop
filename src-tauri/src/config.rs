//! Where each app's database and executable live. Defaults match the installed
//! apps; `config.json` in the app data folder may override any of them.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const ENGLISH_EXE: &str = r"D:\Program Files\my_original_app\ai-english-coach\app.exe";
const FORSCHER_INSTALL_DIR: &str = r"D:\Program Files\Forscher";
const ZUKUNFT_EXE: &str = r"D:\Program Files\Zukunft\zukunft.exe";

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Overrides {
    pub english_db: Option<PathBuf>,
    pub english_exe: Option<PathBuf>,
    pub forscher_db: Option<PathBuf>,
    pub forscher_exe: Option<PathBuf>,
    pub zukunft_exe: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub english_db: PathBuf,
    pub english_exe: PathBuf,
    pub forscher_db: PathBuf,
    pub forscher_exe: PathBuf,
    /// Launch-only: Zukunft has no review counts, just a button in the header.
    pub zukunft_exe: PathBuf,
}

impl Paths {
    fn resolve(o: Overrides, roaming: &Path, forscher_dir: PathBuf) -> Self {
        Paths {
            english_db: o
                .english_db
                .unwrap_or_else(|| roaming.join(r"com.aicoach.english\coach.db")),
            english_exe: o.english_exe.unwrap_or_else(|| ENGLISH_EXE.into()),
            forscher_db: o
                .forscher_db
                .unwrap_or_else(|| roaming.join(r"com.forscher.app\forscher.db")),
            forscher_exe: o
                .forscher_exe
                .unwrap_or_else(|| forscher_dir.join("forscher.exe")),
            zukunft_exe: o.zukunft_exe.unwrap_or_else(|| ZUKUNFT_EXE.into()),
        }
    }
}

fn forscher_install_dir() -> PathBuf {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Forscher\Forscher")
        .and_then(|key| key.get_value::<String, _>(""))
        .map_or_else(
            |_| FORSCHER_INSTALL_DIR.into(),
            |dir| PathBuf::from(dir.trim_matches('"')),
        )
}

/// Returns the resolved paths and, if `config.json` is present but unusable,
/// the reason. An absent file is the normal case.
pub fn load(config_file: &Path, roaming: &Path) -> (Paths, Option<String>) {
    let (overrides, problem) = match std::fs::read_to_string(config_file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Overrides::default(), None),
        Err(e) => (Overrides::default(), Some(e.to_string())),
        Ok(text) => match serde_json::from_str(&text) {
            Ok(o) => (o, None),
            Err(e) => (Overrides::default(), Some(e.to_string())),
        },
    };
    let problem = problem.map(|e| format!("{}: {e}", config_file.display()));
    (
        Paths::resolve(overrides, roaming, forscher_install_dir()),
        problem,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_point_at_each_apps_own_data_folder() {
        let paths = Paths::resolve(
            Overrides::default(),
            Path::new(r"C:\Roaming"),
            PathBuf::from(r"E:\Apps\Forscher"),
        );
        assert_eq!(
            paths.english_db,
            PathBuf::from(r"C:\Roaming\com.aicoach.english\coach.db")
        );
        assert_eq!(
            paths.forscher_db,
            PathBuf::from(r"C:\Roaming\com.forscher.app\forscher.db")
        );
        assert_eq!(
            paths.forscher_exe,
            PathBuf::from(r"E:\Apps\Forscher\forscher.exe")
        );
    }

    #[test]
    fn overrides_replace_only_what_they_name() {
        let o: Overrides = serde_json::from_str(r#"{"englishExe": "C:\\x\\a.exe"}"#).unwrap();
        let paths = Paths::resolve(o, Path::new(r"C:\Roaming"), PathBuf::from(r"E:\F"));
        assert_eq!(paths.english_exe, PathBuf::from(r"C:\x\a.exe"));
        assert_eq!(paths.forscher_exe, PathBuf::from(r"E:\F\forscher.exe"));
    }

    #[test]
    fn zukunft_exe_defaults_to_its_install_dir_and_can_be_overridden() {
        let paths = Paths::resolve(
            Overrides::default(),
            Path::new(r"C:\R"),
            PathBuf::from(r"E:\F"),
        );
        assert_eq!(paths.zukunft_exe, PathBuf::from(ZUKUNFT_EXE));
        let o: Overrides = serde_json::from_str(r#"{"zukunftExe": "C:\\z\\zukunft.exe"}"#).unwrap();
        let paths = Paths::resolve(o, Path::new(r"C:\R"), PathBuf::from(r"E:\F"));
        assert_eq!(paths.zukunft_exe, PathBuf::from(r"C:\z\zukunft.exe"));
    }

    #[test]
    fn broken_config_falls_back_and_reports_the_problem() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        std::fs::write(&file, "{ not json").unwrap();
        let (paths, problem) = load(&file, Path::new(r"C:\Roaming"));
        assert_eq!(paths.english_exe, PathBuf::from(ENGLISH_EXE));
        assert!(problem.is_some());
    }

    #[test]
    fn absent_config_is_not_a_problem() {
        let dir = tempfile::tempdir().unwrap();
        let (_, problem) = load(&dir.path().join("config.json"), Path::new(r"C:\Roaming"));
        assert_eq!(problem, None);
    }
}
