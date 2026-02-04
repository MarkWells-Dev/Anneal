// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Configuration loading and management.
//!
//! Configuration uses a flat key=value format (no sections). Missing keys use defaults.
//! Missing file uses all defaults.

use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

use crate::version::Threshold;

/// System configuration file path.
pub const CONFIG_PATH: &str = "/etc/anneal/config.conf";

/// Known AUR helpers with built-in invocation support.
pub const KNOWN_HELPERS: &[&str] = &["paru", "yay", "pikaur", "aura", "trizen"];

/// Configuration for Anneal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Version threshold for triggering rebuilds.
    pub version_threshold: Threshold,

    /// AUR helper command (e.g., "paru" or "my-helper -S --rebuild").
    /// None means auto-detect at rebuild time.
    pub helper: Option<String>,

    /// Whether to include checkrebuild results in rebuild by default.
    pub include_checkrebuild: bool,

    /// Days to retain trigger event history (0 to disable pruning).
    pub retention_days: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version_threshold: Threshold::Minor,
            helper: None,
            include_checkrebuild: false,
            retention_days: 90,
        }
    }
}

impl Config {
    /// Load configuration from the default system path.
    ///
    /// Returns default config if file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the config file exists but cannot be read or parsed.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(Path::new(CONFIG_PATH))
    }

    /// Load configuration from a specific path.
    ///
    /// Returns default config if file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the config file exists but cannot be read or parsed.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(contents) => Self::parse(&contents),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ConfigError::Io(e)),
        }
    }

    /// Parse configuration from a string.
    fn parse(contents: &str) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        for (line_num, line) in contents.lines().enumerate() {
            let line_num = line_num + 1; // 1-indexed for error messages

            // Skip empty lines and comments
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse key=value
            let Some((key, value)) = line.split_once('=') else {
                return Err(ConfigError::Parse {
                    line: line_num,
                    message: "expected 'key = value' format".into(),
                });
            };

            let key = key.trim();
            let value = value.trim();

            match key {
                "version_threshold" => {
                    config.version_threshold =
                        Threshold::from_str(value).map_err(|_| ConfigError::Parse {
                            line: line_num,
                            message: format!(
                                "invalid version_threshold '{value}', expected: major, minor, patch, always"
                            ),
                        })?;
                }
                "helper" => {
                    if value.is_empty() {
                        config.helper = None;
                    } else {
                        config.helper = Some(value.to_string());
                    }
                }
                "include_checkrebuild" => {
                    config.include_checkrebuild = parse_bool(value).ok_or(ConfigError::Parse {
                        line: line_num,
                        message: format!(
                            "invalid include_checkrebuild '{value}', expected: true, false"
                        ),
                    })?;
                }
                "retention_days" => {
                    config.retention_days = value.parse().map_err(|_| ConfigError::Parse {
                        line: line_num,
                        message: format!(
                            "invalid retention_days '{value}', expected non-negative integer"
                        ),
                    })?;
                }
                _ => {
                    return Err(ConfigError::Parse {
                        line: line_num,
                        message: format!("unknown key '{key}'"),
                    });
                }
            }
        }

        Ok(config)
    }

    /// Serialize configuration to the conf file format.
    pub fn to_conf(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "version_threshold = {}\n",
            self.version_threshold.as_str()
        ));

        match &self.helper {
            Some(helper) => output.push_str(&format!("helper = {helper}\n")),
            None => output.push_str("# helper =\n"),
        }

        output.push_str(&format!(
            "include_checkrebuild = {}\n",
            self.include_checkrebuild
        ));

        output.push_str(&format!("retention_days = {}\n", self.retention_days));

        output
    }

    /// Check if a helper name is a known helper with built-in invocation.
    pub fn is_known_helper(name: &str) -> bool {
        KNOWN_HELPERS.contains(&name)
    }
}

/// Parse a boolean value from common representations.
fn parse_bool(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// Configuration loading errors.
#[derive(Debug)]
pub enum ConfigError {
    /// I/O error reading config file.
    Io(io::Error),
    /// Parse error in config file.
    Parse {
        /// Line number (1-indexed) where the error occurred.
        line: usize,
        /// Description of the parse error.
        message: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "failed to read config: {e}"),
            Self::Parse { line, message } => write!(f, "config line {line}: {message}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse { .. } => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = Config::default();
        assert_eq!(config.version_threshold, Threshold::Minor);
        assert_eq!(config.helper, None);
        assert!(!config.include_checkrebuild);
        assert_eq!(config.retention_days, 90);
    }

    #[test]
    fn parse_empty_file() {
        let config = Config::parse("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn parse_comments_and_blank_lines() {
        let config = Config::parse(
            r"
# This is a comment
   # Indented comment

version_threshold = major
",
        )
        .unwrap();
        assert_eq!(config.version_threshold, Threshold::Major);
    }

    #[test]
    fn parse_all_options() {
        let config = Config::parse(
            r"
version_threshold = patch
helper = yay
include_checkrebuild = true
retention_days = 30
",
        )
        .unwrap();

        assert_eq!(config.version_threshold, Threshold::Patch);
        assert_eq!(config.helper, Some("yay".into()));
        assert!(config.include_checkrebuild);
        assert_eq!(config.retention_days, 30);
    }

    #[test]
    fn parse_custom_helper_command() {
        let config = Config::parse("helper = my-helper -S --rebuild").unwrap();
        assert_eq!(config.helper, Some("my-helper -S --rebuild".into()));
    }

    #[test]
    fn parse_empty_helper() {
        let config = Config::parse("helper =").unwrap();
        assert_eq!(config.helper, None);
    }

    #[test]
    fn parse_bool_variants() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("True"), Some(true));
        assert_eq!(parse_bool("TRUE"), Some(true));
        assert_eq!(parse_bool("yes"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));

        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("False"), Some(false));
        assert_eq!(parse_bool("no"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));

        assert_eq!(parse_bool("maybe"), None);
        assert_eq!(parse_bool(""), None);
    }

    #[test]
    fn parse_error_missing_equals() {
        let err = Config::parse("version_threshold minor").unwrap_err();
        assert!(matches!(err, ConfigError::Parse { line: 1, .. }));
    }

    #[test]
    fn parse_error_unknown_key() {
        let err = Config::parse("unknown_key = value").unwrap_err();
        match err {
            ConfigError::Parse { line, message } => {
                assert_eq!(line, 1);
                assert!(message.contains("unknown key"));
            }
            _ => panic!("expected parse error"),
        }
    }

    #[test]
    fn parse_error_invalid_threshold() {
        let err = Config::parse("version_threshold = invalid").unwrap_err();
        match err {
            ConfigError::Parse { line, message } => {
                assert_eq!(line, 1);
                assert!(message.contains("invalid version_threshold"));
            }
            _ => panic!("expected parse error"),
        }
    }

    #[test]
    fn parse_error_invalid_bool() {
        let err = Config::parse("include_checkrebuild = maybe").unwrap_err();
        assert!(matches!(err, ConfigError::Parse { line: 1, .. }));
    }

    #[test]
    fn parse_error_invalid_retention() {
        let err = Config::parse("retention_days = -1").unwrap_err();
        assert!(matches!(err, ConfigError::Parse { line: 1, .. }));
    }

    #[test]
    fn to_conf_roundtrip() {
        let config = Config {
            version_threshold: Threshold::Patch,
            helper: Some("paru".into()),
            include_checkrebuild: true,
            retention_days: 60,
        };

        let serialized = config.to_conf();
        let parsed = Config::parse(&serialized).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn to_conf_no_helper() {
        let config = Config::default();
        let serialized = config.to_conf();
        assert!(serialized.contains("# helper ="));
    }

    #[test]
    fn known_helpers() {
        assert!(Config::is_known_helper("paru"));
        assert!(Config::is_known_helper("yay"));
        assert!(Config::is_known_helper("pikaur"));
        assert!(Config::is_known_helper("aura"));
        assert!(Config::is_known_helper("trizen"));
        assert!(!Config::is_known_helper("pacman"));
        assert!(!Config::is_known_helper("custom-helper"));
    }
}
