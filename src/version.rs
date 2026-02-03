// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Version parsing and comparison for Arch Linux packages.
//!
//! Handles various version formats:
//! - Semver: `1.2.3`
//! - Two-part: `1.2`
//! - With epoch: `1:2.3.4`
//! - With pkgrel: `1.2.3-1`
//! - Pre-release: `1.2.3alpha`, `1.2.3-rc1`
//! - Date-based: `20240101`, `2024.01.01`

use std::cmp::Ordering;

/// Threshold for determining when a version change should trigger a rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Threshold {
    /// Trigger only on major version changes (1.x.x -> 2.x.x)
    Major,
    /// Trigger on major or minor version changes (1.1.x -> 1.2.x)
    Minor,
    /// Trigger on any version change including patch (1.1.1 -> 1.1.2)
    Patch,
    /// Always trigger on any change, regardless of version parsing
    Always,
}

/// A parsed version with optional epoch and pkgrel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Epoch (the `1:` in `1:2.3.4-5`), defaults to 0
    pub epoch: u32,
    /// Version segments (e.g., [1, 2, 3] for "1.2.3")
    pub segments: Vec<Segment>,
    /// Package release number (the `-5` in `1.2.3-5`)
    pub pkgrel: Option<String>,
}

/// A single segment of a version string.
///
/// Versions like "1.2.3alpha" become [Numeric(1), Numeric(2), Numeric(3), Alpha("alpha")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// A numeric segment (e.g., `1`, `23`, `2024`)
    Numeric(u64),
    /// An alphabetic segment (e.g., `alpha`, `rc`, `beta`)
    Alpha(String),
}

impl Segment {
    fn cmp_to(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Segment::Numeric(a), Segment::Numeric(b)) => a.cmp(b),
            (Segment::Alpha(a), Segment::Alpha(b)) => a.cmp(b),
            // Numeric segments sort after alphabetic (1.0 > 1.0rc1)
            (Segment::Numeric(_), Segment::Alpha(_)) => Ordering::Greater,
            (Segment::Alpha(_), Segment::Numeric(_)) => Ordering::Less,
        }
    }
}

impl Version {
    /// Parse a version string into a Version struct.
    ///
    /// Handles formats like:
    /// - "1.2.3"
    /// - "1:2.3.4" (with epoch)
    /// - "1.2.3-1" (with pkgrel)
    /// - "1:2.3.4-5" (with both)
    /// - "1.2.3alpha" or "1.2.3-rc1" (pre-release)
    /// - "20240101" (date-based)
    pub fn parse(input: &str) -> Option<Self> {
        if input.is_empty() {
            return None;
        }

        let mut remaining = input;

        // Extract epoch (e.g., "1:" prefix)
        let epoch = if let Some(idx) = remaining.find(':') {
            let epoch_str = &remaining[..idx];
            remaining = &remaining[idx + 1..];
            epoch_str.parse().ok()?
        } else {
            0
        };

        // Extract pkgrel (e.g., "-1" suffix, but not "-rc1" style pre-release)
        // pkgrel is only digits after the last hyphen
        let pkgrel = if let Some(idx) = remaining.rfind('-') {
            let potential_pkgrel = &remaining[idx + 1..];
            // Only treat as pkgrel if it's purely numeric (possibly with dots like "1.1")
            if !potential_pkgrel.is_empty()
                && potential_pkgrel
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.')
            {
                let pkgrel = potential_pkgrel.to_string();
                remaining = &remaining[..idx];
                Some(pkgrel)
            } else {
                None
            }
        } else {
            None
        };

        // Parse the main version segments
        let segments = Self::parse_segments(remaining)?;

        if segments.is_empty() {
            return None;
        }

        Some(Version {
            epoch,
            segments,
            pkgrel,
        })
    }

    /// Parse version segments from a string like "1.2.3" or "1.2.3alpha".
    fn parse_segments(input: &str) -> Option<Vec<Segment>> {
        let mut segments = Vec::new();

        // Split on common delimiters: '.', '_', '-'
        // But also handle runs of digits vs alpha within a segment
        for part in input.split(['.', '_', '-']) {
            if part.is_empty() {
                continue;
            }

            // Further split mixed segments like "3alpha" into [3, "alpha"]
            let mut current_num = String::new();
            let mut current_alpha = String::new();

            for ch in part.chars() {
                if ch.is_ascii_digit() {
                    // Flush any pending alpha segment
                    if !current_alpha.is_empty() {
                        segments.push(Segment::Alpha(current_alpha.clone()));
                        current_alpha.clear();
                    }
                    current_num.push(ch);
                } else {
                    // Flush any pending numeric segment
                    if !current_num.is_empty() {
                        if let Ok(n) = current_num.parse() {
                            segments.push(Segment::Numeric(n));
                        }
                        current_num.clear();
                    }
                    current_alpha.push(ch);
                }
            }

            // Flush remaining
            if !current_num.is_empty()
                && let Ok(n) = current_num.parse()
            {
                segments.push(Segment::Numeric(n));
            }
            if !current_alpha.is_empty() {
                segments.push(Segment::Alpha(current_alpha));
            }
        }

        Some(segments)
    }

    /// Get the major version (first numeric segment), if any.
    pub fn major(&self) -> Option<u64> {
        self.segments.iter().find_map(|s| match s {
            Segment::Numeric(n) => Some(*n),
            Segment::Alpha(_) => None,
        })
    }

    /// Get the minor version (second numeric segment), if any.
    pub fn minor(&self) -> Option<u64> {
        self.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Numeric(n) => Some(*n),
                Segment::Alpha(_) => None,
            })
            .nth(1)
    }

    /// Get the patch version (third numeric segment), if any.
    pub fn patch(&self) -> Option<u64> {
        self.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Numeric(n) => Some(*n),
                Segment::Alpha(_) => None,
            })
            .nth(2)
    }

    /// Compare two versions and return the ordering.
    pub fn cmp_to(&self, other: &Self) -> Ordering {
        // Epoch takes precedence
        match self.epoch.cmp(&other.epoch) {
            Ordering::Equal => {}
            ord => return ord,
        }

        // Compare segments
        let max_len = self.segments.len().max(other.segments.len());
        for i in 0..max_len {
            let a = self.segments.get(i);
            let b = other.segments.get(i);

            match (a, b) {
                (Some(seg_a), Some(seg_b)) => match seg_a.cmp_to(seg_b) {
                    Ordering::Equal => continue,
                    ord => return ord,
                },
                // When one version is shorter:
                // - Extra numeric segments = longer is greater (1.0 < 1.0.1)
                // - Extra alpha segments = shorter is greater (1.0.0 > 1.0.0rc1)
                (Some(seg), None) => {
                    return match seg {
                        Segment::Numeric(_) => Ordering::Greater,
                        Segment::Alpha(_) => Ordering::Less,
                    };
                }
                (None, Some(seg)) => {
                    return match seg {
                        Segment::Numeric(_) => Ordering::Less,
                        Segment::Alpha(_) => Ordering::Greater,
                    };
                }
                (None, None) => break,
            }
        }

        // Versions are equal (ignoring pkgrel for version comparison)
        Ordering::Equal
    }
}

/// Determine if a version change should trigger a rebuild based on the threshold.
///
/// Returns `true` if the change exceeds the threshold and should trigger a rebuild.
pub fn exceeds_threshold(old: &Version, new: &Version, threshold: Threshold) -> bool {
    match threshold {
        Threshold::Always => old != new || old.pkgrel != new.pkgrel,

        Threshold::Major => {
            // Epoch change always triggers
            if old.epoch != new.epoch {
                return true;
            }
            // Major version change
            match (old.major(), new.major()) {
                (Some(old_maj), Some(new_maj)) => old_maj != new_maj,
                // If we can't parse major, fall back to any difference
                _ => old.cmp_to(new) != Ordering::Equal,
            }
        }

        Threshold::Minor => {
            // Epoch change always triggers
            if old.epoch != new.epoch {
                return true;
            }
            // Major or minor version change
            match (old.major(), new.major()) {
                (Some(old_maj), Some(new_maj)) if old_maj != new_maj => return true,
                _ => {}
            }
            match (old.minor(), new.minor()) {
                (Some(old_min), Some(new_min)) => old_min != new_min,
                // If old has no minor but new does (1 -> 1.1), that's a change
                (None, Some(_)) => true,
                (Some(_), None) => true,
                // If neither has minor, check if versions differ at all
                (None, None) => old.major() != new.major(),
            }
        }

        Threshold::Patch => {
            // Any version change (ignoring pkgrel)
            old.epoch != new.epoch || old.cmp_to(new) != Ordering::Equal
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ==================== Parsing Tests ====================

    mod parsing {
        use super::*;

        #[test]
        fn simple_semver() {
            let v = Version::parse("1.2.3").unwrap();
            assert_eq!(v.epoch, 0);
            assert_eq!(v.major(), Some(1));
            assert_eq!(v.minor(), Some(2));
            assert_eq!(v.patch(), Some(3));
            assert_eq!(v.pkgrel, None);
        }

        #[test]
        fn two_part_version() {
            let v = Version::parse("1.2").unwrap();
            assert_eq!(v.major(), Some(1));
            assert_eq!(v.minor(), Some(2));
            assert_eq!(v.patch(), None);
        }

        #[test]
        fn single_number() {
            let v = Version::parse("42").unwrap();
            assert_eq!(v.major(), Some(42));
            assert_eq!(v.minor(), None);
        }

        #[test]
        fn with_epoch() {
            let v = Version::parse("1:2.3.4").unwrap();
            assert_eq!(v.epoch, 1);
            assert_eq!(v.major(), Some(2));
            assert_eq!(v.minor(), Some(3));
            assert_eq!(v.patch(), Some(4));
        }

        #[test]
        fn with_pkgrel() {
            let v = Version::parse("1.2.3-1").unwrap();
            assert_eq!(v.major(), Some(1));
            assert_eq!(v.pkgrel, Some("1".to_string()));
        }

        #[test]
        fn with_epoch_and_pkgrel() {
            let v = Version::parse("2:1.2.3-4").unwrap();
            assert_eq!(v.epoch, 2);
            assert_eq!(v.major(), Some(1));
            assert_eq!(v.minor(), Some(2));
            assert_eq!(v.patch(), Some(3));
            assert_eq!(v.pkgrel, Some("4".to_string()));
        }

        #[test]
        fn pkgrel_with_subrelease() {
            let v = Version::parse("1.2.3-1.1").unwrap();
            assert_eq!(v.pkgrel, Some("1.1".to_string()));
        }

        #[test]
        fn prerelease_alpha() {
            let v = Version::parse("1.2.3alpha").unwrap();
            assert_eq!(v.major(), Some(1));
            assert_eq!(v.minor(), Some(2));
            assert_eq!(v.patch(), Some(3));
            // "alpha" should be a separate segment
            assert!(v.segments.contains(&Segment::Alpha("alpha".to_string())));
        }

        #[test]
        fn prerelease_rc() {
            let v = Version::parse("1.2.3-rc1").unwrap();
            assert_eq!(v.major(), Some(1));
            assert!(v.segments.contains(&Segment::Alpha("rc".to_string())));
            // The "1" after rc should be parsed as numeric
            assert_eq!(v.pkgrel, None); // -rc1 is not a pkgrel
        }

        #[test]
        fn prerelease_beta_with_number() {
            let v = Version::parse("2.0beta3").unwrap();
            assert_eq!(v.major(), Some(2));
            assert_eq!(v.minor(), Some(0));
            assert!(v.segments.contains(&Segment::Alpha("beta".to_string())));
            assert!(v.segments.contains(&Segment::Numeric(3)));
        }

        #[test]
        fn date_based_compact() {
            let v = Version::parse("20240115").unwrap();
            assert_eq!(v.major(), Some(20240115));
        }

        #[test]
        fn date_based_dotted() {
            let v = Version::parse("2024.01.15").unwrap();
            assert_eq!(v.major(), Some(2024));
            assert_eq!(v.minor(), Some(1));
            assert_eq!(v.patch(), Some(15));
        }

        #[test]
        fn abseil_cpp_style() {
            // abseil-cpp uses YYYYMMDD.N format
            let v = Version::parse("20240116.2").unwrap();
            assert_eq!(v.major(), Some(20240116));
            assert_eq!(v.minor(), Some(2));
        }

        #[test]
        fn underscore_separator() {
            let v = Version::parse("1_2_3").unwrap();
            assert_eq!(v.major(), Some(1));
            assert_eq!(v.minor(), Some(2));
            assert_eq!(v.patch(), Some(3));
        }

        #[test]
        fn mixed_separators() {
            let v = Version::parse("1.2_3-4").unwrap();
            // Should parse as 1, 2, 3 (4 is pkgrel)
            assert_eq!(v.major(), Some(1));
            assert_eq!(v.minor(), Some(2));
            assert_eq!(v.patch(), Some(3));
            assert_eq!(v.pkgrel, Some("4".to_string()));
        }

        #[test]
        fn empty_string() {
            assert!(Version::parse("").is_none());
        }

        #[test]
        fn only_epoch() {
            // "1:" with nothing after is invalid
            assert!(Version::parse("1:").is_none());
        }

        #[test]
        fn large_version_numbers() {
            let v = Version::parse("2024.12.31").unwrap();
            assert_eq!(v.major(), Some(2024));
        }

        #[test]
        fn qt_style_version() {
            let v = Version::parse("6.7.2").unwrap();
            assert_eq!(v.major(), Some(6));
            assert_eq!(v.minor(), Some(7));
            assert_eq!(v.patch(), Some(2));
        }

        #[test]
        fn electron_style() {
            let v = Version::parse("31.0.0").unwrap();
            assert_eq!(v.major(), Some(31));
            assert_eq!(v.minor(), Some(0));
            assert_eq!(v.patch(), Some(0));
        }

        #[test]
        fn boost_style() {
            let v = Version::parse("1.85.0").unwrap();
            assert_eq!(v.major(), Some(1));
            assert_eq!(v.minor(), Some(85));
            assert_eq!(v.patch(), Some(0));
        }
    }

    // ==================== Comparison Tests ====================

    mod comparison {
        use super::*;

        fn v(s: &str) -> Version {
            Version::parse(s).unwrap()
        }

        #[test]
        fn equal_versions() {
            assert_eq!(v("1.2.3").cmp_to(&v("1.2.3")), Ordering::Equal);
        }

        #[test]
        fn major_difference() {
            assert_eq!(v("2.0.0").cmp_to(&v("1.0.0")), Ordering::Greater);
            assert_eq!(v("1.0.0").cmp_to(&v("2.0.0")), Ordering::Less);
        }

        #[test]
        fn minor_difference() {
            assert_eq!(v("1.2.0").cmp_to(&v("1.1.0")), Ordering::Greater);
            assert_eq!(v("1.1.0").cmp_to(&v("1.2.0")), Ordering::Less);
        }

        #[test]
        fn patch_difference() {
            assert_eq!(v("1.2.3").cmp_to(&v("1.2.2")), Ordering::Greater);
            assert_eq!(v("1.2.2").cmp_to(&v("1.2.3")), Ordering::Less);
        }

        #[test]
        fn epoch_takes_precedence() {
            // 1:1.0.0 > 0:2.0.0
            assert_eq!(v("1:1.0.0").cmp_to(&v("2.0.0")), Ordering::Greater);
            assert_eq!(v("2.0.0").cmp_to(&v("1:1.0.0")), Ordering::Less);
        }

        #[test]
        fn different_segment_counts() {
            // 1.2.3 > 1.2
            assert_eq!(v("1.2.3").cmp_to(&v("1.2")), Ordering::Greater);
            // 1.2 < 1.2.1
            assert_eq!(v("1.2").cmp_to(&v("1.2.1")), Ordering::Less);
        }

        #[test]
        fn prerelease_less_than_release() {
            // 1.0.0 > 1.0.0rc1 (numeric > alpha)
            assert_eq!(v("1.0.0").cmp_to(&v("1.0.0rc1")), Ordering::Greater);
        }

        #[test]
        fn prerelease_ordering() {
            // alpha < beta < rc
            assert_eq!(v("1.0alpha").cmp_to(&v("1.0beta")), Ordering::Less);
            assert_eq!(v("1.0beta").cmp_to(&v("1.0rc")), Ordering::Less);
        }

        #[test]
        fn date_based_comparison() {
            assert_eq!(v("20240201").cmp_to(&v("20240115")), Ordering::Greater);
            assert_eq!(v("20240115").cmp_to(&v("20240201")), Ordering::Less);
        }

        #[test]
        fn pkgrel_ignored_in_version_comparison() {
            // Version comparison ignores pkgrel
            assert_eq!(v("1.2.3-1").cmp_to(&v("1.2.3-2")), Ordering::Equal);
        }
    }

    // ==================== Threshold Tests ====================

    mod threshold {
        use super::*;

        fn v(s: &str) -> Version {
            Version::parse(s).unwrap()
        }

        // --- Major threshold ---

        #[test]
        fn major_triggers_on_major_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("2.0.0"),
                Threshold::Major
            ));
        }

        #[test]
        fn major_ignores_minor_change() {
            assert!(!exceeds_threshold(
                &v("1.0.0"),
                &v("1.1.0"),
                Threshold::Major
            ));
        }

        #[test]
        fn major_ignores_patch_change() {
            assert!(!exceeds_threshold(
                &v("1.0.0"),
                &v("1.0.1"),
                Threshold::Major
            ));
        }

        #[test]
        fn major_triggers_on_epoch_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("1:1.0.0"),
                Threshold::Major
            ));
        }

        // --- Minor threshold ---

        #[test]
        fn minor_triggers_on_major_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("2.0.0"),
                Threshold::Minor
            ));
        }

        #[test]
        fn minor_triggers_on_minor_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("1.1.0"),
                Threshold::Minor
            ));
        }

        #[test]
        fn minor_ignores_patch_change() {
            assert!(!exceeds_threshold(
                &v("1.0.0"),
                &v("1.0.1"),
                Threshold::Minor
            ));
        }

        #[test]
        fn minor_triggers_on_epoch_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("1:1.0.0"),
                Threshold::Minor
            ));
        }

        #[test]
        fn minor_triggers_when_minor_added() {
            // 1 -> 1.1 should trigger
            assert!(exceeds_threshold(&v("1"), &v("1.1"), Threshold::Minor));
        }

        // --- Patch threshold ---

        #[test]
        fn patch_triggers_on_major_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("2.0.0"),
                Threshold::Patch
            ));
        }

        #[test]
        fn patch_triggers_on_minor_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("1.1.0"),
                Threshold::Patch
            ));
        }

        #[test]
        fn patch_triggers_on_patch_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("1.0.1"),
                Threshold::Patch
            ));
        }

        #[test]
        fn patch_ignores_pkgrel_change() {
            assert!(!exceeds_threshold(
                &v("1.0.0-1"),
                &v("1.0.0-2"),
                Threshold::Patch
            ));
        }

        // --- Always threshold ---

        #[test]
        fn always_triggers_on_any_change() {
            assert!(exceeds_threshold(
                &v("1.0.0"),
                &v("1.0.0a"),
                Threshold::Always
            ));
        }

        #[test]
        fn always_triggers_on_pkgrel_change() {
            assert!(exceeds_threshold(
                &v("1.0.0-1"),
                &v("1.0.0-2"),
                Threshold::Always
            ));
        }

        #[test]
        fn always_no_trigger_when_identical() {
            assert!(!exceeds_threshold(
                &v("1.0.0-1"),
                &v("1.0.0-1"),
                Threshold::Always
            ));
        }

        // --- Real-world scenarios ---

        #[test]
        fn qt6_minor_bump() {
            // Qt 6.7.2 -> 6.8.0 should trigger on minor
            assert!(exceeds_threshold(
                &v("6.7.2"),
                &v("6.8.0"),
                Threshold::Minor
            ));
            // Should not trigger on major
            assert!(!exceeds_threshold(
                &v("6.7.2"),
                &v("6.8.0"),
                Threshold::Major
            ));
        }

        #[test]
        fn boost_minor_bump() {
            // Boost 1.85.0 -> 1.86.0
            assert!(exceeds_threshold(
                &v("1.85.0"),
                &v("1.86.0"),
                Threshold::Minor
            ));
        }

        #[test]
        fn electron_major_bump() {
            // Electron 30.0.0 -> 31.0.0
            assert!(exceeds_threshold(
                &v("30.0.0"),
                &v("31.0.0"),
                Threshold::Major
            ));
        }

        #[test]
        fn python_minor_bump() {
            // Python 3.11.9 -> 3.12.0
            assert!(exceeds_threshold(
                &v("3.11.9"),
                &v("3.12.0"),
                Threshold::Minor
            ));
        }

        #[test]
        fn abseil_date_change() {
            // abseil-cpp date-based version
            assert!(exceeds_threshold(
                &v("20240116.2"),
                &v("20240722.0"),
                Threshold::Always
            ));
        }

        #[test]
        fn protobuf_patch_sensitivity() {
            // protobuf is patch-sensitive
            assert!(exceeds_threshold(
                &v("27.0.0"),
                &v("27.0.1"),
                Threshold::Patch
            ));
            assert!(!exceeds_threshold(
                &v("27.0.0"),
                &v("27.0.1"),
                Threshold::Minor
            ));
        }

        #[test]
        fn glibc_major_stability() {
            // glibc 2.39 -> 2.40 should not trigger major (still 2.x)
            assert!(!exceeds_threshold(&v("2.39"), &v("2.40"), Threshold::Major));
            // But should trigger minor
            assert!(exceeds_threshold(&v("2.39"), &v("2.40"), Threshold::Minor));
        }
    }
}
