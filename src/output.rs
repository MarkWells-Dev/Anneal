// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Mark Wells Dev

//! Colorized terminal output in pacman style.
//!
//! Provides formatted output that matches pacman's visual style:
//! - `::` headers in bold blue
//! - `->` status messages in bold blue
//! - Package names in bold white
//! - Warnings in yellow
//! - Errors in red
//!
//! Colors are automatically disabled when stdout/stderr is not a TTY.

use std::io::{self, IsTerminal, Write};

use owo_colors::OwoColorize;

/// Check if stdout supports colors.
fn stdout_supports_color() -> bool {
    io::stdout().is_terminal()
}

/// Check if stderr supports colors.
fn stderr_supports_color() -> bool {
    io::stderr().is_terminal()
}

/// Print a header line in pacman style.
///
/// Format: `:: <message>`
pub fn header(msg: &str) {
    if stdout_supports_color() {
        println!("{} {}", "::".bold().blue(), msg.bold());
    } else {
        println!(":: {msg}");
    }
}

/// Print a status/action line in pacman style.
///
/// Format: `-> <message>`
pub fn status(msg: &str) {
    if stdout_supports_color() {
        println!("{} {msg}", "->".bold().blue());
    } else {
        println!("-> {msg}");
    }
}

/// Print a package name (bold white).
pub fn package(name: &str) {
    if stdout_supports_color() {
        println!("{}", name.bold().white());
    } else {
        println!("{name}");
    }
}

/// Print a package with trigger info.
pub fn package_with_trigger(name: &str, trigger: &str) {
    if stdout_supports_color() {
        println!("{} ({trigger})", name.bold().white());
    } else {
        println!("{name} ({trigger})");
    }
}

/// Print a warning message to stderr.
///
/// Format: `warning: <message>`
pub fn warning(msg: &str) {
    if stderr_supports_color() {
        eprintln!("{} {msg}", "warning:".yellow());
    } else {
        eprintln!("warning: {msg}");
    }
}

/// Print an error message to stderr.
///
/// Format: `error: <message>`
pub fn error(msg: &str) {
    if stderr_supports_color() {
        eprintln!("{} {msg}", "error:".bold().red());
    } else {
        eprintln!("error: {msg}");
    }
}

/// Print a success count message.
///
/// Format: `-> <action> <count> package(s)`
pub fn success_count(action: &str, count: usize) {
    let pkg_word = if count == 1 { "package" } else { "packages" };
    if stdout_supports_color() {
        println!(
            "{} {action} {} {pkg_word}",
            "->".bold().blue(),
            count.bold().green()
        );
    } else {
        println!("-> {action} {count} {pkg_word}");
    }
}

/// Print an info message to stderr (for progress/status).
pub fn info(msg: &str) {
    if stderr_supports_color() {
        eprintln!("{} {msg}", "->".bold().blue());
    } else {
        eprintln!("-> {msg}");
    }
}

/// Flush stdout.
pub fn flush() {
    let _ = io::stdout().flush();
}
