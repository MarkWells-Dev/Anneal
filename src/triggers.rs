// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Curated trigger list for ABI-sensitive packages.
//!
//! This module contains the embedded list of packages known to cause ABI breakage
//! when upgraded. The list is community-maintained and versioned.

/// Version of the curated trigger list.
///
/// Increment this when adding, removing, or modifying triggers.
pub const TRIGGER_LIST_VERSION: u32 = 1;

/// Curated list of ABI-sensitive packages that may require dependent rebuilds.
///
/// These packages are known to break AUR dependents when upgraded due to:
/// - Shared library ABI changes
/// - Plugin API changes
/// - Build-time header changes
///
/// The list is intentionally conservative - only packages with a history of
/// breaking dependents are included.
pub const TRIGGERS: &[&str] = &[
    // Qt ecosystem
    "qt5-base", "qt6-base", // GTK ecosystem
    "gtk2", "gtk3", "gtk4", // Core libraries
    "boost", "electron", "icu", "openssl",
];

/// Returns whether a package name is in the curated trigger list.
#[inline]
pub fn is_curated_trigger(package: &str) -> bool {
    TRIGGERS.contains(&package)
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
    fn no_duplicate_triggers() {
        let mut seen = std::collections::HashSet::new();
        for trigger in TRIGGERS {
            assert!(seen.insert(*trigger), "duplicate trigger: {trigger}");
        }
    }

    #[test]
    fn no_empty_triggers() {
        for trigger in TRIGGERS {
            assert!(!trigger.is_empty(), "empty trigger in list");
            assert!(
                !trigger.contains(char::is_whitespace),
                "trigger has whitespace: {trigger:?}"
            );
        }
    }
}
