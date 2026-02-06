// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Trigger processing logic.
//!
//! Handles the core automation: when a trigger package upgrades,
//! find and mark its AUR dependents for rebuild.
//!
//! ## Version Threshold Checking
//!
//! Packages can be specified with version info: `name:oldver:newver`
//! When version info is provided, the threshold is checked before triggering.
//! Without version info, triggers always fire.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use crate::overrides::Overrides;
use crate::triggers::{TRIGGERS, get_curated_threshold, is_curated_trigger};
use crate::version::{Threshold, Version, exceeds_threshold};

/// Parsed trigger input with optional version info.
///
/// Input format: `name` or `name:oldver:newver`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerInput {
    /// Package name.
    pub name: String,
    /// Old version (before upgrade).
    pub old_version: Option<String>,
    /// New version (after upgrade).
    pub new_version: Option<String>,
}

impl TriggerInput {
    /// Parse a trigger input string.
    ///
    /// Accepts formats:
    /// - `name` - package name only, no version checking
    /// - `name:oldver:newver` - with version info for threshold checking
    pub fn parse(input: &str) -> Self {
        let parts: Vec<&str> = input.splitn(3, ':').collect();
        match parts.as_slice() {
            [name, old, new] => Self {
                name: (*name).to_string(),
                old_version: Some((*old).to_string()),
                new_version: Some((*new).to_string()),
            },
            _ => Self {
                name: input.to_string(),
                old_version: None,
                new_version: None,
            },
        }
    }

    /// Check if this trigger should fire based on version threshold.
    ///
    /// Returns true if:
    /// - No version info provided (always fires)
    /// - Version info provided and exceeds threshold
    /// - Version parsing fails (conservative: always fires)
    pub fn exceeds_threshold(&self, threshold: Threshold) -> bool {
        let (Some(old), Some(new)) = (&self.old_version, &self.new_version) else {
            // No version info, always trigger
            return true;
        };

        let (Some(old_ver), Some(new_ver)) = (Version::parse(old), Version::parse(new)) else {
            // Version parsing failed, be conservative and trigger
            return true;
        };

        exceeds_threshold(&old_ver, &new_ver, threshold)
    }
}

/// Result of processing triggers.
#[derive(Debug, Default)]
pub struct TriggerResult {
    /// Packages that were marked (or would be marked in dry-run).
    pub marked: Vec<MarkedPackage>,
    /// Triggers that were skipped (not in curated list, no override).
    pub skipped: Vec<String>,
    /// Triggers that were skipped due to version threshold.
    pub below_threshold: Vec<String>,
}

/// A package that was marked by a trigger.
#[derive(Debug, Clone)]
pub struct MarkedPackage {
    /// The package name.
    pub package: String,
    /// The trigger that caused the mark.
    pub trigger: String,
}

/// Errors that can occur during trigger processing.
#[derive(Debug)]
pub enum TriggerError {
    /// Failed to run pactree.
    Pactree(std::io::Error),
    /// Failed to run pacman.
    Pacman(std::io::Error),
    /// pactree returned non-zero exit code.
    PactreeExitCode(i32),
    /// pacman returned non-zero exit code.
    PacmanExitCode(i32),
}

impl std::fmt::Display for TriggerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pactree(e) => write!(f, "failed to run pactree: {e}"),
            Self::Pacman(e) => write!(f, "failed to run pacman: {e}"),
            Self::PactreeExitCode(code) => write!(f, "pactree exited with code {code}"),
            Self::PacmanExitCode(code) => write!(f, "pacman exited with code {code}"),
        }
    }
}

impl std::error::Error for TriggerError {}

/// Process a list of upgraded packages and find AUR dependents to mark.
///
/// For each package that's a known trigger:
/// 1. Check version threshold (if version info provided)
/// 2. Query reverse dependencies via pactree (or use override patterns)
/// 3. Filter to AUR packages only
/// 4. Filter out -bin packages
/// 5. Apply package overrides
/// 6. Return the list of packages to mark
///
/// Package format: `name` or `name:oldver:newver`
///
/// # Errors
///
/// Returns an error if pactree or pacman commands fail.
pub fn process_triggers(
    packages: &[String],
    default_threshold: Threshold,
    overrides: &Overrides,
) -> Result<TriggerResult, TriggerError> {
    let mut result = TriggerResult::default();

    // Get list of AUR packages once (expensive operation)
    let aur_packages = get_aur_packages()?;

    for pkg_input in packages {
        let input = TriggerInput::parse(pkg_input);

        if !is_trigger(&input.name, overrides) {
            result.skipped.push(input.name);
            continue;
        }

        // Use per-trigger threshold for curated triggers, global config for user-defined
        let threshold = get_curated_threshold(&input.name).unwrap_or(default_threshold);

        // Check version threshold
        if !input.exceeds_threshold(threshold) {
            result.below_threshold.push(input.name);
            continue;
        }

        let dependents = get_aur_dependents(&input.name, &aur_packages, overrides)?;
        for dep in dependents {
            result.marked.push(MarkedPackage {
                package: dep,
                trigger: input.name.clone(),
            });
        }
    }

    // Deduplicate - a package might be marked by multiple triggers
    deduplicate_marked(&mut result.marked);

    Ok(result)
}

/// Check if a package is a known trigger.
///
/// A package is a trigger if it's in the curated list OR has a user override file.
fn is_trigger(package: &str, overrides: &Overrides) -> bool {
    is_curated_trigger(package) || overrides.is_user_trigger(package)
}

/// Get reverse dependencies of a package that are AUR packages.
fn get_aur_dependents(
    package: &str,
    aur_packages: &HashSet<String>,
    overrides: &Overrides,
) -> Result<Vec<String>, TriggerError> {
    // Check for trigger override first
    if let Some(targets) = overrides.get_trigger_targets(package, aur_packages) {
        // Override handles -bin filtering internally
        // Apply package overrides to the results
        let filtered: Vec<String> = targets
            .into_iter()
            .filter(|dep| overrides.should_mark_package(dep, package))
            .collect();
        return Ok(filtered);
    }

    // Default: pactree lookup
    let reverse_deps = get_reverse_deps(package)?;

    let dependents: Vec<String> = reverse_deps
        .into_iter()
        .filter(|dep| {
            // Must be an AUR package
            aur_packages.contains(dep)
            // Filter out -bin packages (rebuilding just re-downloads the same binary)
            && !dep.ends_with("-bin")
            // Check package override
            && overrides.should_mark_package(dep, package)
        })
        .collect();

    Ok(dependents)
}

/// Get reverse dependencies of a package using pactree.
fn get_reverse_deps(package: &str) -> Result<Vec<String>, TriggerError> {
    let output = Command::new("pactree")
        .args(["-r", "-u", package])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(TriggerError::Pactree)?;

    if !output.status.success() {
        // pactree returns 1 if package not found, which is fine
        // (package might have been removed or not installed)
        return Ok(Vec::new());
    }

    let deps: Vec<String> = BufReader::new(&output.stdout[..])
        .lines()
        .map_while(Result::ok)
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty() && line != package)
        .collect();

    Ok(deps)
}

/// Get list of AUR (foreign) packages.
fn get_aur_packages() -> Result<HashSet<String>, TriggerError> {
    let output = Command::new("pacman")
        .args(["-Qmq"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(TriggerError::Pacman)?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        // Exit code 1 with no output means no foreign packages
        if code == 1 && output.stdout.is_empty() {
            return Ok(HashSet::new());
        }
        return Err(TriggerError::PacmanExitCode(code));
    }

    let packages: HashSet<String> = BufReader::new(&output.stdout[..])
        .lines()
        .map_while(Result::ok)
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    Ok(packages)
}

/// Deduplicate marked packages, keeping the first trigger for each package.
fn deduplicate_marked(marked: &mut Vec<MarkedPackage>) {
    let mut seen = HashSet::new();
    marked.retain(|m| seen.insert(m.package.clone()));
}

/// Get list of all known triggers (curated + user overrides) with thresholds.
pub fn list_all_triggers(
    overrides: &Overrides,
    default_threshold: Threshold,
) -> Vec<(String, Threshold)> {
    let mut triggers: Vec<(String, Threshold)> = TRIGGERS
        .iter()
        .map(|(name, threshold)| ((*name).to_string(), *threshold))
        .collect();

    // Add user-defined triggers with the global default threshold
    for trigger in overrides.user_triggers() {
        if !triggers.iter().any(|(name, _)| name == trigger) {
            triggers.push((trigger.to_string(), default_threshold));
        }
    }

    triggers.sort_by(|(a, _), (b, _)| a.cmp(b));
    triggers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_trigger_curated() {
        let overrides = Overrides::default();
        assert!(is_trigger("qt6-base", &overrides));
        assert!(is_trigger("gtk4", &overrides));
        assert!(!is_trigger("not-a-trigger", &overrides));
    }

    #[test]
    fn deduplicate_keeps_first() {
        let mut marked = vec![
            MarkedPackage {
                package: "pkg1".into(),
                trigger: "trigger1".into(),
            },
            MarkedPackage {
                package: "pkg1".into(),
                trigger: "trigger2".into(),
            },
            MarkedPackage {
                package: "pkg2".into(),
                trigger: "trigger1".into(),
            },
        ];

        deduplicate_marked(&mut marked);

        assert_eq!(marked.len(), 2);
        assert_eq!(marked[0].package, "pkg1");
        assert_eq!(marked[0].trigger, "trigger1"); // First one kept
        assert_eq!(marked[1].package, "pkg2");
    }

    #[test]
    fn bin_suffix_detection() {
        assert!("foo-bin".ends_with("-bin"));
        assert!(!"binary".ends_with("-bin"));
        assert!(!"bin-foo".ends_with("-bin"));
    }

    mod trigger_input {
        use super::*;

        #[test]
        fn parse_name_only() {
            let input = TriggerInput::parse("qt6-base");
            assert_eq!(input.name, "qt6-base");
            assert_eq!(input.old_version, None);
            assert_eq!(input.new_version, None);
        }

        #[test]
        fn parse_with_versions() {
            let input = TriggerInput::parse("qt6-base:6.6.0-1:6.7.0-1");
            assert_eq!(input.name, "qt6-base");
            assert_eq!(input.old_version, Some("6.6.0-1".to_string()));
            assert_eq!(input.new_version, Some("6.7.0-1".to_string()));
        }

        #[test]
        fn parse_version_with_colons() {
            // Edge case: version contains colons (epoch)
            let input = TriggerInput::parse("pkg:1:2.0.0-1:1:3.0.0-1");
            assert_eq!(input.name, "pkg");
            // First split gives us name, then rest is treated as old:new
            assert_eq!(input.old_version, Some("1".to_string()));
            assert_eq!(input.new_version, Some("2.0.0-1:1:3.0.0-1".to_string()));
        }

        #[test]
        fn exceeds_threshold_no_versions() {
            let input = TriggerInput::parse("qt6-base");
            // No versions = always trigger
            assert!(input.exceeds_threshold(Threshold::Major));
            assert!(input.exceeds_threshold(Threshold::Minor));
            assert!(input.exceeds_threshold(Threshold::Patch));
        }

        #[test]
        fn exceeds_threshold_major_change() {
            let input = TriggerInput::parse("qt6-base:5.0.0:6.0.0");
            assert!(input.exceeds_threshold(Threshold::Major));
            assert!(input.exceeds_threshold(Threshold::Minor));
            assert!(input.exceeds_threshold(Threshold::Patch));
        }

        #[test]
        fn exceeds_threshold_minor_change() {
            let input = TriggerInput::parse("qt6-base:6.6.0:6.7.0");
            assert!(!input.exceeds_threshold(Threshold::Major));
            assert!(input.exceeds_threshold(Threshold::Minor));
            assert!(input.exceeds_threshold(Threshold::Patch));
        }

        #[test]
        fn exceeds_threshold_patch_change() {
            let input = TriggerInput::parse("qt6-base:6.7.0:6.7.1");
            assert!(!input.exceeds_threshold(Threshold::Major));
            assert!(!input.exceeds_threshold(Threshold::Minor));
            assert!(input.exceeds_threshold(Threshold::Patch));
        }

        #[test]
        fn exceeds_threshold_unparseable_versions() {
            // Unparseable versions should trigger (conservative)
            let input = TriggerInput {
                name: "pkg".into(),
                old_version: Some("".into()),
                new_version: Some("".into()),
            };
            assert!(input.exceeds_threshold(Threshold::Major));
        }
    }
}
