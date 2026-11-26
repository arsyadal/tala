use serde::Deserialize;
use std::{fs, io::ErrorKind, path::PathBuf};

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub default_shell: Option<String>,
    pub scrollback: usize,
    pub mouse: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { default_shell: None, scrollback: 2_000, mouse: true }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|path| path.join("tala").join("config.toml"))
    }

    pub fn load() -> Result<Self, String> {
        let Some(path) = Self::path() else { return Ok(Self::default()); };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        let config: Self = toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        if config.scrollback > 20_000 {
            return Err(format!("{}: scrollback cannot exceed 20,000 rows", path.display()));
        }
        Ok(config)
    }
}
