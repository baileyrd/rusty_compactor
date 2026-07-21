use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// User/project-level configuration, loaded from (in priority order):
/// 1. `./.rusty_compactor.toml` (project-local)
/// 2. `~/.rusty_compactor/config.toml` (user-global)
/// 3. Built-in defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Master on/off switch. When false, `run` passes output through untouched.
    pub enabled: bool,
    /// Hard cap on lines kept in compacted output before head/tail truncation kicks in.
    pub max_output_lines: usize,
    /// Lines preserved from the start of output when truncating.
    pub head_lines: usize,
    /// Lines preserved from the end of output when truncating.
    pub tail_lines: usize,
    /// Minimum number of repeats before a line is collapsed into a "(xN)" summary.
    pub dedupe_min_repeats: usize,
    /// Opt-in anonymous usage telemetry (off by default; unused unless a backend is configured).
    pub telemetry: bool,
    /// Path to the JSONL stats log. Defaults to `~/.rusty_compactor/stats.jsonl`.
    pub stats_file: Option<String>,
    /// Path to a TOML file with additional/overriding command rules.
    pub custom_rules_file: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            max_output_lines: 200,
            head_lines: 40,
            tail_lines: 40,
            dedupe_min_repeats: 3,
            telemetry: false,
            stats_file: None,
            custom_rules_file: None,
        }
    }
}

impl Config {
    pub fn user_config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".rusty_compactor")
    }

    pub fn user_config_path() -> PathBuf {
        Self::user_config_dir().join("config.toml")
    }

    pub fn project_config_path() -> PathBuf {
        PathBuf::from(".rusty_compactor.toml")
    }

    /// Load with the priority described in the struct docs, falling back to defaults
    /// if neither file exists or fails to parse.
    pub fn load() -> Self {
        if let Some(cfg) = Self::try_load(&Self::project_config_path()) {
            return cfg;
        }
        if let Some(cfg) = Self::try_load(&Self::user_config_path()) {
            return cfg;
        }
        Config::default()
    }

    fn try_load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        toml::from_str(&text).ok()
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }

    pub fn resolved_stats_file(&self) -> PathBuf {
        match &self.stats_file {
            Some(p) => PathBuf::from(p),
            None => Self::user_config_dir().join("stats.jsonl"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let cfg = Config::default();
        assert!(cfg.enabled);
        assert!(cfg.max_output_lines > cfg.head_lines + cfg.tail_lines);
    }

    #[test]
    fn round_trips_through_toml() {
        let cfg = Config::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg.max_output_lines, back.max_output_lines);
    }
}
