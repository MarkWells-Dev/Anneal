// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Integration tests for the anneal CLI.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::process::Command;

fn anneal() -> Command {
    Command::new(env!("CARGO_BIN_EXE_anneal"))
}

mod help {
    use super::*;

    #[test]
    fn help_flag() {
        let output = anneal().arg("--help").output().expect("failed to run");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Proactive AUR rebuild management"));
        assert!(stdout.contains("Commands:"));
    }

    #[test]
    fn version_flag() {
        let output = anneal().arg("--version").output().expect("failed to run");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("anneal"));
    }

    #[test]
    fn subcommand_help() {
        let output = anneal()
            .args(["mark", "--help"])
            .output()
            .expect("failed to run");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Add packages to the rebuild queue"));
    }
}

mod triggers {
    use super::*;

    #[test]
    fn list_triggers() {
        let output = anneal().arg("triggers").output().expect("failed to run");
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("qt6-base"));
        assert!(stdout.contains("gtk4"));
        assert!(stdout.contains("boost"));
    }

    #[test]
    fn list_triggers_quiet() {
        let output = anneal()
            .args(["--quiet", "triggers"])
            .output()
            .expect("failed to run");
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Quiet mode should still output the trigger names
        assert!(stdout.contains("qt6-base"));
        // But not the header
        assert!(!stdout.contains("Curated triggers"));
    }
}

mod config {
    use super::*;

    #[test]
    fn dump_config() {
        let output = anneal().arg("config").output().expect("failed to run");
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("version_threshold"));
        assert!(stdout.contains("retention_days"));
    }
}

mod root_required {
    use super::*;

    #[test]
    fn mark_requires_root() {
        // Skip if running as root
        if unsafe { libc::getuid() } == 0 {
            return;
        }

        let output = anneal()
            .args(["mark", "test-pkg"])
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Permission denied"));
        assert!(stderr.contains("requires root"));
    }

    #[test]
    fn unmark_requires_root() {
        if unsafe { libc::getuid() } == 0 {
            return;
        }

        let output = anneal()
            .args(["unmark", "test-pkg"])
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Permission denied"));
    }

    #[test]
    fn clear_requires_root() {
        if unsafe { libc::getuid() } == 0 {
            return;
        }

        let output = anneal()
            .args(["clear", "-f"])
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Permission denied"));
    }

    #[test]
    fn trigger_requires_root() {
        if unsafe { libc::getuid() } == 0 {
            return;
        }

        let output = anneal()
            .args(["trigger", "qt6-base"])
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Permission denied"));
    }
}

mod readonly_commands {
    use super::*;

    #[test]
    fn list_without_database() {
        // When no database exists, list should give a helpful error
        let output = anneal().arg("list").output().expect("failed to run");

        // Either succeeds with empty queue or fails with no database error
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        if !output.status.success() {
            assert!(
                stderr.contains("No database found") || stderr.contains("unable to open"),
                "unexpected error: {stderr}"
            );
        } else {
            assert!(
                stdout.contains("No packages in queue") || stdout.is_empty(),
                "unexpected output: {stdout}"
            );
        }
    }

    #[test]
    fn ismarked_without_database() {
        let output = anneal()
            .args(["ismarked", "test-pkg"])
            .output()
            .expect("failed to run");

        // Should fail - either no database or package not found
        // Exit code 1 = error, Exit code 2 = not found
        assert!(
            output.status.code() == Some(1) || output.status.code() == Some(2),
            "expected exit code 1 or 2, got {:?}",
            output.status.code()
        );
    }

    #[test]
    fn query_without_database() {
        let output = anneal()
            .args(["query", "test-pkg"])
            .output()
            .expect("failed to run");

        // Should either succeed with empty output or fail with no database
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            assert!(
                stderr.contains("No database found") || stderr.contains("unable to open"),
                "unexpected error: {stderr}"
            );
        }
    }
}

mod quiet_mode {
    use super::*;

    #[test]
    fn quiet_with_clear_no_force_fails() {
        // Skip if running as root (would try to actually clear)
        if unsafe { libc::getuid() } == 0 {
            return;
        }

        // This should fail before root check because of quiet+confirmation conflict
        // Actually, root check happens first, so this will fail with permission denied
        let output = anneal()
            .args(["--quiet", "clear"])
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Will hit root check first
        assert!(
            stderr.contains("Permission denied")
                || stderr.contains("Cannot prompt for confirmation"),
            "unexpected error: {stderr}"
        );
    }
}

mod cli_parsing {
    use super::*;

    #[test]
    fn unknown_command_fails() {
        let output = anneal()
            .arg("unknown-command")
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("error:"));
    }

    #[test]
    fn mark_requires_packages() {
        let output = anneal().arg("mark").output().expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("required"));
    }

    #[test]
    fn query_requires_packages() {
        let output = anneal().arg("query").output().expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("required"));
    }

    #[test]
    fn ismarked_requires_package() {
        let output = anneal().arg("ismarked").output().expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("required"));
    }

    #[test]
    fn trigger_version_requires_trigger() {
        let output = anneal()
            .args(["mark", "pkg", "--trigger-version", "1.0"])
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--trigger"));
    }
}

mod rebuild_command {
    use super::*;

    #[test]
    fn rebuild_help() {
        let output = anneal()
            .args(["rebuild", "--help"])
            .output()
            .expect("failed to run");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Rebuild queued packages"));
        assert!(stdout.contains("--checkrebuild"));
        assert!(stdout.contains("--cmd"));
        assert!(stdout.contains("--force"));
    }

    #[test]
    fn rebuild_without_database() {
        let output = anneal().arg("rebuild").output().expect("failed to run");

        // Should fail with no database error
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            assert!(
                stderr.contains("No database found")
                    || stderr.contains("unable to open")
                    || stderr.contains("No AUR helper"),
                "unexpected error: {stderr}"
            );
        }
    }

    #[test]
    fn rebuild_does_not_require_root() {
        // rebuild command should NOT require root (AUR helpers don't need root)
        // It will fail for other reasons (no helper, no db) but not permission denied
        if unsafe { libc::getuid() } == 0 {
            return;
        }

        let output = anneal()
            .args(["rebuild", "-f"])
            .output()
            .expect("failed to run");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("Permission denied"),
            "rebuild should not require root: {stderr}"
        );
    }

    #[test]
    fn rebuild_quiet_without_force_fails() {
        // --quiet without -f should fail since we can't prompt
        let output = anneal()
            .args(["--quiet", "rebuild"])
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Cannot prompt for confirmation")
                || stderr.contains("No database")
                || stderr.contains("No AUR helper"),
            "unexpected error: {stderr}"
        );
    }

    #[test]
    fn rebuild_quiet_with_force_ok() {
        // --quiet with -f should not fail due to confirmation conflict
        let output = anneal()
            .args(["--quiet", "rebuild", "-f"])
            .output()
            .expect("failed to run");

        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should NOT fail due to confirmation conflict
        assert!(
            !stderr.contains("Cannot prompt"),
            "quiet+force should work: {stderr}"
        );
    }

    #[test]
    fn rebuild_nonexistent_helper() {
        // Using a non-existent helper should fail gracefully
        let output = anneal()
            .args(["rebuild", "-f", "--cmd", "nonexistent-helper-xyz"])
            .output()
            .expect("failed to run");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("not found") || stderr.contains("No database"),
            "expected helper not found error: {stderr}"
        );
    }
}

mod trigger_command {
    use super::*;

    fn has_pactree() -> bool {
        Command::new("pactree").arg("--version").output().is_ok()
    }

    fn has_pacman() -> bool {
        Command::new("pacman").arg("--version").output().is_ok()
    }

    #[test]
    fn trigger_dry_run_non_trigger() {
        // Skip if not on Arch Linux
        if !has_pactree() || !has_pacman() {
            return;
        }

        // A package that's not in the trigger list should be skipped
        let output = anneal()
            .args(["trigger", "--dry-run", "not-a-trigger-package"])
            .output()
            .expect("failed to run");

        // Should succeed but mark nothing
        assert!(output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("No packages to mark") || stderr.is_empty(),
            "unexpected stderr: {stderr}"
        );
    }

    #[test]
    fn trigger_dry_run_known_trigger() {
        // Skip if not on Arch Linux
        if !has_pactree() || !has_pacman() {
            return;
        }

        // qt6-base is a known trigger, dry-run should work
        let output = anneal()
            .args(["trigger", "--dry-run", "qt6-base"])
            .output()
            .expect("failed to run");

        // Should succeed (may or may not have packages to mark depending on system)
        assert!(output.status.success());
    }

    #[test]
    fn trigger_from_stdin_dry_run() {
        // Skip if not on Arch Linux
        if !has_pactree() || !has_pacman() {
            return;
        }

        use std::io::Write;
        use std::process::Stdio;

        let mut child = Command::new(env!("CARGO_BIN_EXE_anneal"))
            .args(["trigger", "--dry-run"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn");

        // Write packages to stdin
        {
            let stdin = child.stdin.as_mut().expect("failed to get stdin");
            writeln!(stdin, "qt6-base").expect("failed to write");
            writeln!(stdin, "not-a-trigger").expect("failed to write");
        }

        let output = child.wait_with_output().expect("failed to wait");
        assert!(output.status.success());
    }

    #[test]
    fn trigger_with_version_info() {
        // Skip if not on Arch Linux
        if !has_pactree() || !has_pacman() {
            return;
        }

        use std::io::Write;
        use std::process::Stdio;

        let mut child = Command::new(env!("CARGO_BIN_EXE_anneal"))
            .args(["trigger", "--dry-run"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn");

        // Write package with version info (minor change should trigger with default threshold)
        {
            let stdin = child.stdin.as_mut().expect("failed to get stdin");
            writeln!(stdin, "qt6-base:6.6.0:6.7.0").expect("failed to write");
        }

        let output = child.wait_with_output().expect("failed to wait");
        assert!(output.status.success());
        // Should not mention "below threshold" since minor change exceeds minor threshold
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.contains("below"), "stderr: {stderr}");
    }

    #[test]
    fn trigger_below_threshold() {
        // Skip if not on Arch Linux
        if !has_pactree() || !has_pacman() {
            return;
        }

        use std::io::Write;
        use std::process::Stdio;

        let mut child = Command::new(env!("CARGO_BIN_EXE_anneal"))
            .args(["trigger", "--dry-run"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn");

        // Write package with patch-only change (should be skipped with default minor threshold)
        {
            let stdin = child.stdin.as_mut().expect("failed to get stdin");
            writeln!(stdin, "qt6-base:6.7.0:6.7.1").expect("failed to write");
        }

        let output = child.wait_with_output().expect("failed to wait");
        assert!(output.status.success());
        // Should mention skipped due to threshold
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("below") || stderr.contains("Skipped"),
            "expected threshold skip message, got stderr: {stderr}"
        );
    }
}

mod overrides {
    use anneal::overrides::{Overrides, matches_glob};
    use std::collections::HashSet;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_override_file(dir: &std::path::Path, name: &str, content: &str) {
        let path = dir.join(format!("{name}.conf"));
        let mut file = fs::File::create(&path).expect("failed to create file");
        file.write_all(content.as_bytes())
            .expect("failed to write file");
    }

    #[test]
    fn load_trigger_override_with_patterns() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let triggers_dir = temp.path().join("triggers");
        let packages_dir = temp.path().join("packages");
        fs::create_dir(&triggers_dir).expect("failed to create triggers dir");
        fs::create_dir(&packages_dir).expect("failed to create packages dir");

        // Create a trigger override with patterns
        create_override_file(&triggers_dir, "custom-lib", "custom-app\ncustom-*\n");

        let overrides = Overrides::load_from_paths(&triggers_dir, &packages_dir);

        // Should recognize custom-lib as a user trigger
        assert!(overrides.is_user_trigger("custom-lib"));
        assert!(!overrides.is_user_trigger("qt6-base")); // Not a user trigger

        // Test target matching
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
            .expect("should have override");

        assert!(targets.contains(&"custom-app".to_string()));
        assert!(targets.contains(&"custom-tool".to_string()));
        assert!(!targets.contains(&"custom-bin".to_string())); // -bin filtered
        assert!(!targets.contains(&"other-pkg".to_string()));
    }

    #[test]
    fn load_trigger_override_disabled() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let triggers_dir = temp.path().join("triggers");
        let packages_dir = temp.path().join("packages");
        fs::create_dir(&triggers_dir).expect("failed to create triggers dir");
        fs::create_dir(&packages_dir).expect("failed to create packages dir");

        // Create an empty trigger override (disables the trigger)
        create_override_file(&triggers_dir, "disabled-trigger", "");

        let overrides = Overrides::load_from_paths(&triggers_dir, &packages_dir);

        assert!(overrides.is_user_trigger("disabled-trigger"));

        let aur_packages: HashSet<String> =
            ["pkg1", "pkg2"].into_iter().map(String::from).collect();

        let targets = overrides
            .get_trigger_targets("disabled-trigger", &aur_packages)
            .expect("should have override");

        assert!(targets.is_empty(), "disabled trigger should mark nothing");
    }

    #[test]
    fn load_package_override_never_mark() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let triggers_dir = temp.path().join("triggers");
        let packages_dir = temp.path().join("packages");
        fs::create_dir(&triggers_dir).expect("failed to create triggers dir");
        fs::create_dir(&packages_dir).expect("failed to create packages dir");

        // Create an empty package override (never mark)
        create_override_file(&packages_dir, "protected-pkg", "");

        let overrides = Overrides::load_from_paths(&triggers_dir, &packages_dir);

        // Package should never be marked
        assert!(!overrides.should_mark_package("protected-pkg", "qt6-base"));
        assert!(!overrides.should_mark_package("protected-pkg", "any-trigger"));

        // Other packages should still be markable
        assert!(overrides.should_mark_package("other-pkg", "qt6-base"));
    }

    #[test]
    fn load_package_override_restricted_triggers() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let triggers_dir = temp.path().join("triggers");
        let packages_dir = temp.path().join("packages");
        fs::create_dir(&triggers_dir).expect("failed to create triggers dir");
        fs::create_dir(&packages_dir).expect("failed to create packages dir");

        // Create a package override that only allows qt6-base
        create_override_file(&packages_dir, "qt-only-pkg", "qt6-base\nqt6-*\n");

        let overrides = Overrides::load_from_paths(&triggers_dir, &packages_dir);

        // Should be marked by qt6-base
        assert!(overrides.should_mark_package("qt-only-pkg", "qt6-base"));
        // Should be marked by qt6-svg (matches qt6-*)
        assert!(overrides.should_mark_package("qt-only-pkg", "qt6-svg"));
        // Should NOT be marked by gtk4
        assert!(!overrides.should_mark_package("qt-only-pkg", "gtk4"));
        assert!(!overrides.should_mark_package("qt-only-pkg", "boost"));
    }

    #[test]
    fn load_ignores_non_conf_files() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let triggers_dir = temp.path().join("triggers");
        let packages_dir = temp.path().join("packages");
        fs::create_dir(&triggers_dir).expect("failed to create triggers dir");
        fs::create_dir(&packages_dir).expect("failed to create packages dir");

        // Create files that should be ignored (wrong extension)
        fs::write(triggers_dir.join("readme.txt"), "ignore me").expect("failed to write");
        fs::write(triggers_dir.join("backup.conf.bak"), "ignore me").expect("failed to write");
        fs::write(triggers_dir.join("no-extension"), "ignore me").expect("failed to write");

        // Create a valid override
        create_override_file(&triggers_dir, "valid-trigger", "pkg1\n");

        let overrides = Overrides::load_from_paths(&triggers_dir, &packages_dir);

        assert!(overrides.is_user_trigger("valid-trigger"));
        assert!(!overrides.is_user_trigger("readme"));
        assert!(!overrides.is_user_trigger("backup.conf"));
        assert!(!overrides.is_user_trigger("no-extension"));
    }

    #[test]
    fn load_handles_comments_and_whitespace() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let triggers_dir = temp.path().join("triggers");
        let packages_dir = temp.path().join("packages");
        fs::create_dir(&triggers_dir).expect("failed to create triggers dir");
        fs::create_dir(&packages_dir).expect("failed to create packages dir");

        // Create a trigger override with comments and whitespace
        create_override_file(
            &triggers_dir,
            "commented-trigger",
            "# This is a comment\n\n  pkg1  \n# Another comment\npkg2\n\n",
        );

        let overrides = Overrides::load_from_paths(&triggers_dir, &packages_dir);

        let aur_packages: HashSet<String> = ["pkg1", "pkg2", "pkg3"]
            .into_iter()
            .map(String::from)
            .collect();

        let targets = overrides
            .get_trigger_targets("commented-trigger", &aur_packages)
            .expect("should have override");

        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&"pkg1".to_string()));
        assert!(targets.contains(&"pkg2".to_string()));
        assert!(!targets.contains(&"pkg3".to_string()));
    }

    #[test]
    fn load_missing_directories() {
        let temp = TempDir::new().expect("failed to create temp dir");
        let triggers_dir = temp.path().join("nonexistent-triggers");
        let packages_dir = temp.path().join("nonexistent-packages");

        // Should not panic, just return empty overrides
        let overrides = Overrides::load_from_paths(&triggers_dir, &packages_dir);

        assert!(!overrides.is_user_trigger("anything"));
        assert!(overrides.should_mark_package("any-pkg", "any-trigger"));
    }

    #[test]
    fn glob_pattern_matching() {
        // Test various glob patterns
        assert!(matches_glob("*-git", "neovim-git"));
        assert!(matches_glob("*-git", "foo-bar-git"));
        assert!(!matches_glob("*-git", "neovim-git-extra"));

        assert!(matches_glob("python-*", "python-requests"));
        assert!(matches_glob("python-*", "python-"));
        assert!(!matches_glob("python-*", "python"));

        assert!(matches_glob("*", "anything"));
        assert!(matches_glob("*", ""));

        assert!(matches_glob("qt?-base", "qt6-base"));
        assert!(matches_glob("qt?-base", "qt5-base"));
        assert!(!matches_glob("qt?-base", "qt-base"));
        assert!(!matches_glob("qt?-base", "qt66-base"));
    }
}
