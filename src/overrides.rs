// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! User override system for triggers and packages.
//!
//! Allows users to customize trigger behavior via config files:
//! - `/etc/anneal/triggers/<trigger>.conf` - Override what packages a trigger marks
//! - `/etc/anneal/packages/<package>.conf` - Override what triggers can mark a package
//!
//! ## File Format
//!
//! Line-delimited patterns with `#` comments:
//! ```text
//! # This is a comment
//! package-name
//! prefix-*      # Glob pattern
//! ```
//!
//! Empty file = disable trigger / never mark package.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;

/// Directory containing trigger override files.
pub const TRIGGERS_DIR: &str = "/etc/anneal/triggers";

/// Directory containing package override files.
pub const PACKAGES_DIR: &str = "/etc/anneal/packages";

/// Loaded user overrides.
#[derive(Debug, Default)]
pub struct Overrides {
    /// Trigger overrides keyed by trigger name.
    triggers: HashMap<String, TriggerOverride>,
    /// Package overrides keyed by package name.
    packages: HashMap<String, PackageOverride>,
}

/// Override for a trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerOverride {
    /// Trigger is disabled (empty file).
    Disabled,
    /// Trigger marks packages matching these patterns.
    Patterns(Vec<String>),
}

/// Override for a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageOverride {
    /// Package is never marked (empty file).
    NeverMark,
    /// Package is only marked by these triggers.
    OnlyTriggers(Vec<String>),
}

impl Overrides {
    /// Load overrides from the system directories.
    ///
    /// Missing directories are silently ignored.
    pub fn load() -> Self {
        Self::load_from_paths(Path::new(TRIGGERS_DIR), Path::new(PACKAGES_DIR))
    }

    /// Load overrides from custom directories.
    ///
    /// This is useful for testing without requiring root access.
    /// Missing directories are silently ignored.
    pub fn load_from_paths(triggers_dir: &Path, packages_dir: &Path) -> Self {
        let mut overrides = Self::default();

        // Load trigger overrides
        if let Ok(entries) = fs::read_dir(triggers_dir) {
            for entry in entries.flatten() {
                if let Some((name, override_)) = Self::load_trigger_entry(&entry) {
                    overrides.triggers.insert(name, override_);
                }
            }
        }

        // Load package overrides
        if let Ok(entries) = fs::read_dir(packages_dir) {
            for entry in entries.flatten() {
                if let Some((name, override_)) = Self::load_package_entry(&entry) {
                    overrides.packages.insert(name, override_);
                }
            }
        }

        overrides
    }

    /// Load a single trigger override entry.
    fn load_trigger_entry(entry: &fs::DirEntry) -> Option<(String, TriggerOverride)> {
        let path = entry.path();
        if path.extension()? != "conf" {
            return None;
        }
        let name = path.file_stem()?.to_str()?.to_string();
        let override_ = TriggerOverride::load(&path).ok()?;
        Some((name, override_))
    }

    /// Load a single package override entry.
    fn load_package_entry(entry: &fs::DirEntry) -> Option<(String, PackageOverride)> {
        let path = entry.path();
        if path.extension()? != "conf" {
            return None;
        }
        let name = path.file_stem()?.to_str()?.to_string();
        let override_ = PackageOverride::load(&path).ok()?;
        Some((name, override_))
    }

    /// Check if a package name is a trigger (has an override file).
    ///
    /// Note: This only checks for user-defined triggers. Curated triggers
    /// are checked separately.
    pub fn is_user_trigger(&self, name: &str) -> bool {
        self.triggers.contains_key(name)
    }

    /// Get the target packages for a trigger override.
    ///
    /// Returns:
    /// - `Some(vec)` if there's an override (may be empty if disabled)
    /// - `None` if no override exists (use default pactree behavior)
    pub fn get_trigger_targets(
        &self,
        trigger: &str,
        aur_packages: &HashSet<String>,
    ) -> Option<Vec<String>> {
        let override_ = self.triggers.get(trigger)?;

        match override_ {
            TriggerOverride::Disabled => Some(Vec::new()),
            TriggerOverride::Patterns(patterns) => {
                let targets: Vec<String> = aur_packages
                    .iter()
                    .filter(|pkg| {
                        patterns.iter().any(|pattern| matches_glob(pattern, pkg))
                            && !pkg.ends_with("-bin")
                    })
                    .cloned()
                    .collect();
                Some(targets)
            }
        }
    }

    /// Check if a package should be marked by a trigger.
    ///
    /// Returns:
    /// - `true` if no override exists (default behavior)
    /// - `true` if override allows this trigger
    /// - `false` if override blocks this trigger or marks never
    pub fn should_mark_package(&self, package: &str, trigger: &str) -> bool {
        let Some(override_) = self.packages.get(package) else {
            // No override, use default behavior
            return true;
        };

        match override_ {
            PackageOverride::NeverMark => false,
            PackageOverride::OnlyTriggers(allowed) => {
                allowed.iter().any(|pattern| matches_glob(pattern, trigger))
            }
        }
    }

    /// List all user-defined trigger names.
    pub fn user_triggers(&self) -> impl Iterator<Item = &str> {
        self.triggers.keys().map(String::as_str)
    }
}

impl TriggerOverride {
    /// Load a trigger override from a file.
    fn load(path: &Path) -> io::Result<Self> {
        let patterns = parse_override_file(path)?;
        if patterns.is_empty() {
            Ok(Self::Disabled)
        } else {
            Ok(Self::Patterns(patterns))
        }
    }
}

impl PackageOverride {
    /// Load a package override from a file.
    fn load(path: &Path) -> io::Result<Self> {
        let triggers = parse_override_file(path)?;
        if triggers.is_empty() {
            Ok(Self::NeverMark)
        } else {
            Ok(Self::OnlyTriggers(triggers))
        }
    }
}

/// Parse an override file into a list of patterns.
///
/// - Skips empty lines
/// - Skips lines starting with `#`
/// - Trims whitespace
fn parse_override_file(path: &Path) -> io::Result<Vec<String>> {
    let content = fs::read_to_string(path)?;

    let patterns: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect();

    Ok(patterns)
}

/// Match a glob pattern against a string.
///
/// Supports:
/// - `*` matches any sequence of characters (including empty)
/// - `?` matches any single character
/// - All other characters match literally
pub fn matches_glob(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    matches_glob_recursive(&pattern, &text)
}

fn matches_glob_recursive(pattern: &[char], text: &[char]) -> bool {
    match (pattern.first(), text.first()) {
        // Both empty - match
        (None, None) => true,

        // Pattern empty but text remains - no match
        (None, Some(_)) => false,

        // Pattern has '*'
        (Some('*'), _) => {
            // Try matching zero characters (skip the *)
            if matches_glob_recursive(&pattern[1..], text) {
                return true;
            }
            // Try matching one or more characters (consume one from text, keep *)
            if !text.is_empty() && matches_glob_recursive(pattern, &text[1..]) {
                return true;
            }
            false
        }

        // Pattern has '?' - match any single character
        (Some('?'), Some(_)) => matches_glob_recursive(&pattern[1..], &text[1..]),
        (Some('?'), None) => false,

        // Literal character match
        (Some(p), Some(t)) if *p == *t => matches_glob_recursive(&pattern[1..], &text[1..]),

        // No match
        _ => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    mod glob_matching {
        use super::*;

        #[test]
        fn exact_match() {
            assert!(matches_glob("hello", "hello"));
            assert!(!matches_glob("hello", "world"));
            assert!(!matches_glob("hello", "hello!"));
            assert!(!matches_glob("hello!", "hello"));
        }

        #[test]
        fn asterisk_suffix() {
            assert!(matches_glob("hello*", "hello"));
            assert!(matches_glob("hello*", "helloworld"));
            assert!(matches_glob("hello*", "hello-world"));
            assert!(!matches_glob("hello*", "hell"));
            assert!(!matches_glob("hello*", "world"));
        }

        #[test]
        fn asterisk_prefix() {
            assert!(matches_glob("*world", "world"));
            assert!(matches_glob("*world", "helloworld"));
            assert!(matches_glob("*world", "hello-world"));
            assert!(!matches_glob("*world", "worlds"));
        }

        #[test]
        fn asterisk_middle() {
            assert!(matches_glob("h*o", "ho"));
            assert!(matches_glob("h*o", "hello"));
            assert!(matches_glob("h*o", "hxxxxo"));
            assert!(!matches_glob("h*o", "helloX"));
        }

        #[test]
        fn asterisk_only() {
            assert!(matches_glob("*", ""));
            assert!(matches_glob("*", "anything"));
            assert!(matches_glob("*", "hello-world"));
        }

        #[test]
        fn multiple_asterisks() {
            assert!(matches_glob("*-*", "hello-world"));
            assert!(matches_glob("*-*", "-"));
            assert!(matches_glob("a*b*c", "abc"));
            assert!(matches_glob("a*b*c", "aXXbYYc"));
            assert!(!matches_glob("a*b*c", "ac"));
        }

        #[test]
        fn question_mark() {
            assert!(matches_glob("h?llo", "hello"));
            assert!(matches_glob("h?llo", "hallo"));
            assert!(!matches_glob("h?llo", "hllo"));
            assert!(!matches_glob("h?llo", "heello"));
        }

        #[test]
        fn combined_wildcards() {
            assert!(matches_glob("*-git", "foo-git"));
            assert!(matches_glob("*-git", "bar-baz-git"));
            assert!(!matches_glob("*-git", "foo-git-extra"));

            assert!(matches_glob("qt?-*", "qt6-base"));
            assert!(matches_glob("qt?-*", "qt5-svg"));
            assert!(!matches_glob("qt?-*", "qt-base"));
        }

        #[test]
        fn empty_strings() {
            assert!(matches_glob("", ""));
            assert!(!matches_glob("", "x"));
            assert!(matches_glob("*", ""));
            assert!(!matches_glob("?", ""));
        }

        #[test]
        fn package_name_patterns() {
            // Common patterns for AUR packages
            assert!(matches_glob("*-bin", "discord-bin"));
            assert!(matches_glob("*-git", "neovim-git"));
            assert!(matches_glob("python-*", "python-requests"));
            assert!(matches_glob("lib32-*", "lib32-mesa"));
        }
    }

    mod override_parsing {
        use super::*;
        use std::io::Write;
        use tempfile::NamedTempFile;

        fn write_temp_file(content: &str) -> NamedTempFile {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(content.as_bytes()).unwrap();
            file
        }

        #[test]
        fn parse_simple_patterns() {
            let file = write_temp_file("pkg1\npkg2\npkg3\n");
            let patterns = parse_override_file(file.path()).unwrap();
            assert_eq!(patterns, vec!["pkg1", "pkg2", "pkg3"]);
        }

        #[test]
        fn parse_with_comments() {
            let file = write_temp_file("# Comment\npkg1\n# Another comment\npkg2\n");
            let patterns = parse_override_file(file.path()).unwrap();
            assert_eq!(patterns, vec!["pkg1", "pkg2"]);
        }

        #[test]
        fn parse_with_blank_lines() {
            let file = write_temp_file("pkg1\n\n\npkg2\n");
            let patterns = parse_override_file(file.path()).unwrap();
            assert_eq!(patterns, vec!["pkg1", "pkg2"]);
        }

        #[test]
        fn parse_with_whitespace() {
            let file = write_temp_file("  pkg1  \n\tpkg2\t\n");
            let patterns = parse_override_file(file.path()).unwrap();
            assert_eq!(patterns, vec!["pkg1", "pkg2"]);
        }

        #[test]
        fn parse_empty_file() {
            let file = write_temp_file("");
            let patterns = parse_override_file(file.path()).unwrap();
            assert!(patterns.is_empty());
        }

        #[test]
        fn parse_comments_only() {
            let file = write_temp_file("# Only comments\n# Nothing else\n");
            let patterns = parse_override_file(file.path()).unwrap();
            assert!(patterns.is_empty());
        }

        #[test]
        fn parse_glob_patterns() {
            let file = write_temp_file("pkg-*\n*-git\nprefix-?-suffix\n");
            let patterns = parse_override_file(file.path()).unwrap();
            assert_eq!(patterns, vec!["pkg-*", "*-git", "prefix-?-suffix"]);
        }
    }

    mod trigger_override {
        use super::*;
        use std::io::Write;
        use tempfile::NamedTempFile;

        #[test]
        fn load_disabled() {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(b"").unwrap();
            let override_ = TriggerOverride::load(file.path()).unwrap();
            assert_eq!(override_, TriggerOverride::Disabled);
        }

        #[test]
        fn load_with_patterns() {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(b"pkg1\npkg2\n").unwrap();
            let override_ = TriggerOverride::load(file.path()).unwrap();
            assert_eq!(
                override_,
                TriggerOverride::Patterns(vec!["pkg1".into(), "pkg2".into()])
            );
        }
    }

    mod package_override {
        use super::*;
        use std::io::Write;
        use tempfile::NamedTempFile;

        #[test]
        fn load_never_mark() {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(b"").unwrap();
            let override_ = PackageOverride::load(file.path()).unwrap();
            assert_eq!(override_, PackageOverride::NeverMark);
        }

        #[test]
        fn load_with_triggers() {
            let mut file = NamedTempFile::new().unwrap();
            file.write_all(b"qt6-base\ngtk4\n").unwrap();
            let override_ = PackageOverride::load(file.path()).unwrap();
            assert_eq!(
                override_,
                PackageOverride::OnlyTriggers(vec!["qt6-base".into(), "gtk4".into()])
            );
        }
    }

    mod overrides_struct {
        use super::*;

        fn make_overrides() -> Overrides {
            let mut overrides = Overrides::default();

            // Add trigger overrides
            overrides.triggers.insert(
                "custom-lib".into(),
                TriggerOverride::Patterns(vec!["custom-app".into(), "custom-*".into()]),
            );
            overrides
                .triggers
                .insert("disabled-trigger".into(), TriggerOverride::Disabled);

            // Add package overrides
            overrides.packages.insert(
                "restricted-pkg".into(),
                PackageOverride::OnlyTriggers(vec!["qt6-base".into()]),
            );
            overrides
                .packages
                .insert("never-pkg".into(), PackageOverride::NeverMark);

            overrides
        }

        #[test]
        fn is_user_trigger() {
            let overrides = make_overrides();
            assert!(overrides.is_user_trigger("custom-lib"));
            assert!(overrides.is_user_trigger("disabled-trigger"));
            assert!(!overrides.is_user_trigger("qt6-base")); // curated, not user
            assert!(!overrides.is_user_trigger("unknown"));
        }

        #[test]
        fn get_trigger_targets_with_patterns() {
            let overrides = make_overrides();
            let aur_packages: HashSet<String> = [
                "custom-app",
                "custom-tool",
                "custom-bin", // -bin should be filtered
                "other-pkg",
            ]
            .into_iter()
            .map(String::from)
            .collect();

            let targets = overrides
                .get_trigger_targets("custom-lib", &aur_packages)
                .unwrap();
            assert!(targets.contains(&"custom-app".to_string()));
            assert!(targets.contains(&"custom-tool".to_string()));
            assert!(!targets.contains(&"custom-bin".to_string())); // -bin filtered
            assert!(!targets.contains(&"other-pkg".to_string()));
        }

        #[test]
        fn get_trigger_targets_disabled() {
            let overrides = make_overrides();
            let aur_packages: HashSet<String> =
                ["pkg1", "pkg2"].into_iter().map(String::from).collect();

            let targets = overrides
                .get_trigger_targets("disabled-trigger", &aur_packages)
                .unwrap();
            assert!(targets.is_empty());
        }

        #[test]
        fn get_trigger_targets_no_override() {
            let overrides = make_overrides();
            let aur_packages: HashSet<String> =
                ["pkg1", "pkg2"].into_iter().map(String::from).collect();

            // No override for qt6-base, should return None
            assert!(
                overrides
                    .get_trigger_targets("qt6-base", &aur_packages)
                    .is_none()
            );
        }

        #[test]
        fn should_mark_package_no_override() {
            let overrides = make_overrides();
            // No override, should allow marking
            assert!(overrides.should_mark_package("normal-pkg", "any-trigger"));
        }

        #[test]
        fn should_mark_package_never_mark() {
            let overrides = make_overrides();
            assert!(!overrides.should_mark_package("never-pkg", "qt6-base"));
            assert!(!overrides.should_mark_package("never-pkg", "any-trigger"));
        }

        #[test]
        fn should_mark_package_restricted() {
            let overrides = make_overrides();
            // restricted-pkg only allows qt6-base
            assert!(overrides.should_mark_package("restricted-pkg", "qt6-base"));
            assert!(!overrides.should_mark_package("restricted-pkg", "gtk4"));
            assert!(!overrides.should_mark_package("restricted-pkg", "other"));
        }
    }
}
