use crate::theme::ThemeName;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io::ErrorKind, path::{Path, PathBuf}};

const MAX_RECENT_FOLDERS: usize = 12;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CustomAgentConfig {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub default_shell: Option<String>,
    pub scrollback: usize,
    pub mouse: bool,
    pub theme: ThemeName,
    pub agents: Vec<CustomAgentConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_shell: None,
            scrollback: 2_000,
            mouse: true,
            theme: ThemeName::Dark,
            agents: Vec::new(),
        }
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
        Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Self = toml::from_str(text).map_err(|error| error.to_string())?;
        if config.scrollback > 20_000 {
            return Err("scrollback cannot exceed 20,000 rows".to_owned());
        }
        let mut seen = HashSet::new();
        for agent in &config.agents {
            let id = agent.id.trim();
            if id.is_empty() || agent.label.trim().is_empty() || agent.command.trim().is_empty() {
                return Err("custom agent entries require non-empty id, label, and command".to_owned());
            }
            if !seen.insert(id.to_owned()) {
                return Err(format!("duplicate custom agent id {id:?}"));
            }
        }
        Ok(config)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct WorkspaceState {
    pub recent_folders: Vec<PathBuf>,
}

impl WorkspaceState {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|path| path.join("tala").join("state.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else { return Self::default(); };
        let Ok(text) = fs::read_to_string(path) else { return Self::default(); };
        toml::from_str(&text).unwrap_or_default()
    }

    pub fn record_folder(&mut self, folder: &Path) {
        self.recent_folders.retain(|item| item != folder);
        self.recent_folders.insert(0, folder.to_path_buf());
        self.recent_folders.truncate(MAX_RECENT_FOLDERS);
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return; };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(serialized) = toml::to_string_pretty(self) {
            let _ = fs::write(path, serialized);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_theme_and_custom_agents() {
        let parsed = Config::parse(
            r#"
scrollback = 4000
mouse = false
theme = "ocean"

[[agents]]
id = "claude"
label = "Claude Code"
command = "claude"
args = ["--model", "sonnet"]
"#,
        )
        .unwrap();
        assert_eq!(parsed.scrollback, 4000);
        assert!(!parsed.mouse);
        assert_eq!(parsed.theme, ThemeName::Ocean);
        assert_eq!(parsed.agents.len(), 1);
        assert_eq!(parsed.agents[0].id, "claude");
        assert_eq!(parsed.agents[0].args, vec!["--model", "sonnet"]);
    }

    #[test]
    fn rejects_duplicate_agent_ids_and_unknown_keys() {
        assert!(Config::parse("unknown_key = true").is_err());
        assert!(Config::parse("scrollback = 25000").is_err());
        let dup = r#"
[[agents]]
id = "bot"
label = "Bot 1"
command = "bot"
[[agents]]
id = "bot"
label = "Bot 2"
command = "bot2"
"#;
        assert!(Config::parse(dup).is_err());
    }

    #[test]
    fn recent_folders_deduplicates_and_caps_entries() {
        let mut state = WorkspaceState::default();
        for i in 0..16 {
            state.record_folder(Path::new(&format!("folder-{i}")));
        }
        state.record_folder(Path::new("folder-5"));
        assert_eq!(state.recent_folders.len(), MAX_RECENT_FOLDERS);
        assert_eq!(state.recent_folders[0], PathBuf::from("folder-5"));
    }
}
