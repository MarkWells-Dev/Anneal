# Anneal

Proactive AUR rebuild management for Arch Linux.

## Overview

Anneal monitors package upgrades and marks AUR packages that may need rebuilding when their dependencies change. Unlike reactive tools that detect broken packages after the fact, Anneal works proactively at upgrade time.

## The Problem

When official Arch packages update, AUR packages that depend on them may break due to:

- ABI changes (even when sonames stay the same)
- Plugin architecture mismatches (Qt platform themes, GTK modules)
- Language runtime version changes (Python, Ruby, Lua)

Tools like `checkrebuild` detect broken linkage after the fact, but miss ABI breaks that don't change sonames (e.g., Qt 6.7 → 6.8 keeps `libQt6Core.so.6`).

## How Anneal Helps

Anneal runs as a pacman hook during upgrades. When a tracked "trigger" package updates:

1. Detects the version change
2. Compares against configured thresholds (major/minor/patch)
3. Marks dependent AUR packages for rebuild
4. Notifies the user what needs attention

## Installation

```bash
# From AUR (coming soon)
paru -S anneal
```

## Usage

```bash
# Check status of marked packages
anneal status

# List packages marked for rebuild
anneal list

# Clear rebuild mark after rebuilding
anneal clear <package>

# Show trigger configuration
anneal triggers
```

## Configuration

Configuration lives in `/etc/anneal/config.toml` and `~/.config/anneal/config.toml`.

See [docs/DESIGN.md](docs/DESIGN.md) for architecture details and [docs/CURATED_LIST.md](docs/CURATED_LIST.md) for the list of tracked triggers.

## Documentation

- [DESIGN.md](docs/DESIGN.md) - Architecture and design decisions
- [TRIGGERING.md](docs/TRIGGERING.md) - How trigger detection works
- [CURATED_LIST.md](docs/CURATED_LIST.md) - Curated trigger packages and rationale
- [ISSUES.md](docs/ISSUES.md) - Known issues and edge cases

## Contributing

```bash
# Setup
git clone https://github.com/MarkWells-Dev/Anneal.git
cd Anneal
pre-commit install
pre-commit install --hook-type pre-push

# Install dev tools (if not already installed)
cargo install cargo-nextest cargo-deny

# Run checks
cargo fmt
cargo clippy
cargo nextest run
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) for details.
