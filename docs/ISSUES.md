# Documentation Issues

Tracking issues identified during pre-implementation review. All items should be resolved before coding begins.

## Critical

- [x] **Package Override Filtering Logic** (DESIGN.md)
  - ~~When an empty `/etc/anneal/packages/foo.conf` exists, does it:~~
    - ~~Completely prevent the package from ever being marked?~~
    - ~~Or apply no filtering rules (allowing default behavior)?~~
  - **Resolution:** An empty file means no triggers can mark that package (opt-out mechanism).

- [x] **Semver Parsing Definition** (TRIGGERING.md, DESIGN.md)
  - **Resolution:** Standard parsing: strip pkgrel (after last `-`), strip `v` prefix, parse as `X.Y.Z` or `X.Y` where components must be numeric. If parsing fails, always trigger. Curated triggers use stable versioning so non-semver fallback is rare.

- [x] **Glob Evaluation Timing** (DESIGN.md)
  - **Resolution:** Globs are evaluated at trigger fire time (dynamic). Matches against currently installed packages when the trigger fires.

## High Priority

- [x] **`-bin` Filtering Stage** (DESIGN.md, TRIGGERING.md)
  - **Resolution:** Filtered at mark time. Rebuilding a `-bin` package just re-downloads the same prebuilt binary - if it was broken, it stays broken. Running `rebuild` accomplishes nothing.

- [x] **`allow_rebuild` Config Semantics** (DESIGN.md)
  - **Resolution:** Remove this config option. `-f` flag already covers this use case.

- [x] **Checkrebuild Deduplication** (DESIGN.md)
  - **Resolution:** Displayed in both sections (we don't modify checkrebuild output), but only rebuilt once. Deduplication happens at rebuild time, not display time.

- [x] **Exit Codes for All Commands** (DESIGN.md)
  - **Resolution:** 0 = operation completed (even if empty result), 1 = error, 2 = reserved for "not found" (`ismarked`, `unmark --strict` only). User declining confirmation is 0 (valid choice, not error).

- [x] **Hook User Context** (DESIGN.md)
  - **Resolution:** Hooks only call `mark`/`trigger`, never `rebuild`. Never build as root - good AUR helpers won't allow it. User runs `rebuild` manually later.

## Moderate Priority

- [x] **Manual Mark Event Storage** (DESIGN.md)
  - **Resolution:** Yes, trigger_events row created with NULL trigger_package and timestamp. Displayed as "external" in `anneal list` - indicates it wasn't marked by Anneal's automation without assuming who did it.

- [x] **Repeated Trigger Events** (DESIGN.md)
  - **Resolution:** Each trigger creates a new event row. Queue table has one row per package (what needs rebuilding). Event history table has multiple rows (why and when). History retained for 90 days by default, configurable, 0 to disable.

- [x] **`glob_trigger_ok` Matching Semantics** (DESIGN.md)
  - **Resolution:** Remove the glob warning system entirely (`glob_threshold`, `glob_trigger_ok`, `glob_package_ok`). Users who write globs are responsible for their configs.

- [x] **Transaction Semantics** (DESIGN.md)
  - **Resolution:** SQLite WAL mode handles concurrency. Multi-table operations wrapped in transactions for atomicity. Event pruning happens as a post-transaction hook (opportunistic cleanup after any transaction).

## Low Priority

- [x] **Empty Results Output** (DESIGN.md)
  - **Resolution:** Scripting commands stay silent, human commands give feedback. `list`: "No packages in queue". `query`: silent, exit 0. `ismarked`: silent, exit 2. `triggers`: always has output.

- [x] **Configuration Default Handling** (DESIGN.md)
  - **Resolution:** Use flat conf format (key=value), not TOML. No sections. Missing keys use defaults. Missing file uses all defaults.

- [x] **Shell Completion Paths** (DESIGN.md)
  - **Resolution:** Paths verified correct for Arch Linux.

- [x] **Helper Config Edge Cases** (DESIGN.md)
  - **Resolution:** Just `helper` key. Known helpers (paru, yay, etc.) use built-in invocation. Custom commands have packages appended. If positional args needed, wrap in a script. Validation at rebuild time.

## Minor/Typos

- [x] **Leading space before `icu`** (TRIGGERING.md line 110)
  - **Resolution:** No issue - file is clean. False positive from initial review.
