# Anneal Implementation TODO

## Core Commands

- [x] `mark` - Add packages to rebuild queue
- [x] `unmark` - Remove packages from queue
- [x] `list` - Show current queue
- [x] `clear` - Reset queue
- [x] `ismarked` - Check if package is marked
- [x] `query` - Query packages in queue
- [x] `triggers` - List configured triggers
- [x] `config` - Dump configuration
- [x] `rebuild` - Invoke AUR helper to rebuild packages
- [x] `trigger` - Process triggers from upgraded packages

## Rebuild Command

- [x] AUR helper detection (paru, yay, pikaur, aura, trizen)
- [x] Known helper invocation (`paru -S --rebuild <pkg>...`)
- [x] Custom helper command support
- [x] `--checkrebuild` integration (rebuild-detector package)
- [x] Confirmation prompt before rebuilding
- [x] Remove packages from queue after successful rebuild

## Trigger Command

- [x] Filter to curated trigger list
- [x] Query reverse dependencies via `pactree -r -u <trigger>`
- [x] Filter to AUR packages only (`pacman -Qm`)
- [x] Filter out `-bin` packages
- [x] Version threshold checking (via `pkg:oldver:newver` format)
- [x] Mark resulting packages
- [x] `--dry-run` mode

## User Override System

- [ ] Load trigger overrides from `/etc/anneal/triggers/*.conf`
- [ ] Load package overrides from `/etc/anneal/packages/*.conf`
- [ ] Glob pattern matching for trigger targets
- [ ] Empty file = disable trigger / never mark package

## Pacman Hooks

- [ ] Create `anneal-trigger.hook` for PostTransaction
- [ ] Hook invokes `anneal trigger` with upgraded packages
- [ ] Install hook to `/usr/share/libalpm/hooks/`

## Shell Completions

- [ ] Bash completions
- [ ] Zsh completions
- [ ] Fish completions

## Polish

- [ ] Colorized output (TTY detection)
- [ ] Match pacman output style
- [ ] Event history pruning (use config.retention_days)
- [ ] Proper error messages for all edge cases

## Packaging

- [ ] PKGBUILD for AUR
- [ ] Systemd timer for periodic `anneal rebuild` (optional)
