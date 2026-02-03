# Anneal Design Document

## Problem Statement

When certain Arch Linux packages update (e.g., `qt6`), dependent AUR packages (e.g., `qt6gtk2`) may need to be rebuilt. Currently, package maintainers handle this by including pacman hooks that emit warnings like:

> Warning: qt6 apps may not work until qt6gtk2 is rebuilt

This approach is brittle:

- Warnings scroll by and are easily missed
- No persistent record of what needs attention
- No integration with tooling
- Each package reinvents the same pattern

## Solution Overview

Anneal provides a structured system for tracking packages that need rebuilding:

1. A **queue** of packages marked as needing rebuild
2. A **CLI tool** for managing the queue
3. A **curated trigger list** of ABI-sensitive packages (Qt, GTK, etc.)
4. A **pacman hook** that automatically marks packages when triggers update

## CLI Interface

```
anneal mark <pkg>... [--trigger <trigger> [version]]  # Add packages to queue
anneal unmark [--strict] [pkg]...  # Remove packages from queue (stdin if no args)
anneal list                     # Show the current queue
anneal clear [-f] [trigger]     # Reset queue, or clear events by trigger
anneal rebuild [-f] [--checkrebuild] [pkg]...  # Rebuild queued packages
anneal ismarked <pkg>           # Check if package is marked (exit 0=yes, 1=no)
anneal query <pkg>...           # Print which of the given packages are in queue
anneal triggers                 # List configured triggers
anneal trigger [--dry-run] [pkg]...  # Process triggers (stdin if no args)
anneal config                   # Dump current configuration
anneal -h, --help               # Show help
anneal -V, --version            # Show version and trigger list version
```

**Commands requiring root** (modify queue or system state):

- `mark`, `unmark`, `clear`, `trigger`

**Commands not requiring root** (read-only):

- `list`, `ismarked`, `query`, `triggers`, `config`, `--help`, `--version`

**Special case**:

- `rebuild` - Does not require root itself, but invokes the AUR helper which handles sudo elevation for the install step (building occurs as the invoking user)

When a command requires root but is run without it:

```
[anneal] error: Permission denied. This command requires root privileges.
```

### Output Styling

All commands use consistent styling that matches pacman's output style for seamless integration during transaction hooks.

**Colorized output** (when stdout is a TTY):

- Follows pacman's color conventions (bold white for emphasis, green for success, yellow for warnings, red for errors)
- Example: `[anneal]` prefix in bold, package names in white, trigger info in dim

**Plain text fallback** (when piping or capturing stdout):

- Colors disabled automatically via TTY detection
- Clean output suitable for parsing or logging

This ensures anneal output looks native alongside pacman's hook messages while remaining scriptable.

### Global Flags

```
anneal --quiet <command>        # Suppress stdout (errors still go to stderr)
```

The `--quiet` flag works with any command to suppress normal output while still reporting errors.

Note: `--quiet` does not imply `-f`. If a command requires confirmation and `--quiet` is set without `-f`:

```
[anneal] error: Cannot prompt for confirmation with --quiet. Use -f to force.
```

### Exit Codes

All commands return sensible exit codes for scripting:

| Code | Meaning                                                  |
| ---- | -------------------------------------------------------- |
| 0    | Success (operation completed, even if result is empty)   |
| 1    | General error (invalid args, file errors, etc.)          |
| 2    | Package not found (for `ismarked`, or `unmark --strict`) |

Specific behaviors:

- `anneal list` - Returns 0 (empty queue is valid result)
- `anneal query` - Returns 0 (no matches is valid result, silent output)
- `anneal ismarked` - Returns 0 if in queue, 2 if not (silent output)
- `anneal unmark` - Returns 0 even if package wasn't in queue (idempotent)
- `anneal unmark --strict` - Returns 2 if any package wasn't in queue
- `anneal rebuild` - Attempts all packages, returns non-zero if any failed
- `anneal clear` - Returns 0 whether user confirms or declines
- `anneal triggers` - Returns 0 (always has output)
- `anneal mark` - Returns 0 on success, 1 on error
- `anneal trigger` - Returns 0 on success, 1 on error

### Shell Completions

Shell completions for bash, zsh, and fish are generated at build time and installed with the package.

## Storage

### Database Location

```
/var/lib/anneal/anneal.db
```

SQLite database storing the rebuild queue.

#### Schema

```sql
-- Packages currently marked for rebuild
CREATE TABLE queue (
    package TEXT PRIMARY KEY,
    first_marked_at TEXT NOT NULL  -- ISO8601 timestamp
);

-- Trigger event history (persists after unmark for debugging)
CREATE TABLE trigger_events (
    id INTEGER PRIMARY KEY,
    package TEXT NOT NULL,
    trigger_package TEXT,      -- NULL for external marks (no --trigger provided)
    trigger_version TEXT,      -- NULL if not provided
    marked_at TEXT NOT NULL    -- ISO8601 timestamp
);

CREATE INDEX idx_trigger_events_package ON trigger_events(package);
CREATE INDEX idx_trigger_events_trigger ON trigger_events(trigger_package);
CREATE INDEX idx_trigger_events_marked_at ON trigger_events(marked_at);
```

Events are retained for 90 days (configurable via `retention_days`, 0 to disable). Old events are pruned as a post-transaction hook after any database operation. This provides history for debugging without unbounded growth.

#### Why SQLite

- **Concurrent access**: WAL mode handles simultaneous hooks
- **Atomic transactions**: No corruption on failure
- **Indexed queries**: Fast lookups by package
- **Single file**: Simple deployment and backup

#### Permissions

The database is owned by `root:root` with mode `0644`:

- **World-readable**: Any user can query (for `anneal list`, `ismarked`, `query`)
- **Root-writable**: Only root can modify

#### Inspecting the Database

```bash
sqlite3 /var/lib/anneal/anneal.db "SELECT * FROM queue"
sqlite3 /var/lib/anneal/anneal.db "SELECT * FROM trigger_events"
```

### Curated Trigger List

Anneal ships with a curated list of ABI-sensitive packages that are known to break dependent packages when updated.

#### Shipped Triggers

```
# Qt ecosystem
qt5-base
qt6-base

# GTK ecosystem
gtk2
gtk3
gtk4

# Other common triggers
electron
boost
icu
openssl
```

This list is embedded in the binary and community-maintained via PRs. The list has a version number that increments with each change, displayed in `anneal --version`:

```
anneal 0.1.0 (triggers v3)
```

#### How It Works

When a trigger package upgrades:

1. Check version threshold (default: major/minor changes only)
2. Query reverse dependencies via `pactree -r -u <trigger>`
3. Filter to AUR packages only (`pacman -Qm`)
4. Filter out `-bin` packages (rebuilding just re-downloads the same binary - pointless)
5. Mark remaining packages for rebuild

No caching or bootstrapping required - uses pacman's own dependency data at trigger time.

### User Overrides

#### Trigger Overrides

All trigger customization uses `/etc/anneal/triggers/<trigger>.conf`:

**Override what packages a trigger marks:**

```
# /etc/anneal/triggers/ultra-lib.conf
# When ultra-lib upgrades, mark these (instead of pactree discovery):
ultra-pkg1
ultra-pkg2
ultra-*        # Glob: any installed AUR package matching ultra-*
```

**Disable a shipped trigger:**

```bash
# Empty file = trigger marks nothing
sudo touch /etc/anneal/triggers/qt5-base.conf
```

**Add a custom trigger:**

```bash
# Create config for a package not in shipped list
echo -e "my-app\nmy-other-app" | sudo tee /etc/anneal/triggers/my-lib.conf
```

#### Package Overrides

**Override what triggers mark a package** (`/etc/anneal/packages/<package>.conf`):

```
# /etc/anneal/packages/my-qt-app.conf
# Only mark when these specific triggers upgrade:
qt6-base
```

```
# /etc/anneal/packages/bleeding-edge.conf
# Mark when ANY package upgrades (nuclear option):
*
```

Empty file = package is never marked.

#### Glob Patterns

Globs are evaluated at trigger time against currently installed AUR packages (`pacman -Qm`), excluding `-bin`. This is dynamic - newly installed packages will be matched on subsequent triggers.

| Pattern   | Matches                       |
| --------- | ----------------------------- |
| `ultra-*` | ultra-pkg1, ultra-tools, etc. |
| `*-git`   | All VCS packages              |
| `*`       | All AUR packages              |

#### Override Precedence

1. `/etc/anneal/packages/<pkg>.conf` - if exists, controls what marks this package
2. `/etc/anneal/triggers/<trigger>.conf` - if exists, controls what this trigger marks
3. Default: pactree reverse dependency lookup

All files use line-delimited format with `#` comments.

## Pacman Hooks

### Upgrade Hook

Installed to `/usr/share/libalpm/hooks/anneal-upgrade.hook`:

```ini
[Trigger]
Operation = Upgrade
Type = Package
Target = *

[Action]
Description = Checking for packages needing rebuild...
When = PostTransaction
NeedsTargets
Exec = /usr/bin/anneal trigger
```

The `trigger` subcommand:

1. Reads upgraded packages from stdin (one per line)
2. Filters to packages in the curated trigger list (+ user additions from `/etc/anneal/triggers.conf`)
3. For each trigger, checks version threshold (default: major/minor changes only)
4. Queries reverse dependencies via `pactree -r -u <trigger>`
5. Filters to AUR packages only (`pacman -Qm`)
6. Filters out `-bin` packages and packages with override files in `/etc/anneal/packages/`
7. Marks remaining packages in the queue

Use `--dry-run` to see what would be marked without modifying the queue:

```bash
anneal trigger --dry-run qt6-base
echo "qt6-base" | anneal trigger --dry-run
```

Dry-run output uses "would" to indicate hypothetical:

```
[anneal] would mark qt6gtk2 (qt6-base 6.7.0)
[anneal] would mark hyprqt6engine (qt6-base 6.7.0)
```

Dry-run still queries versions and loads user overrides to show realistic results.

### Remove Hook

Installed to `/usr/share/libalpm/hooks/anneal-remove.hook`:

```ini
[Trigger]
Operation = Remove
Type = Package
Target = *

[Action]
Description = Cleaning up removed packages from rebuild queue...
When = PostTransaction
NeedsTargets
Exec = /usr/bin/anneal unmark
```

When called without arguments, `anneal unmark` reads package names from stdin (one per line). This automatically removes uninstalled packages from the rebuild queue.

Note: If a trigger package (e.g., `qt6`) and its dependent (e.g., `qt6gtk2`) are both upgraded in the same transaction, the dependent is still marked. This is intentional - the dependent package was built _before_ the transaction started, meaning it was built against the old version of its dependency. It still needs a rebuild against the new version.

Unmarking happens via:

- `anneal rebuild` - automatically unmarks packages after receiving a clean exit code from the AUR helper (confirming successful build and install)
- `anneal unmark` - manual removal when the user knows a rebuild is unnecessary

## Queue Operations

### Marking

```
anneal mark qt6gtk2
anneal mark qt6gtk2 --trigger qt6-base
anneal mark qt6gtk2 --trigger qt6-base 6.6.1-1
```

1. Validate and normalize package name:
   - Lowercase alphanumerics and `@._+-`
   - Cannot start with hyphen or dot
   - Uppercase input normalized to lowercase
2. Add trigger event to package's trigger array (or create entry if new)
3. If `--trigger` is provided, record it; otherwise record as manual mark
4. If version is provided (positional after trigger), record it; otherwise omit

Output (suitable for pacman hooks):

```
[anneal] qt6gtk2 marked (qt6-base 6.6.1-1)
[anneal] qt6ct marked (qt6-base 6.6.1-1)
```

With trigger but no version:

```
[anneal] qt6gtk2 marked (qt6-base)
```

Marks without `--trigger` (displayed as "external" in `anneal list` since we can't assume it was manual - could be a third-party hook):

```
[anneal] qt6gtk2 marked
```

### Unmarking

```
anneal unmark qt6gtk2
anneal unmark qt6gtk2 qt6ct
echo -e "qt6gtk2\nqt6ct" | anneal unmark
```

When called with arguments, removes those packages from the queue. When called without arguments, reads package names from stdin (one per line). This allows the remove hook to pipe uninstalled packages directly.

Stdin parsing (matches trigger file format):

- Lines are trimmed of leading/trailing whitespace
- Empty/blank lines are skipped
- Lines starting with `#` are skipped (comments)

By default, `unmark` is idempotent - it returns 0 even if packages weren't in the queue. Use `--strict` to return non-zero if any package wasn't found.

1. Read queue
2. Remove matching entries (silently skip missing unless `--strict`)
3. Write queue

### Listing

```
anneal list
```

Output format:

```
Packages needing rebuild:
  qt6gtk2
    qt6-base 6.6.1-1 (2024-01-15)
    qt6-base 6.6.2-1 (2024-01-20)
  qt6ct
    qt6-base 6.6.1-1 (2024-01-15)
  my-custom-pkg
    external (2024-01-18)

3 packages in queue
```

Or if empty:

```
No packages in queue
```

### Clearing

```
anneal clear
```

Prompts for confirmation, showing affected packages:

```
Clear all packages from rebuild queue? [y/N]
  qt6gtk2
  qt6ct
  qt6-svg
  qt6-tools
  +3 more
```

Package list display logic:

- N ≤ 5: show all packages
- N > 5: show first 4, then "+N more" (avoids awkward "+1 more")

Use `anneal clear -f` to skip confirmation. This is intentionally not configurable - clearing should always be explicit.

```
anneal clear qt6-base
```

Removes all trigger events for `qt6-base` from the queue. If a package has no remaining triggers after removal, it is removed from the queue entirely.

```
Remove qt6-base triggers? [y/N]
  qt6gtk2
  qt6ct
  qt6-svg
```

Example: If `qt6gtk2` was triggered by both `qt6-base` and `qt6-declarative`:

- `anneal clear qt6-base` removes the qt6-base events
- `qt6gtk2` stays in queue with the qt6-declarative trigger(s)
- If `qt6-declarative` events are also cleared, `qt6gtk2` is removed entirely

Use `anneal clear -f <trigger>` to skip confirmation.

### Querying

```
anneal ismarked qt6gtk2
```

Exit code 0 if package is in queue, 1 if not. Useful for scripting.

```
anneal query qt6gtk2 qt6ct python-foo
```

Prints the names of packages that are in the queue (one per line). Only outputs packages that match, useful for filtering.

### Rebuilding

```
anneal rebuild [-f] [--checkrebuild] [--cmd <helper>] [pkg]... [-- <helper-args>...]
```

Invokes an AUR helper to rebuild packages. If no packages are specified, rebuilds all queued packages.

If specific packages are provided that aren't in the queue:

- Default: warn per package and skip it, continue with others
  ```
  [anneal] warning: foo is not in the rebuild queue, skipping
  ```
- With `-f`: rebuild anyway without warning

**checkrebuild integration:**

With `--checkrebuild` (or `include_checkrebuild = true` in config), Anneal also includes packages detected by `checkrebuild` (from `rebuild-detector` package). This catches packages with broken shared library linkage.

Output with both sources:

```
[anneal] Packages marked for rebuild:
  qt6gtk2 (qt6-base 6.7.0)

[anneal] Packages with broken linkage (via checkrebuild):
  lib32-mesa

Rebuild 2 packages? [y/N]
```

If a package appears in both the Anneal queue and checkrebuild output, it is shown in both sections (checkrebuild output is not modified) but only rebuilt once. Deduplication happens at rebuild time, not display time.

Examples:

```bash
anneal rebuild                        # Rebuild all queued packages
anneal rebuild --checkrebuild         # Include checkrebuild results
anneal rebuild qt6gtk2                # Rebuild specific package (must be in queue)
anneal rebuild -f qt6gtk2             # Rebuild even if not in queue
anneal rebuild --cmd yay              # Use yay instead of configured default
anneal rebuild -- --noconfirm         # Pass args to the helper
```

The helper is invoked based on configuration (see Helper Configuration Formats below). Additional arguments passed after `--` are appended to the command.

Packages are only unmarked after the AUR helper returns exit code 0, confirming successful build and install. This is the only way Anneal can validate that a rebuild actually occurred against the current dependencies.

Note: AUR helpers handle sudo elevation themselves - they build as the invoking user and only elevate for the install step. Anneal does not manage sudo credentials.

### Configuration

```
/etc/anneal/config.conf
```

Flat key=value format (no sections):

```conf
version_threshold = minor
helper = paru
include_checkrebuild = false
retention_days = 90
```

The config file is **optional**. If missing, anneal uses sensible defaults. Missing keys in an existing file also use defaults:

- `version_threshold`: `minor` (trigger on major/minor changes, not patch)
- `helper`: auto-detected from PATH (see AUR Helper Detection below)
- `include_checkrebuild`: `false` (set to `true` to always include checkrebuild results)
- `retention_days`: `90` (days to keep event history after unmark, 0 to disable)

**Version threshold options:**

- `major` - Only trigger on major version changes (risky)
- `minor` - Trigger on major or minor changes (default)
- `patch` - Trigger on any version change including patch
- `always` - Always trigger regardless of version

**Semver parsing:** Strip pkgrel (after last `-`), strip `v` prefix if present, parse as `X.Y.Z` or `X.Y` where components must be numeric. Non-semver versions (e.g., `-git`/`-svn` packages) always trigger regardless of threshold.

To generate a config file with current settings:

```bash
anneal config | sudo tee /etc/anneal/config.conf
```

If no helper is detected, the output comments out the helper line:

```conf
version_threshold = minor
# helper =
include_checkrebuild = false
retention_days = 90
```

#### AUR Helper Detection

If no helper is configured:

1. Anneal checks PATH for known helpers: `paru`, `yay`, `pikaur`, `aura`, `trizen`
2. If exactly one is found, it's used automatically
3. If multiple are found, Anneal errors and lists them for the user to choose
4. If none are found:
   ```
   [anneal] error: No AUR helper detected. Set 'helper' in /etc/anneal/config.conf
   [anneal] Supported helpers: paru, yay, pikaur, aura, trizen
   ```

#### Helper Configuration Formats

**Known helper:**

```conf
helper = paru
```

Anneal uses built-in flags for rebuild: `paru -S --rebuild <pkg>...`

**Custom command:**

```conf
helper = my-helper -S --rebuild
```

Packages appended to command: `my-helper -S --rebuild <pkg>...`

If a custom helper requires positional arguments (packages not at end), wrap it in a script.

Helper validation happens at rebuild time, not config load. If the helper doesn't exist or fails, the error is reported then.

Known helpers and their built-in invocations:
| Helper | Invocation |
|--------|------------|
| paru | `paru -S --rebuild <pkg>...` |
| yay | `yay -S --rebuild <pkg>...` |
| pikaur | `pikaur -S --rebuild <pkg>...` |
| aura | `aura -A --rebuild <pkg>...` |
| trizen | `trizen -S --rebuild <pkg>...` |

## Third-Party Integration

### For Package Maintainers

Packages can check for Anneal and use it in their hooks:

```bash
if command -v anneal &>/dev/null; then
    anneal mark qt6gtk2
else
    echo "Warning: qt6 apps may not work until qt6gtk2 is rebuilt"
fi
```

If a package needs custom trigger logic (e.g., only on major version bumps), they can:

1. Maintain their own pacman hook with the custom logic
2. Call `anneal mark` from that hook
3. Submit a PR to remove their source trigger file (e.g., `packages/qt6gtk2`) from Anneal's repository

This keeps Anneal as the single source of truth for tracking, while letting maintainers own their trigger conditions.

#### Example Custom Hook

A package maintaining its own trigger logic would include a hook like:

```ini
# /usr/share/libalpm/hooks/qt6gtk2-rebuild.hook
[Trigger]
Type = Package
Operation = Upgrade
Target = qt6-base

[Action]
Description = Checking if qt6gtk2 needs rebuild...
When = PostTransaction
Exec = /usr/share/qt6gtk2/check-rebuild.sh
```

```bash
#!/bin/bash
# /usr/share/qt6gtk2/check-rebuild.sh

# Custom logic here (version checks, etc.)
QT_VERSION=$(pacman -Q qt6-base | awk '{print $2}')

# Pacman hooks run as root, so no sudo needed
if command -v anneal &>/dev/null; then
    anneal mark qt6gtk2 --trigger qt6-base "$QT_VERSION"
else
    echo "Warning: qt6 apps may not work until qt6gtk2 is rebuilt"
fi
```

Note: Pacman runs as root, so hooks execute with root privileges. No `sudo` is needed within hook scripts. Hooks should only call `mark` or `trigger`, never `rebuild` - good AUR helpers refuse to build as root.

### For AUR Helpers

Future integration could allow helpers to:

- Display queue contents after transactions
- Offer to rebuild queued packages automatically
- Clear entries after successful rebuilds

## Package Contents

The `anneal` AUR package would install:

```
/usr/bin/anneal                               # CLI tool (trigger list embedded)
/usr/share/libalpm/hooks/anneal-upgrade.hook  # Marks packages on dependency upgrades
/usr/share/libalpm/hooks/anneal-remove.hook   # Cleans queue when packages uninstalled
/usr/share/bash-completion/completions/anneal # Bash completions
/usr/share/zsh/site-functions/_anneal         # Zsh completions
/usr/share/fish/vendor_completions.d/anneal.fish  # Fish completions
/var/lib/anneal/                              # Data directory (created by package)
```

**Optional dependencies:**

```
optdepends=('rebuild-detector: detect packages with broken shared library links')
```

Note: `/var/lib/anneal/anneal.db` (SQLite database) is not included in the package - it is created on first write and updated on package operations. This file is machine-specific state.

## Build Process

Standard Rust build process. The curated trigger list is embedded at compile time.

## Performance

SQLite handles all expected usage scenarios:

- Indexed lookups for package queries
- Concurrent access via WAL mode
- Atomic transactions for queue modifications

Typical systems have 50-100 AUR packages. SQLite comfortably handles thousands.

## License

Anneal is licensed under the GNU General Public License v3.0 (GPLv3).

This license is appropriate for:

- A community-focused Arch Linux utility
- A project that expects contributions to the trigger database
- Integration with the AUR ecosystem
