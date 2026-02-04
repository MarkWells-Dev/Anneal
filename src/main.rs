// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Anneal CLI - Proactive AUR rebuild management for Arch Linux.

use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;

use anneal::cli::{Cli, Command};
use anneal::config::Config;
use anneal::db::{DB_PATH, Database, DbError};
use anneal::trigger::{TriggerError, process_triggers};
use anneal::triggers::{TRIGGER_LIST_VERSION, TRIGGERS};
use clap::Parser;

/// Exit codes.
mod exit {
    pub const SUCCESS: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const NOT_FOUND: u8 = 2;
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Check root requirement
    if cli.command.requires_root() && !is_root() {
        eprintln!("[anneal] error: Permission denied. This command requires root privileges.");
        return ExitCode::from(exit::ERROR);
    }

    // Check quiet + confirmation conflict
    if cli.quiet && needs_confirmation(&cli.command) && !has_force_flag(&cli.command) {
        eprintln!("[anneal] error: Cannot prompt for confirmation with --quiet. Use -f to force.");
        return ExitCode::from(exit::ERROR);
    }

    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("[anneal] error: {e}");
            ExitCode::from(exit::ERROR)
        }
    }
}

/// Run the CLI command.
fn run(cli: Cli) -> Result<u8, Error> {
    let config = Config::load()?;

    match cli.command {
        Command::Mark {
            packages,
            trigger,
            trigger_version,
        } => cmd_mark(
            &packages,
            trigger.as_deref(),
            trigger_version.as_deref(),
            cli.quiet,
        ),

        Command::Unmark { packages, strict } => cmd_unmark(packages, strict, cli.quiet),

        Command::List => cmd_list(cli.quiet),

        Command::Clear { force, trigger } => cmd_clear(force, trigger.as_deref(), cli.quiet),

        Command::Rebuild {
            force,
            checkrebuild,
            cmd,
            packages,
            helper_args,
        } => cmd_rebuild(
            &config,
            force,
            checkrebuild,
            cmd.as_deref(),
            &packages,
            &helper_args,
            cli.quiet,
        ),

        Command::IsMarked { package } => cmd_ismarked(&package),

        Command::Query { packages } => cmd_query(&packages, cli.quiet),

        Command::Triggers => cmd_triggers(cli.quiet),

        Command::Trigger { dry_run, packages } => {
            cmd_trigger(&config, dry_run, packages, cli.quiet)
        }

        Command::Config => cmd_config(&config, cli.quiet),
    }
}

// ==================== Command Implementations ====================

fn cmd_mark(
    packages: &[String],
    trigger: Option<&str>,
    trigger_version: Option<&str>,
    quiet: bool,
) -> Result<u8, Error> {
    let mut db = Database::open(90)?; // TODO: use config.retention_days

    let mut newly_marked = 0;
    for pkg in packages {
        if db.mark(pkg, trigger, trigger_version)? {
            newly_marked += 1;
        }
    }

    if !quiet {
        match trigger {
            Some(t) => {
                println!("[anneal] Marked {newly_marked} package(s) for rebuild (trigger: {t})")
            }
            None => println!("[anneal] Marked {newly_marked} package(s) for rebuild"),
        }
    }

    Ok(exit::SUCCESS)
}

fn cmd_unmark(packages: Vec<String>, strict: bool, quiet: bool) -> Result<u8, Error> {
    let packages = if packages.is_empty() {
        read_stdin_packages()?
    } else {
        packages
    };

    if packages.is_empty() {
        if !quiet {
            println!("[anneal] No packages specified");
        }
        return Ok(exit::SUCCESS);
    }

    let mut db = Database::open(90)?;
    let mut removed = 0;
    let mut not_found = Vec::new();

    for pkg in &packages {
        if db.unmark(pkg)? {
            removed += 1;
        } else {
            not_found.push(pkg.as_str());
        }
    }

    if !quiet {
        println!("[anneal] Removed {removed} package(s) from queue");
    }

    if strict && !not_found.is_empty() {
        eprintln!("[anneal] warning: Not in queue: {}", not_found.join(", "));
        return Ok(exit::NOT_FOUND);
    }

    Ok(exit::SUCCESS)
}

fn cmd_list(quiet: bool) -> Result<u8, Error> {
    let db = open_readonly()?;
    let queue = db.list()?;

    if queue.is_empty() {
        if !quiet {
            println!("[anneal] No packages in queue");
        }
        return Ok(exit::SUCCESS);
    }

    for entry in &queue {
        // Get the most recent trigger event for context
        if let Some(event) = db.get_latest_event(&entry.package)? {
            match event.trigger_package {
                Some(trigger) => println!("{} ({})", entry.package, trigger),
                None => println!("{} (external)", entry.package),
            }
        } else {
            println!("{}", entry.package);
        }
    }

    if !quiet {
        eprintln!("[anneal] {} package(s) in queue", queue.len());
    }

    Ok(exit::SUCCESS)
}

fn cmd_clear(force: bool, trigger: Option<&str>, quiet: bool) -> Result<u8, Error> {
    let mut db = Database::open(90)?;

    if let Some(trigger_name) = trigger {
        // Clear events for a specific trigger
        let count = db.clear_trigger_events(trigger_name)?;
        if !quiet {
            println!("[anneal] Cleared {count} event(s) for trigger '{trigger_name}'");
        }
    } else {
        // Clear entire queue
        let queue = db.list()?;
        if queue.is_empty() {
            if !quiet {
                println!("[anneal] Queue is already empty");
            }
            return Ok(exit::SUCCESS);
        }

        if !force {
            eprint!(
                "[anneal] Clear {} package(s) from queue? [y/N] ",
                queue.len()
            );
            io::stderr().flush().ok();

            if !confirm()? {
                if !quiet {
                    println!("[anneal] Cancelled");
                }
                return Ok(exit::SUCCESS);
            }
        }

        let count = db.clear()?;
        if !quiet {
            println!("[anneal] Cleared {count} package(s) from queue");
        }
    }

    Ok(exit::SUCCESS)
}

fn cmd_rebuild(
    _config: &Config,
    _force: bool,
    _checkrebuild: bool,
    _cmd: Option<&str>,
    _packages: &[String],
    _helper_args: &[String],
    _quiet: bool,
) -> Result<u8, Error> {
    // TODO: Implement rebuild command
    eprintln!("[anneal] error: rebuild command not yet implemented");
    Ok(exit::ERROR)
}

fn cmd_ismarked(package: &str) -> Result<u8, Error> {
    let db = open_readonly()?;

    if db.is_marked(package)? {
        Ok(exit::SUCCESS)
    } else {
        Ok(exit::NOT_FOUND)
    }
}

fn cmd_query(packages: &[String], quiet: bool) -> Result<u8, Error> {
    let db = open_readonly()?;
    let pkg_refs: Vec<&str> = packages.iter().map(String::as_str).collect();
    let found = db.query(&pkg_refs)?;

    for pkg in &found {
        println!("{pkg}");
    }

    if !quiet && found.is_empty() {
        // Silent for scripting, but show feedback when interactive
    }

    Ok(exit::SUCCESS)
}

fn cmd_triggers(quiet: bool) -> Result<u8, Error> {
    if !quiet {
        println!("[anneal] Curated triggers (v{TRIGGER_LIST_VERSION}):");
    }

    for trigger in TRIGGERS {
        println!("{trigger}");
    }

    Ok(exit::SUCCESS)
}

fn cmd_trigger(
    config: &Config,
    dry_run: bool,
    packages: Vec<String>,
    quiet: bool,
) -> Result<u8, Error> {
    let packages = if packages.is_empty() {
        read_stdin_packages()?
    } else {
        packages
    };

    if packages.is_empty() {
        return Ok(exit::SUCCESS);
    }

    // Process triggers to find AUR dependents
    let result = process_triggers(&packages, config.version_threshold)?;

    // Report packages skipped due to version threshold
    if !quiet && !result.below_threshold.is_empty() {
        eprintln!(
            "[anneal] Skipped {} trigger(s) below {} threshold",
            result.below_threshold.len(),
            config.version_threshold.as_str()
        );
    }

    if result.marked.is_empty() {
        if !quiet {
            eprintln!("[anneal] No packages to mark");
        }
        return Ok(exit::SUCCESS);
    }

    if dry_run {
        // Just print what would be marked
        for m in &result.marked {
            println!("{} ({})", m.package, m.trigger);
        }
        if !quiet {
            eprintln!(
                "[anneal] Would mark {} package(s) for rebuild",
                result.marked.len()
            );
        }
    } else {
        // Actually mark the packages
        let mut db = Database::open(config.retention_days)?;
        let mut newly_marked = 0;

        for m in &result.marked {
            if db.mark(&m.package, Some(&m.trigger), None)? {
                newly_marked += 1;
                if !quiet {
                    println!("[anneal] Marked {} (triggered by {})", m.package, m.trigger);
                }
            }
        }

        if !quiet {
            eprintln!("[anneal] Marked {newly_marked} package(s) for rebuild");
        }
    }

    Ok(exit::SUCCESS)
}

fn cmd_config(config: &Config, quiet: bool) -> Result<u8, Error> {
    if !quiet {
        print!("{}", config.to_conf());
    }
    Ok(exit::SUCCESS)
}

// ==================== Helper Functions ====================

/// Check if running as root.
fn is_root() -> bool {
    // SAFETY: getuid is always safe to call
    unsafe { libc::getuid() == 0 }
}

/// Check if a command needs confirmation.
fn needs_confirmation(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Clear {
            force: false,
            trigger: None
        } | Command::Rebuild { force: false, .. }
    )
}

/// Check if a command has the force flag set.
fn has_force_flag(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Clear { force: true, .. } | Command::Rebuild { force: true, .. }
    )
}

/// Open the database in read-only mode, with a helpful error if it doesn't exist.
fn open_readonly() -> Result<Database, Error> {
    Database::open_readonly(std::path::Path::new(DB_PATH)).map_err(|e| {
        if matches!(&e, DbError::Sqlite(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::CannotOpen)
        {
            Error::NoDatabase
        } else {
            e.into()
        }
    })
}

/// Read packages from stdin (one per line).
fn read_stdin_packages() -> Result<Vec<String>, Error> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        // Don't block waiting for input if stdin is a terminal
        return Ok(Vec::new());
    }

    let packages: Vec<String> = stdin
        .lock()
        .lines()
        .map_while(Result::ok)
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    Ok(packages)
}

/// Read confirmation from user.
fn confirm() -> Result<bool, Error> {
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    Ok(line.trim().eq_ignore_ascii_case("y") || line.trim().eq_ignore_ascii_case("yes"))
}

// ==================== Error Handling ====================

/// Application errors.
#[derive(Debug)]
enum Error {
    Config(anneal::config::ConfigError),
    Db(anneal::db::DbError),
    Trigger(TriggerError),
    Io(io::Error),
    NoDatabase,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "{e}"),
            Self::Db(e) => write!(f, "{e}"),
            Self::Trigger(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::NoDatabase => write!(
                f,
                "No database found at {DB_PATH}. Run a command as root first to create it."
            ),
        }
    }
}

impl From<anneal::config::ConfigError> for Error {
    fn from(e: anneal::config::ConfigError) -> Self {
        Self::Config(e)
    }
}

impl From<anneal::db::DbError> for Error {
    fn from(e: anneal::db::DbError) -> Self {
        Self::Db(e)
    }
}

impl From<TriggerError> for Error {
    fn from(e: TriggerError) -> Self {
        Self::Trigger(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
