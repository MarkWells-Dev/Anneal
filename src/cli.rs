// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Command-line interface definition.
//!
//! Uses clap for argument parsing with derive macros.

use clap::{Parser, Subcommand};
use clap_complete::Shell;

/// Proactive AUR rebuild management for Arch Linux.
#[derive(Parser, Debug)]
#[command(name = "anneal")]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Suppress stdout (errors still go to stderr).
    #[arg(long, short, global = true)]
    pub quiet: bool,

    /// The subcommand to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Add packages to the rebuild queue.
    Mark {
        /// Packages to mark for rebuild.
        #[arg(required = true)]
        packages: Vec<String>,

        /// Trigger package that caused the mark.
        #[arg(long)]
        trigger: Option<String>,

        /// Version of the trigger package.
        #[arg(long = "trigger-version", requires = "trigger")]
        trigger_version: Option<String>,
    },

    /// Remove packages from the rebuild queue.
    Unmark {
        /// Packages to remove (reads from stdin if empty).
        packages: Vec<String>,

        /// Exit with code 2 if any package wasn't in the queue.
        #[arg(long)]
        strict: bool,
    },

    /// Show the current rebuild queue.
    List,

    /// Reset the rebuild queue.
    Clear {
        /// Skip confirmation prompt.
        #[arg(short, long)]
        force: bool,

        /// Only clear events for this trigger (keeps queue intact).
        trigger: Option<String>,
    },

    /// Rebuild queued packages.
    Rebuild {
        /// Skip confirmation prompt.
        #[arg(short, long)]
        force: bool,

        /// Include packages detected by checkrebuild.
        #[arg(long)]
        checkrebuild: bool,

        /// Override the configured AUR helper.
        #[arg(long)]
        cmd: Option<String>,

        /// Only rebuild these packages (must be in queue).
        packages: Vec<String>,

        /// Additional arguments passed to the AUR helper.
        #[arg(last = true)]
        helper_args: Vec<String>,
    },

    /// Check if a package is marked for rebuild.
    #[command(name = "ismarked")]
    IsMarked {
        /// Package to check.
        package: String,
    },

    /// Print which of the given packages are in the queue.
    Query {
        /// Packages to check.
        #[arg(required = true)]
        packages: Vec<String>,
    },

    /// List configured triggers.
    Triggers,

    /// Process triggers from upgraded packages.
    Trigger {
        /// Show what would be marked without modifying the queue.
        #[arg(long)]
        dry_run: bool,

        /// Packages to process (reads from stdin if empty).
        packages: Vec<String>,
    },

    /// Dump current configuration.
    Config,

    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },
}

impl Command {
    /// Returns true if this command requires root privileges.
    pub fn requires_root(&self) -> bool {
        match self {
            Self::Mark { .. } | Self::Unmark { .. } | Self::Clear { .. } => true,
            Self::Trigger { dry_run, .. } => !dry_run,
            _ => false,
        }
    }

    /// Returns true if this command modifies the queue (excluding dry-run).
    pub fn modifies_queue(&self) -> bool {
        match self {
            Self::Mark { .. } | Self::Unmark { .. } | Self::Clear { .. } => true,
            Self::Trigger { dry_run, .. } => !dry_run,
            _ => false,
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        // Clap's built-in validation
        Cli::command().debug_assert();
    }

    #[test]
    fn parse_mark() {
        let cli = Cli::parse_from(["anneal", "mark", "pkg1", "pkg2"]);
        match cli.command {
            Command::Mark {
                packages,
                trigger,
                trigger_version,
            } => {
                assert_eq!(packages, vec!["pkg1", "pkg2"]);
                assert!(trigger.is_none());
                assert!(trigger_version.is_none());
            }
            _ => panic!("expected Mark command"),
        }
    }

    #[test]
    fn parse_mark_with_trigger() {
        let cli = Cli::parse_from([
            "anneal",
            "mark",
            "pkg1",
            "--trigger",
            "qt6-base",
            "--trigger-version",
            "6.7.0",
        ]);
        match cli.command {
            Command::Mark {
                packages,
                trigger,
                trigger_version,
            } => {
                assert_eq!(packages, vec!["pkg1"]);
                assert_eq!(trigger, Some("qt6-base".to_string()));
                assert_eq!(trigger_version, Some("6.7.0".to_string()));
            }
            _ => panic!("expected Mark command"),
        }
    }

    #[test]
    fn parse_unmark() {
        let cli = Cli::parse_from(["anneal", "unmark", "pkg1"]);
        match cli.command {
            Command::Unmark { packages, strict } => {
                assert_eq!(packages, vec!["pkg1"]);
                assert!(!strict);
            }
            _ => panic!("expected Unmark command"),
        }
    }

    #[test]
    fn parse_unmark_strict() {
        let cli = Cli::parse_from(["anneal", "unmark", "--strict", "pkg1"]);
        match cli.command {
            Command::Unmark { strict, .. } => assert!(strict),
            _ => panic!("expected Unmark command"),
        }
    }

    #[test]
    fn parse_list() {
        let cli = Cli::parse_from(["anneal", "list"]);
        assert!(matches!(cli.command, Command::List));
    }

    #[test]
    fn parse_clear() {
        let cli = Cli::parse_from(["anneal", "clear"]);
        match cli.command {
            Command::Clear { force, trigger } => {
                assert!(!force);
                assert!(trigger.is_none());
            }
            _ => panic!("expected Clear command"),
        }
    }

    #[test]
    fn parse_clear_force() {
        let cli = Cli::parse_from(["anneal", "clear", "-f"]);
        match cli.command {
            Command::Clear { force, .. } => assert!(force),
            _ => panic!("expected Clear command"),
        }
    }

    #[test]
    fn parse_clear_trigger() {
        let cli = Cli::parse_from(["anneal", "clear", "qt6-base"]);
        match cli.command {
            Command::Clear { trigger, .. } => {
                assert_eq!(trigger, Some("qt6-base".to_string()));
            }
            _ => panic!("expected Clear command"),
        }
    }

    #[test]
    fn parse_rebuild() {
        let cli = Cli::parse_from(["anneal", "rebuild"]);
        match cli.command {
            Command::Rebuild {
                force,
                checkrebuild,
                cmd,
                packages,
                helper_args,
            } => {
                assert!(!force);
                assert!(!checkrebuild);
                assert!(cmd.is_none());
                assert!(packages.is_empty());
                assert!(helper_args.is_empty());
            }
            _ => panic!("expected Rebuild command"),
        }
    }

    #[test]
    fn parse_rebuild_with_options() {
        let cli = Cli::parse_from([
            "anneal",
            "rebuild",
            "-f",
            "--checkrebuild",
            "--cmd",
            "yay",
            "pkg1",
            "--",
            "--noconfirm",
        ]);
        match cli.command {
            Command::Rebuild {
                force,
                checkrebuild,
                cmd,
                packages,
                helper_args,
            } => {
                assert!(force);
                assert!(checkrebuild);
                assert_eq!(cmd, Some("yay".to_string()));
                assert_eq!(packages, vec!["pkg1"]);
                assert_eq!(helper_args, vec!["--noconfirm"]);
            }
            _ => panic!("expected Rebuild command"),
        }
    }

    #[test]
    fn parse_ismarked() {
        let cli = Cli::parse_from(["anneal", "ismarked", "pkg1"]);
        match cli.command {
            Command::IsMarked { package } => assert_eq!(package, "pkg1"),
            _ => panic!("expected IsMarked command"),
        }
    }

    #[test]
    fn parse_query() {
        let cli = Cli::parse_from(["anneal", "query", "pkg1", "pkg2"]);
        match cli.command {
            Command::Query { packages } => {
                assert_eq!(packages, vec!["pkg1", "pkg2"]);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn parse_triggers() {
        let cli = Cli::parse_from(["anneal", "triggers"]);
        assert!(matches!(cli.command, Command::Triggers));
    }

    #[test]
    fn parse_trigger() {
        let cli = Cli::parse_from(["anneal", "trigger", "qt6-base"]);
        match cli.command {
            Command::Trigger { dry_run, packages } => {
                assert!(!dry_run);
                assert_eq!(packages, vec!["qt6-base"]);
            }
            _ => panic!("expected Trigger command"),
        }
    }

    #[test]
    fn parse_trigger_dry_run() {
        let cli = Cli::parse_from(["anneal", "trigger", "--dry-run", "qt6-base"]);
        match cli.command {
            Command::Trigger { dry_run, .. } => assert!(dry_run),
            _ => panic!("expected Trigger command"),
        }
    }

    #[test]
    fn parse_config() {
        let cli = Cli::parse_from(["anneal", "config"]);
        assert!(matches!(cli.command, Command::Config));
    }

    #[test]
    fn quiet_flag_global() {
        let cli = Cli::parse_from(["anneal", "--quiet", "list"]);
        assert!(cli.quiet);

        let cli = Cli::parse_from(["anneal", "list", "--quiet"]);
        assert!(cli.quiet);
    }

    #[test]
    fn requires_root() {
        assert!(
            Command::Mark {
                packages: vec![],
                trigger: None,
                trigger_version: None
            }
            .requires_root()
        );
        assert!(
            Command::Unmark {
                packages: vec![],
                strict: false
            }
            .requires_root()
        );
        assert!(
            Command::Clear {
                force: false,
                trigger: None
            }
            .requires_root()
        );
        assert!(
            Command::Trigger {
                dry_run: false,
                packages: vec![]
            }
            .requires_root()
        );

        // dry_run doesn't require root
        assert!(
            !Command::Trigger {
                dry_run: true,
                packages: vec![]
            }
            .requires_root()
        );

        assert!(!Command::List.requires_root());
        assert!(
            !Command::IsMarked {
                package: String::new()
            }
            .requires_root()
        );
        assert!(!Command::Query { packages: vec![] }.requires_root());
        assert!(!Command::Triggers.requires_root());
        assert!(!Command::Config.requires_root());
        assert!(
            !Command::Rebuild {
                force: false,
                checkrebuild: false,
                cmd: None,
                packages: vec![],
                helper_args: vec![],
            }
            .requires_root()
        );
    }

    #[test]
    fn modifies_queue() {
        assert!(
            Command::Mark {
                packages: vec![],
                trigger: None,
                trigger_version: None
            }
            .modifies_queue()
        );
        assert!(
            Command::Unmark {
                packages: vec![],
                strict: false
            }
            .modifies_queue()
        );
        assert!(
            Command::Clear {
                force: false,
                trigger: None
            }
            .modifies_queue()
        );
        assert!(
            Command::Trigger {
                dry_run: false,
                packages: vec![]
            }
            .modifies_queue()
        );

        // dry_run does not modify
        assert!(
            !Command::Trigger {
                dry_run: true,
                packages: vec![]
            }
            .modifies_queue()
        );

        assert!(!Command::List.modifies_queue());
        assert!(
            !Command::IsMarked {
                package: String::new()
            }
            .modifies_queue()
        );
    }
}
