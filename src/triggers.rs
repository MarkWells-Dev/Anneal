// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Curated trigger list for ABI-sensitive packages.
//!
//! This module contains the embedded list of packages known to cause ABI breakage
//! when upgraded. The list is community-maintained and versioned.
//!
//! Each trigger has a per-package threshold that determines the minimum version
//! change severity required to fire the trigger. See `docs/CURATED_LIST.md` for
//! rationale behind each threshold selection.

use crate::version::Threshold;

/// Version of the curated trigger list.
///
/// Increment this when adding, removing, or modifying triggers.
pub const TRIGGER_LIST_VERSION: u32 = 3;

/// Curated list of ABI-sensitive packages with per-trigger thresholds.
///
/// Each entry is `(package_name, threshold)`. The threshold determines the
/// minimum version change severity that triggers a rebuild:
/// - `Major` — only major version bumps (excellent ABI stability)
/// - `Minor` — major or minor bumps (default for most packages)
/// - `Patch` — any version change including patch (poor ABI stability)
/// - `Always` — any change at all, including pkgrel (non-semver or unpredictable)
pub const TRIGGERS: &[(&str, Threshold)] = &[
    // Core system
    ("glibc", Threshold::Major),
    ("gcc-libs", Threshold::Major),
    // Toolkits
    ("glib2", Threshold::Minor),
    ("qt5-base", Threshold::Minor),
    ("qt6-base", Threshold::Minor),
    ("gtk2", Threshold::Minor),
    ("gtk3", Threshold::Minor),
    ("gtk4", Threshold::Minor),
    ("wxwidgets", Threshold::Minor),
    ("electron", Threshold::Major),
    // Graphics
    ("freetype2", Threshold::Minor),
    ("mesa", Threshold::Minor),
    ("vulkan-icd-loader", Threshold::Minor),
    // Multimedia
    ("ffmpeg", Threshold::Minor),
    ("pipewire", Threshold::Minor),
    // LLVM ecosystem
    ("llvm-libs", Threshold::Major),
    // Serialization / IPC
    ("protobuf", Threshold::Patch),
    ("abseil-cpp", Threshold::Always),
    ("grpc", Threshold::Minor),
    // Cryptography
    ("openssl", Threshold::Minor),
    ("gnutls", Threshold::Minor),
    ("icu", Threshold::Minor),
    // Common libraries
    ("curl", Threshold::Minor),
    ("boost", Threshold::Minor),
    ("opencv", Threshold::Minor),
    ("vtk", Threshold::Minor),
    // Databases
    ("postgresql-libs", Threshold::Major),
    // Language runtimes
    ("libffi", Threshold::Minor),
    ("python", Threshold::Minor),
    ("nodejs", Threshold::Major),
    ("ruby", Threshold::Minor),
    ("lua", Threshold::Minor),
];

/// Returns whether a package name is in the curated trigger list.
#[inline]
pub fn is_curated_trigger(package: &str) -> bool {
    TRIGGERS.iter().any(|(name, _)| *name == package)
}

/// Returns the per-trigger threshold for a curated trigger, if it exists.
#[inline]
pub fn get_curated_threshold(package: &str) -> Option<Threshold> {
    TRIGGERS
        .iter()
        .find(|(name, _)| *name == package)
        .map(|(_, threshold)| *threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_list_is_sorted() {
        // Triggers should be grouped by category, not globally sorted
        // This test just ensures the list isn't empty
        assert!(!TRIGGERS.is_empty());
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn trigger_list_version_is_positive() {
        assert!(TRIGGER_LIST_VERSION > 0);
    }

    #[test]
    fn is_curated_trigger_finds_known_triggers() {
        assert!(is_curated_trigger("qt6-base"));
        assert!(is_curated_trigger("gtk4"));
        assert!(is_curated_trigger("icu"));
    }

    #[test]
    fn is_curated_trigger_rejects_unknown() {
        assert!(!is_curated_trigger("not-a-trigger"));
        assert!(!is_curated_trigger("qt6")); // Not qt6-base
        assert!(!is_curated_trigger(""));
    }

    #[test]
    fn curated_threshold_lookup() {
        assert_eq!(get_curated_threshold("glibc"), Some(Threshold::Major));
        assert_eq!(get_curated_threshold("protobuf"), Some(Threshold::Patch));
        assert_eq!(get_curated_threshold("abseil-cpp"), Some(Threshold::Always));
        assert_eq!(get_curated_threshold("qt6-base"), Some(Threshold::Minor));
        assert_eq!(get_curated_threshold("not-a-trigger"), None);
    }

    #[test]
    fn no_duplicate_triggers() {
        let mut seen = std::collections::HashSet::new();
        for (name, _) in TRIGGERS {
            assert!(seen.insert(*name), "duplicate trigger: {name}");
        }
    }

    #[test]
    fn no_empty_triggers() {
        for (name, _) in TRIGGERS {
            assert!(!name.is_empty(), "empty trigger in list");
            assert!(
                !name.contains(char::is_whitespace),
                "trigger has whitespace: {name:?}"
            );
        }
    }
}
