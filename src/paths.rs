//! Where the player looks for ROMs and keeps its settings.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "Parrot Cart MIDI";

/// The user data folder:
/// macOS ~/Library/Application Support/Parrot Cart MIDI,
/// Windows %APPDATA%\Parrot Cart MIDI,
/// Linux ~/.local/share/parrot-cart-midi (or $XDG_DATA_HOME/parrot-cart-midi).
pub fn data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support").join(APP_NAME))
    } else if cfg!(windows) {
        env::var_os("APPDATA").map(|a| PathBuf::from(a).join(APP_NAME))
    } else {
        let base = match env::var_os("XDG_DATA_HOME") {
            Some(x) if Path::new(&x).is_absolute() => PathBuf::from(x),
            _ => home()?.join(".local/share"),
        };
        Some(base.join("parrot-cart-midi"))
    }
}

fn home() -> Option<PathBuf> {
    env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// "roms" next to the program. For a macOS app, that is next to the .app
/// (and, as a fallback, next to the program inside it).
pub fn program_rom_dirs() -> Vec<PathBuf> {
    let Ok(exe) = env::current_exe() else { return Vec::new() };
    // Follow links (a symlink in ~/bin, say). Not on Windows, where this
    // would give a \\?\ path.
    #[cfg(not(windows))]
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    let mut dirs = Vec::new();
    if let Some(dir) = exe.parent() {
        // .../Name.app/Contents/MacOS/parrot-midi
        if dir.ends_with("Contents/MacOS")
            && let Some(bundle) = dir.parent().and_then(Path::parent)
            && bundle.extension().is_some_and(|e| e == "app")
            && let Some(outside) = bundle.parent()
        {
            dirs.push(outside.join("roms"));
        }
        dirs.push(dir.join("roms"));
    }
    dirs
}

/// The folders searched for ROMs, in order: the one given with --roms, the
/// one chosen in the window, "roms" next to the program, the data folder.
pub fn rom_search_list(cli: Option<&Path>, chosen: Option<&Path>) -> Vec<PathBuf> {
    let mut list = Vec::new();
    let mut add = |p: PathBuf| {
        if !list.contains(&p) {
            list.push(p);
        }
    };
    if let Some(p) = cli {
        add(p.to_path_buf());
    }
    if let Some(p) = chosen {
        add(p.to_path_buf());
    }
    for p in program_rom_dirs() {
        add(p);
    }
    if let Some(d) = data_dir() {
        add(d);
    }
    list
}

/// Settings remembered between runs, in settings.txt in the data folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub rom_folder: Option<PathBuf>,
    pub volume: f32,
    /// None: the built-in MT-32. Some(name): that MIDI output.
    pub output: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { rom_folder: None, volume: 0.8, output: None }
    }
}

impl Settings {
    fn file() -> Option<PathBuf> {
        data_dir().map(|d| d.join("settings.txt"))
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Some(text) = Settings::file().and_then(|f| fs::read_to_string(f).ok()) else { return s };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            match key.trim() {
                "rom_folder" if !value.is_empty() => s.rom_folder = Some(PathBuf::from(value)),
                "volume" => s.volume = value.parse::<f32>().unwrap_or(s.volume).clamp(0.0, 1.0),
                "output" if !value.is_empty() => s.output = Some(value.to_string()),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let Some(file) = Settings::file() else { return };
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let text = format!(
            "# Parrot Cart MIDI settings\nrom_folder={}\nvolume={:.2}\noutput={}\n",
            self.rom_folder.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            self.volume,
            self.output.clone().unwrap_or_default()
        );
        let _ = fs::write(file, text);
    }
}
