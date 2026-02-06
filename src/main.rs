// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Anneal CLI - Proactive AUR rebuild management for Arch Linux.

use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::process::{Command as ProcessCommand, ExitCode, Stdio};

use anneal::cli::{Cli, Command};
use anneal::config::{Config, KNOWN_HELPERS};
use anneal::db::{DB_PATH, Database, DbError};
use anneal::output;
use anneal::overrides::Overrides;
use anneal::trigger::{TriggerError, process_triggers};
use anneal::triggers::{TRIGGER_LIST_VERSION, TRIGGERS};
use clap::{CommandFactory, Parser};
use clap_complete::generate;

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
        output::error("Permission denied. This command requires root privileges.");
        return ExitCode::from(exit::ERROR);
    }

    // Check quiet + confirmation conflict
    if cli.quiet && needs_confirmation(&cli.command) && !has_force_flag(&cli.command) {
        output::error("Cannot prompt for confirmation with --quiet. Use -f to force.");
        return ExitCode::from(exit::ERROR);
    }

    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            output::error(&e.to_string());
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
            &config,
            &packages,
            trigger.as_deref(),
            trigger_version.as_deref(),
            cli.quiet,
        ),

        Command::Unmark { packages, strict } => cmd_unmark(&config, packages, strict, cli.quiet),

        Command::List => cmd_list(cli.quiet),

        Command::Clear { force, trigger } => {
            cmd_clear(&config, force, trigger.as_deref(), cli.quiet)
        }

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

        Command::Completions { shell } => {
            cmd_completions(shell);
            Ok(exit::SUCCESS)
        }
    }
}

// ==================== Rebuild Types ====================

/// Rebuild-specific errors.
#[derive(Debug)]
enum RebuildError {
    /// No AUR helper found in PATH.
    NoHelper,
    /// Multiple AUR helpers found, user must configure one.
    AmbiguousHelper(Vec<String>),
    /// Specified helper not found in PATH.
    HelperNotFound(String),
    /// Helper process failed to start.
    HelperSpawn(io::Error),
    /// Helper exited with non-zero code.
    HelperFailed(i32),
    /// checkrebuild command failed.
    CheckrebuildFailed(io::Error),
    /// Package not in queue (without -f flag).
    PackageNotInQueue(String),
}

impl std::fmt::Display for RebuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHelper => write!(
                f,
                "No AUR helper detected. Set 'helper' in /etc/anneal/config.conf\nSupported helpers: {}",
                KNOWN_HELPERS.join(", ")
            ),
            Self::AmbiguousHelper(helpers) => write!(
                f,
                "Multiple AUR helpers found: {}. Set 'helper' in /etc/anneal/config.conf",
                helpers.join(", ")
            ),
            Self::HelperNotFound(name) => write!(f, "AUR helper '{name}' not found in PATH"),
            Self::HelperSpawn(e) => write!(f, "Failed to start AUR helper: {e}"),
            Self::HelperFailed(code) => write!(f, "AUR helper exited with code {code}"),
            Self::CheckrebuildFailed(e) => write!(f, "Failed to run checkrebuild: {e}"),
            Self::PackageNotInQueue(pkg) => {
                write!(f, "Package '{pkg}' is not in the queue (use -f to force)")
            }
        }
    }
}

/// Information about how to invoke an AUR helper.
struct HelperInvocation {
    /// The command to run (e.g., "paru").
    command: String,
    /// Base arguments for rebuild (e.g., ["-S", "--rebuild"]).
    base_args: Vec<String>,
}

impl HelperInvocation {
    /// Create invocation for a known helper.
    fn for_known_helper(name: &str) -> Self {
        let base_args = match name {
            "aura" => vec!["-A".to_string(), "--rebuild".to_string()],
            _ => vec!["-S".to_string(), "--rebuild".to_string()],
        };
        Self {
            command: name.to_string(),
            base_args,
        }
    }

    /// Create invocation from a custom command string.
    fn from_custom(cmd: &str) -> Self {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            // Shouldn't happen, but handle gracefully
            Self {
                command: cmd.to_string(),
                base_args: vec![],
            }
        } else {
            Self {
                command: parts[0].to_string(),
                base_args: parts[1..].iter().map(|s| s.to_string()).collect(),
            }
        }
    }
}

// ==================== Command Implementations ====================

fn cmd_mark(
    config: &Config,
    packages: &[String],
    trigger: Option<&str>,
    trigger_version: Option<&str>,
    quiet: bool,
) -> Result<u8, Error> {
    let mut db = Database::open(config.retention_days)?;

    let mut newly_marked = 0;
    for pkg in packages {
        if db.mark(pkg, trigger, trigger_version)? {
            newly_marked += 1;
        }
    }

    if !quiet {
        match trigger {
            Some(t) => output::status(&format!(
                "Marked {newly_marked} package(s) for rebuild (trigger: {t})"
            )),
            None => output::success_count("Marked", newly_marked),
        }
    }

    Ok(exit::SUCCESS)
}

fn cmd_unmark(
    config: &Config,
    packages: Vec<String>,
    strict: bool,
    quiet: bool,
) -> Result<u8, Error> {
    let packages = if packages.is_empty() {
        read_stdin_packages()?
    } else {
        packages
    };

    if packages.is_empty() {
        if !quiet {
            output::status("No packages specified");
        }
        return Ok(exit::SUCCESS);
    }

    let mut db = Database::open(config.retention_days)?;
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
        output::success_count("Removed", removed);
    }

    if strict && !not_found.is_empty() {
        output::warning(&format!("Not in queue: {}", not_found.join(", ")));
        return Ok(exit::NOT_FOUND);
    }

    Ok(exit::SUCCESS)
}

fn cmd_list(quiet: bool) -> Result<u8, Error> {
    let db = open_readonly()?;
    let queue = db.list()?;

    if queue.is_empty() {
        if !quiet {
            output::status("No packages in queue");
        }
        return Ok(exit::SUCCESS);
    }

    for entry in &queue {
        // Get the most recent trigger event for context
        if let Some(event) = db.get_latest_event(&entry.package)? {
            match event.trigger_package {
                Some(ref trigger) => output::package_with_trigger(&entry.package, trigger),
                None => output::package_with_trigger(&entry.package, "external"),
            }
        } else {
            output::package(&entry.package);
        }
    }

    if !quiet {
        output::info(&format!("{} package(s) in queue", queue.len()));
    }

    Ok(exit::SUCCESS)
}

fn cmd_clear(
    config: &Config,
    force: bool,
    trigger: Option<&str>,
    quiet: bool,
) -> Result<u8, Error> {
    let mut db = Database::open(config.retention_days)?;

    if let Some(trigger_name) = trigger {
        // Clear events for a specific trigger
        let count = db.clear_trigger_events(trigger_name)?;
        if !quiet {
            output::status(&format!(
                "Cleared {count} event(s) for trigger '{trigger_name}'"
            ));
        }
    } else {
        // Clear entire queue
        let queue = db.list()?;
        if queue.is_empty() {
            if !quiet {
                output::status("Queue is already empty");
            }
            return Ok(exit::SUCCESS);
        }

        if !force {
            eprint!(":: Clear {} package(s) from queue? [y/N] ", queue.len());
            io::stderr().flush().ok();

            if !confirm()? {
                if !quiet {
                    output::status("Cancelled");
                }
                return Ok(exit::SUCCESS);
            }
        }

        let count = db.clear()?;
        if !quiet {
            output::success_count("Cleared", count);
        }
    }

    Ok(exit::SUCCESS)
}

fn cmd_rebuild(
    config: &Config,
    force: bool,
    checkrebuild: bool,
    cmd: Option<&str>,
    packages: &[String],
    helper_args: &[String],
    quiet: bool,
) -> Result<u8, Error> {
    // Step 1: Detect helper
    let helper = detect_helper(config, cmd)?;

    // Step 2: Collect packages from queue
    let db = open_readonly()?;
    let queue = db.list()?;
    let queue_set: HashSet<&str> = queue.iter().map(|e| e.package.as_str()).collect();

    // Step 3: Determine which packages to rebuild
    let from_queue: Vec<String> = if packages.is_empty() {
        // Rebuild all queued packages
        queue.iter().map(|e| e.package.clone()).collect()
    } else {
        // Rebuild specified packages
        let mut result = Vec::new();
        for pkg in packages {
            if queue_set.contains(pkg.as_str()) {
                result.push(pkg.clone());
            } else if !force {
                return Err(RebuildError::PackageNotInQueue(pkg.clone()).into());
            } else {
                // With -f, allow packages not in queue
                result.push(pkg.clone());
            }
        }
        result
    };

    // Step 4: Add checkrebuild packages if requested
    let mut from_checkrebuild: Vec<String> = Vec::new();
    if checkrebuild || config.include_checkrebuild {
        match run_checkrebuild() {
            Ok(pkgs) => {
                for pkg in pkgs {
                    // Only add if not already in the list
                    if !from_queue.contains(&pkg) {
                        from_checkrebuild.push(pkg);
                    }
                }
            }
            Err(e) => {
                // Warn but don't fail if checkrebuild isn't available
                output::warning(&e.to_string());
            }
        }
    }

    // Step 5: Check if there's anything to rebuild
    let total_count = from_queue.len() + from_checkrebuild.len();
    if total_count == 0 {
        if !quiet {
            output::status("No packages to rebuild");
        }
        return Ok(exit::SUCCESS);
    }

    // Step 6: Show packages and confirm
    if !quiet {
        if !from_queue.is_empty() {
            output::header("From queue:");
            for pkg in &from_queue {
                eprintln!("  {pkg}");
            }
        }
        if !from_checkrebuild.is_empty() {
            output::header("From checkrebuild:");
            for pkg in &from_checkrebuild {
                eprintln!("  {pkg}");
            }
        }
    }

    if !force {
        eprint!(":: Rebuild {total_count} package(s)? [y/N] ");
        io::stderr().flush().ok();

        if !confirm()? {
            if !quiet {
                output::status("Cancelled");
            }
            return Ok(exit::SUCCESS);
        }
    }

    // Step 7: Build and execute the helper command
    let all_packages: Vec<&str> = from_queue
        .iter()
        .chain(from_checkrebuild.iter())
        .map(String::as_str)
        .collect();

    let status = ProcessCommand::new(&helper.command)
        .args(&helper.base_args)
        .args(&all_packages)
        .args(helper_args)
        .status()
        .map_err(RebuildError::HelperSpawn)?;

    // Step 8: Handle result
    if status.success() {
        // Unmark packages that were in the queue
        if !from_queue.is_empty() {
            let mut db = Database::open(config.retention_days)?;
            for pkg in &from_queue {
                db.unmark(pkg)?;
            }
        }

        if !quiet {
            output::success_count("Successfully rebuilt", total_count);
        }
        Ok(exit::SUCCESS)
    } else {
        let code = status.code().unwrap_or(-1);
        Err(RebuildError::HelperFailed(code).into())
    }
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
        output::header(&format!("Curated triggers (v{TRIGGER_LIST_VERSION})"));
    }

    for (name, threshold) in TRIGGERS {
        if quiet {
            output::package(name);
        } else {
            output::package(&format!(
                "{name} ({threshold})",
                threshold = threshold.as_str()
            ));
        }
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

    // Load user overrides
    let overrides = Overrides::load();

    // Process triggers to find AUR dependents
    let result = process_triggers(&packages, config.version_threshold, &overrides)?;

    // Report packages skipped due to version threshold
    if !quiet && !result.below_threshold.is_empty() {
        output::info(&format!(
            "Skipped {} trigger(s) below threshold",
            result.below_threshold.len(),
        ));
    }

    if result.marked.is_empty() {
        if !quiet {
            output::info("No packages to mark");
        }
        return Ok(exit::SUCCESS);
    }

    if dry_run {
        // Just print what would be marked
        for m in &result.marked {
            output::package_with_trigger(&m.package, &m.trigger);
        }
        if !quiet {
            output::info(&format!(
                "Would mark {} package(s) for rebuild",
                result.marked.len()
            ));
        }
    } else {
        // Actually mark the packages
        let mut db = Database::open(config.retention_days)?;
        let mut newly_marked = 0;

        for m in &result.marked {
            if db.mark(&m.package, Some(&m.trigger), None)? {
                newly_marked += 1;
                if !quiet {
                    output::status(&format!(
                        "Marked {} (triggered by {})",
                        m.package, m.trigger
                    ));
                }
            }
        }

        if !quiet {
            output::info(&format!("Marked {newly_marked} package(s) for rebuild"));
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

fn cmd_completions(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "anneal", &mut io::stdout());
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

// ==================== Rebuild Helpers ====================

/// Detect which AUR helper to use.
fn detect_helper(
    config: &Config,
    cmd_override: Option<&str>,
) -> Result<HelperInvocation, RebuildError> {
    // Priority 1: Command-line override
    if let Some(cmd) = cmd_override {
        return resolve_helper(cmd);
    }

    // Priority 2: Config file
    if let Some(ref helper) = config.helper {
        return resolve_helper(helper);
    }

    // Priority 3: Auto-detect from PATH
    let found: Vec<&str> = KNOWN_HELPERS
        .iter()
        .copied()
        .filter(|h| is_in_path(h))
        .collect();

    match found.len() {
        0 => Err(RebuildError::NoHelper),
        1 => Ok(HelperInvocation::for_known_helper(found[0])),
        _ => Err(RebuildError::AmbiguousHelper(
            found.into_iter().map(String::from).collect(),
        )),
    }
}

/// Resolve a helper string to an invocation.
fn resolve_helper(helper: &str) -> Result<HelperInvocation, RebuildError> {
    // Check if it's a known helper name
    if Config::is_known_helper(helper) {
        if !is_in_path(helper) {
            return Err(RebuildError::HelperNotFound(helper.to_string()));
        }
        return Ok(HelperInvocation::for_known_helper(helper));
    }

    // Custom command - extract first word to verify it exists
    let cmd_name = helper.split_whitespace().next().unwrap_or(helper);
    if !is_in_path(cmd_name) {
        return Err(RebuildError::HelperNotFound(cmd_name.to_string()));
    }

    Ok(HelperInvocation::from_custom(helper))
}

/// Check if a command exists in PATH.
fn is_in_path(cmd: &str) -> bool {
    ProcessCommand::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run checkrebuild and return the list of packages needing rebuild.
fn run_checkrebuild() -> Result<Vec<String>, RebuildError> {
    let output = ProcessCommand::new("checkrebuild")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(RebuildError::CheckrebuildFailed)?;

    // checkrebuild exits 0 regardless of whether packages need rebuild
    let packages: Vec<String> = BufReader::new(&output.stdout[..])
        .lines()
        .map_while(Result::ok)
        .map(|line| {
            // checkrebuild output format: "package_name dependency_that_changed"
            // We only want the package name (first field)
            line.split_whitespace().next().unwrap_or(&line).to_string()
        })
        .filter(|line| !line.is_empty())
        .collect();

    Ok(packages)
}

// ==================== Error Handling ====================

/// Application errors.
#[derive(Debug)]
enum Error {
    Config(anneal::config::ConfigError),
    Db(anneal::db::DbError),
    Trigger(TriggerError),
    Rebuild(RebuildError),
    Io(io::Error),
    NoDatabase,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "{e}"),
            Self::Db(e) => write!(f, "{e}"),
            Self::Trigger(e) => write!(f, "{e}"),
            Self::Rebuild(e) => write!(f, "{e}"),
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

impl From<RebuildError> for Error {
    fn from(e: RebuildError) -> Self {
        Self::Rebuild(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    mod helper_invocation {
        use super::*;

        #[test]
        fn known_helper_paru() {
            let inv = HelperInvocation::for_known_helper("paru");
            assert_eq!(inv.command, "paru");
            assert_eq!(inv.base_args, vec!["-S", "--rebuild"]);
        }

        #[test]
        fn known_helper_yay() {
            let inv = HelperInvocation::for_known_helper("yay");
            assert_eq!(inv.command, "yay");
            assert_eq!(inv.base_args, vec!["-S", "--rebuild"]);
        }

        #[test]
        fn known_helper_pikaur() {
            let inv = HelperInvocation::for_known_helper("pikaur");
            assert_eq!(inv.command, "pikaur");
            assert_eq!(inv.base_args, vec!["-S", "--rebuild"]);
        }

        #[test]
        fn known_helper_aura() {
            // aura uses -A instead of -S
            let inv = HelperInvocation::for_known_helper("aura");
            assert_eq!(inv.command, "aura");
            assert_eq!(inv.base_args, vec!["-A", "--rebuild"]);
        }

        #[test]
        fn known_helper_trizen() {
            let inv = HelperInvocation::for_known_helper("trizen");
            assert_eq!(inv.command, "trizen");
            assert_eq!(inv.base_args, vec!["-S", "--rebuild"]);
        }

        #[test]
        fn custom_command_simple() {
            let inv = HelperInvocation::from_custom("my-helper");
            assert_eq!(inv.command, "my-helper");
            assert!(inv.base_args.is_empty());
        }

        #[test]
        fn custom_command_with_args() {
            let inv = HelperInvocation::from_custom("my-helper -S --rebuild --custom");
            assert_eq!(inv.command, "my-helper");
            assert_eq!(inv.base_args, vec!["-S", "--rebuild", "--custom"]);
        }

        #[test]
        fn custom_command_extra_whitespace() {
            let inv = HelperInvocation::from_custom("  my-helper   -S   --rebuild  ");
            assert_eq!(inv.command, "my-helper");
            assert_eq!(inv.base_args, vec!["-S", "--rebuild"]);
        }
    }

    mod rebuild_error_display {
        use super::*;

        #[test]
        fn no_helper() {
            let err = RebuildError::NoHelper;
            let msg = err.to_string();
            assert!(msg.contains("No AUR helper detected"));
            assert!(msg.contains("paru"));
            assert!(msg.contains("yay"));
        }

        #[test]
        fn ambiguous_helper() {
            let err = RebuildError::AmbiguousHelper(vec!["paru".into(), "yay".into()]);
            let msg = err.to_string();
            assert!(msg.contains("Multiple AUR helpers found"));
            assert!(msg.contains("paru"));
            assert!(msg.contains("yay"));
        }

        #[test]
        fn helper_not_found() {
            let err = RebuildError::HelperNotFound("nonexistent".into());
            let msg = err.to_string();
            assert!(msg.contains("nonexistent"));
            assert!(msg.contains("not found"));
        }

        #[test]
        fn helper_failed() {
            let err = RebuildError::HelperFailed(1);
            let msg = err.to_string();
            assert!(msg.contains("exited with code 1"));
        }

        #[test]
        fn package_not_in_queue() {
            let err = RebuildError::PackageNotInQueue("my-pkg".into());
            let msg = err.to_string();
            assert!(msg.contains("my-pkg"));
            assert!(msg.contains("not in the queue"));
            assert!(msg.contains("-f"));
        }
    }
}
