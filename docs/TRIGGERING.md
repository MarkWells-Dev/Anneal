# Triggering Mechanism Design

## Problem Statement

When a package updates, how do we determine which installed packages need rebuilding?

### Why Packages Need Rebuilding

1. **ABI breaks** - Package links against a shared library whose ABI changed
2. **Plugin architectures** - Plugins (e.g., Qt platform themes) loaded via `dlopen()` at runtime
3. **Header/inline changes** - Code compiled against old headers with different struct layouts

## Target Packages

Anneal targets a specific subset of installed packages:

### What Anneal Tracks

- **AUR packages built from source** - Where rebuilding produces a new binary compiled against current dependencies

### What Anneal Ignores

| Category               | Why ignore                                                |
| ---------------------- | --------------------------------------------------------- |
| Official repo packages | Maintainers handle rebuilds via pkgrel bumps              |
| `*-bin` packages       | Rebuilding re-downloads same binary, doesn't fix anything |
| Precompiled blobs      | Same as `-bin`, rebuilding is meaningless                 |

**Note on `-bin` suffix:** Per AUR submission guidelines, packages using prebuilt deliverables when sources are available _must_ use the `-bin` suffix (exception: Java). This makes filtering reliable.

## Analysis

### `checkrebuild` Limitations

`checkrebuild` inspects `ldd`, `python`, `perl`, `ruby`, and `haskell` dependencies. It would **not** catch:

- Qt6 breaking qt6gtk2 (soname stays `libQt6Core.so.6` across 6.x releases)
- Any ABI break that doesn't change the soname
- Plugin loading via `dlopen()`

This makes it useful but insufficient as a sole mechanism.

### Why Not `makedepends`?

Initial design explored using `makedepends` from PKGBUILDs to auto-discover triggers. This approach failed because:

1. **`makedepends` contains build tools, not libraries** - npm, cargo, meson, cmake are in makedepends
2. **ABI-sensitive libraries go in `depends`** - If a package links against Qt6, qt6-base is in `depends` (needed at runtime), not `makedepends`

Example from qt6gtk2's PKGBUILD:

```bash
depends=(qt6-base gtk2 libx11)  # Libraries it links against
# No makedepends - build deps are implicit from depends
```

### Why `depends` Works

If a package has `depends=(qt6-base)`, it means:

- The package directly needs qt6-base at runtime
- It almost certainly links against Qt6 libraries
- When Qt6's ABI changes, the package needs rebuilding

We can query reverse dependencies using pacman's own data:

```bash
pactree -r -u qt6-base  # All packages depending on qt6-base
```

Then filter to AUR packages:

```bash
comm -12 <(pactree -r -u qt6-base | sort -u) <(pacman -Qm | cut -d' ' -f1 | sort)
```

## Proposed Direction

**Curated Trigger List + Reverse Dependency Lookup**

Anneal ships with a curated list of ABI-sensitive "trigger" packages. When one upgrades:

1. Query reverse dependencies via `pactree -r -u <trigger>`
2. Filter to AUR packages only (`pacman -Qm`)
3. Filter out `-bin` packages
4. Apply version threshold
5. Mark for rebuild

### Why This Works

- **No PKGBUILD parsing** - Uses pacman's own dependency data
- **No caching needed** - Query at trigger time
- **No bootstrapping** - Works immediately after install
- **Manageable maintenance** - Trigger list is small (~20-50 packages)
- **Low false positives** - Only triggers on known ABI-sensitive packages

### Curated Trigger List

Anneal ships with a list of known ABI-sensitive packages:

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

This list is:

- Embedded in the binary (no external file to lose)
- User-customizable via `/etc/anneal/triggers/<trigger>.conf`
- Community-maintained (PRs welcome to add triggers)

### User Overrides

All customization via `/etc/anneal/triggers/<trigger>.conf` and `/etc/anneal/packages/<package>.conf`.

**Override/disable/add triggers** (`/etc/anneal/triggers/<trigger>.conf`):

```bash
# Override what qt6-base marks
echo -e "qt6gtk2\nmy-qt-app" | sudo tee /etc/anneal/triggers/qt6-base.conf

# Disable a shipped trigger (empty file)
sudo touch /etc/anneal/triggers/qt5-base.conf

# Add custom trigger for my-lib
echo -e "my-app\nmy-*" | sudo tee /etc/anneal/triggers/my-lib.conf
```

**Override package behavior** (`/etc/anneal/packages/<package>.conf`):

```bash
# Mark bleeding-edge when ANY package upgrades
echo "*" | sudo tee /etc/anneal/packages/bleeding-edge.conf

# Never mark this package (empty file)
sudo touch /etc/anneal/packages/stable-pkg.conf
```

### Glob Patterns

Globs match against installed AUR packages:

- `ultra-*` → all packages starting with "ultra-"
- `*` → all AUR packages (nuclear option)

Users who write globs are responsible for matching the intended packages.

### Package Maintainer Opt-In

Maintainers can ship hooks that call `anneal mark` directly:

```bash
if command -v anneal &>/dev/null; then
    anneal mark my-package --trigger some-dep "$VERSION"
fi
```

This is the escape hatch for packages with unusual trigger requirements.

## Version Thresholds

**Decision:** Default threshold is `minor` - only trigger on major/minor version changes, not patch releases.

**Logic:**

```
if version parses as semver:
    trigger if major or minor changed (not patch)
else:
    always trigger (conservative fallback)
```

**Examples:**
| Old Version | New Version | Triggers? | Reason |
|-------------|-------------|-----------|--------|
| `6.6.1-1` | `6.6.2-1` | No | Patch bump only |
| `6.6.1-1` | `6.7.0-1` | Yes | Minor bump |
| `6.6.1-1` | `7.0.0-1` | Yes | Major bump |
| `1.0.r123.abc-1` | `1.0.r124.def-1` | Yes | Can't parse as semver |
| `2024.01.15-1` | `2024.01.16-1` | Yes | Can't parse as semver |

**Rationale:**

- Patch releases typically maintain ABI compatibility
- Major/minor releases more likely to break ABI
- Non-semver versions (VCS packages, date-based) can't be reasoned about, so trigger conservatively

**Configuration:**

```conf
# /etc/anneal/config.conf
version_threshold = minor  # Options: major, minor, patch, always
```

- `major` - Only trigger on major version changes (risky)
- `minor` - Trigger on major or minor changes (default)
- `patch` - Trigger on any version change including patch
- `always` - Always trigger regardless of version (most conservative)

Non-semver versions always trigger regardless of this setting.

## Trigger Flow

```
qt6-base upgrades from 6.6.1 to 6.7.0
                │
                ▼
    ┌───────────────────────┐
    │ Version threshold     │
    │ 6.6 → 6.7 = minor     │
    │ Threshold: minor ✓    │
    └───────────┬───────────┘
                │
                ▼
    ┌───────────────────────┐
    │ pactree -r -u qt6-base│
    │ Get reverse deps      │
    └───────────┬───────────┘
                │
                ▼
    ┌───────────────────────┐
    │ Filter: AUR only      │
    │ (pacman -Qm)          │
    └───────────┬───────────┘
                │
                ▼
    ┌───────────────────────┐
    │ Filter: exclude -bin  │
    └───────────┬───────────┘
                │
                ▼
    ┌───────────────────────┐
    │ Mark for rebuild:     │
    │ - qt6gtk2             │
    │ - hyprqt6engine       │
    │ - yin-yang            │
    └───────────────────────┘
```

## checkrebuild Integration

### Why Not Mark?

Anneal's queue exists to capture _transient events_ (dependency upgrades) and persist them until acted on. Without marking, the information is lost after the transaction.

`checkrebuild` is different - it queries _current state_. Broken linkage is detectable anytime. There's no event to capture; the breakage persists until fixed.

**Therefore:**

- Anneal queue = persisted events (trigger upgrades)
- checkrebuild = live query (broken linkage right now)

Marking checkrebuild results would create duplicate, potentially stale state.

### Integration Design

checkrebuild is an **optional dependency**:

```
optdepends=('rebuild-detector: detect packages with broken shared library links')
```

At rebuild time, Anneal can include checkrebuild results:

```bash
anneal rebuild                   # Rebuild marked packages only
anneal rebuild --checkrebuild    # Rebuild marked + checkrebuild results
```

Config option to make this the default:

```conf
# /etc/anneal/config.conf
include_checkrebuild = true
```

### Output Example

```
[anneal] Packages marked for rebuild:
  qt6gtk2 (qt6-base 6.7.0)
  qt6ct (qt6-base 6.7.0)

[anneal] Packages with broken linkage (via checkrebuild):
  lib32-mesa

Rebuild 3 packages? [y/N]
```

### Complementary Coverage

| Tool                   | Detects                              | Example                      |
| ---------------------- | ------------------------------------ | ---------------------------- |
| Anneal (reverse deps)  | ABI breaks with same soname, plugins | qt6-base breaking qt6gtk2    |
| checkrebuild (linkage) | Missing/changed sonames              | lib update breaking lib32-\* |

Together they provide comprehensive rebuild detection.

## Design Decisions Summary

| Question               | Decision                                         |
| ---------------------- | ------------------------------------------------ |
| Trigger source         | Curated list + reverse dependency lookup         |
| Data source            | `pactree -r` (pacman's dependency data)          |
| Caching                | None needed - query at trigger time              |
| Official repo packages | Ignore                                           |
| `-bin` packages        | Ignore                                           |
| Version thresholds     | Default `minor` (major/minor trigger, not patch) |
| Non-semver versions    | Always trigger                                   |
| checkrebuild           | Live query at rebuild time, not marks            |

## References

- [checkrebuild](https://github.com/archlinux/contrib/blob/main/src/checkrebuild.in) - Arch tool for detecting packages linked against outdated libraries
- [rebuild-detector](https://github.com/maximbaz/rebuild-detector) - Similar tool with additional checks
- [pactree](https://man.archlinux.org/man/pactree.8) - Pacman tool for viewing dependency trees
