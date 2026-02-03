# Curated Trigger List

This document defines the curated triggers shipped with Anneal, including their versioning schemes and threshold behavior.

## Guiding Philosophy

### Selection Criteria

A package belongs in the curated trigger list if:

1. **ABI-sensitive** - Updates can break dependent packages through ABI changes, struct layout changes, or plugin incompatibility
2. **Has AUR dependents** - AUR packages exist that link against or depend on this package
3. **Not reliably caught by soname** - Ideally, the package maintains soname stability across ABI-breaking changes (e.g., `libQt6Core.so.6` stays the same across Qt 6.x releases)

### On False Positives

**Over-marking is acceptable; under-marking is not.**

- If we mark a package unnecessarily, the user rebuilds something they didn't need to. Minor inconvenience.
- If we fail to mark a package that needed rebuilding, the user has a broken system. Unacceptable.

When in doubt, include the trigger with an appropriate threshold.

### Overlap with checkrebuild

Some triggers in this list will also be detected by `checkrebuild` / `rebuild-detector` when sonames change. This overlap is intentional:

- Provides verification that Anneal's marking is working correctly
- Catches breaks earlier (at upgrade time vs. when user runs checkrebuild)
- checkrebuild is an optional dependency; Anneal should work standalone

### Threshold Selection

Choose the most conservative threshold that avoids excessive noise:

| Threshold | Use when                                                                        |
| --------- | ------------------------------------------------------------------------------- |
| `major`   | Package has excellent ABI stability; breaks only on major versions              |
| `minor`   | Package generally maintains ABI within major version, but minor bumps can break |
| `patch`   | Package has poor ABI stability; even patch releases can break dependents        |
| `always`  | Non-semver versioning or known to break unpredictably                           |

When unsure, prefer `minor` as the default.

## Triggers

### Core System

#### glibc

- **Version scheme:** Semver (2.x.y)
- **Threshold:** `major`
- **Rationale:** Foundational C library. Major version changes (extremely rare) affect all compiled code. Minor/patch versions maintain ABI compatibility.
- **Example dependents:** Nearly all AUR packages (transitively)

#### gcc-libs

- **Version scheme:** Semver (major.minor.patch)
- **Threshold:** `major`
- **Rationale:** C++ ABI changes between major GCC versions. libstdc++ ABI is generally stable within a major version.
- **Example dependents:** All C++ AUR packages

### Toolkits

#### glib2

- **Version scheme:** Semver (2.x.y)
- **Threshold:** `minor`
- **Rationale:** Foundation library for GTK ecosystem. Provides GObject, GIO, and core utilities. ABI changes can affect all GTK-based applications and libraries built on top of GLib.
- **Example dependents:** All GTK applications, dconf, many GNOME/GTK-based AUR packages

#### qt5-base

- **Version scheme:** Semver (5.x.y)
- **Threshold:** `minor`
- **Rationale:** Qt5 maintains soname as `libQt5Core.so.5` across all 5.x releases, but ABI breaks can occur on minor bumps. Plugin architecture means platform themes and plugins must match.
- **Example dependents:** qt5gtk2, qt5ct, qbittorrent (if AUR)

#### qt6-base

- **Version scheme:** Semver (6.x.y)
- **Threshold:** `minor`
- **Rationale:** Same as qt5-base. Soname stays `libQt6Core.so.6` across 6.x, but ABI can break on minor versions. Qt platform plugins loaded via dlopen() must be rebuilt.
- **Example dependents:** qt6gtk2, qt6ct, AUR Qt6 applications

#### gtk2

- **Version scheme:** Semver (2.x.y)
- **Threshold:** `minor`
- **Rationale:** Legacy toolkit, rarely updated. GTK modules and theme engines loaded at runtime.
- **Example dependents:** gtk2 theme engines, legacy AUR applications

#### gtk3

- **Version scheme:** Semver (3.x.y)
- **Threshold:** `minor`
- **Rationale:** GTK modules, input methods, and themes loaded dynamically.
- **Example dependents:** AUR GTK3 applications, custom theme engines

#### gtk4

- **Version scheme:** Semver (4.x.y)
- **Threshold:** `minor`
- **Rationale:** Active development with potential ABI changes between minor versions.
- **Example dependents:** AUR GTK4/libadwaita applications

#### wxwidgets

- **Version scheme:** Semver (3.x.y)
- **Threshold:** `minor`
- **Rationale:** wxWidgets ABI can change between minor versions.
- **Example dependents:** audacity (if AUR), wxwidgets-based AUR applications

#### electron

- **Version scheme:** Semver (major.minor.patch), rapid release cycle
- **Threshold:** `major`
- **Rationale:** Electron apps bundle most dependencies, but native Node modules link against specific Electron versions. Major version changes break native modules.
- **Example dependents:** AUR Electron applications with native components

### Graphics

#### freetype2

- **Version scheme:** Semver (2.x.y)
- **Threshold:** `minor`
- **Rationale:** Font rendering library. Many applications link against libfreetype for text rendering. ABI changes between minor versions can affect font rendering in dependent applications.
- **Example dependents:** AUR applications with custom font rendering, PDF viewers, image editors

#### mesa

- **Version scheme:** Semver (year.minor.patch, e.g., 24.1.0)
- **Threshold:** `minor`
- **Rationale:** OpenGL/Vulkan implementation. ABI changes can affect graphics applications. Also caught by checkrebuild when sonames change.
- **Example dependents:** AUR games, graphics applications

#### vulkan-icd-loader

- **Version scheme:** Semver (follows Vulkan spec version)
- **Threshold:** `minor`
- **Rationale:** Vulkan loader ABI. Applications using Vulkan may need rebuilding.
- **Example dependents:** AUR Vulkan games and applications

### Multimedia

#### ffmpeg

- **Version scheme:** Semver (major.minor.patch)
- **Threshold:** `minor`
- **Rationale:** libavcodec, libavformat, etc. APIs change between minor versions. Many media applications link against FFmpeg.
- **Example dependents:** AUR media players, video editors, streaming tools

#### pipewire

- **Version scheme:** Semver (0.3.x currently)
- **Threshold:** `minor`
- **Rationale:** Audio/video routing. Native PipeWire clients may need rebuilding.
- **Example dependents:** AUR audio applications with PipeWire support

### LLVM Ecosystem

#### llvm-libs

- **Version scheme:** Semver (major.minor.patch)
- **Threshold:** `major`
- **Rationale:** LLVM ABI changes between major versions. Packages linking against LLVM (compilers, tools) need rebuilding.
- **Example dependents:** AUR language tooling, mesa (if AUR), compiler frontends

### Serialization / IPC

#### protobuf

- **Version scheme:** Semver, but frequent breaking changes
- **Threshold:** `patch`
- **Rationale:** Notorious for ABI instability. Even patch releases have been known to break dependent packages. Google does not maintain ABI compatibility guarantees.
- **Example dependents:** AUR applications using Protocol Buffers

#### abseil-cpp

- **Version scheme:** Date-based (YYYYMMDD.N)
- **Threshold:** `always`
- **Rationale:** Abseil explicitly does not provide ABI stability. Often updated alongside protobuf. Date-based versioning means semver parsing fails, triggering `always` behavior anyway.
- **Example dependents:** Usually rebuilt alongside protobuf dependents

#### grpc

- **Version scheme:** Semver (1.x.y)
- **Threshold:** `minor`
- **Rationale:** gRPC depends on protobuf and abseil-cpp. ABI changes propagate through the stack.
- **Example dependents:** AUR gRPC applications

### Cryptography

#### openssl

- **Version scheme:** Semver (3.x.y, previously 1.x.y)
- **Threshold:** `minor`
- **Rationale:** TLS/crypto library. ABI can change between minor versions. The 1.x to 3.x transition changed sonames, but minor bumps within 3.x may not.
- **Example dependents:** AUR applications with TLS support

#### gnutls

- **Version scheme:** Semver (3.x.y)
- **Threshold:** `minor`
- **Rationale:** Alternative TLS library to OpenSSL. Many applications use GnuTLS for TLS/SSL support. ABI changes can occur between minor versions.
- **Example dependents:** AUR applications using GnuTLS (CUPS, Emacs, wget, etc.)

#### icu

- **Version scheme:** Semver (major.minor)
- **Threshold:** `minor`
- **Rationale:** Unicode library with frequent ABI changes. Soname includes major version but ABI breaks can occur within.
- **Example dependents:** AUR applications with internationalization

### Common Libraries

#### curl

- **Version scheme:** Semver (8.x.y)
- **Threshold:** `minor`
- **Rationale:** libcurl is one of the most widely linked libraries for HTTP/network operations. ABI changes between minor versions can affect many applications. 559 reverse dependencies on typical Arch systems.
- **Example dependents:** AUR applications with HTTP/network functionality, download managers, API clients

#### boost

- **Version scheme:** Semver (1.x.y)
- **Threshold:** `minor`
- **Rationale:** Large C++ library collection. ABI compatibility not guaranteed between minor versions. Some Boost libraries are header-only (safe), but many are compiled.
- **Example dependents:** AUR C++ applications using Boost

#### opencv

- **Version scheme:** Semver (4.x.y)
- **Threshold:** `minor`
- **Rationale:** Computer vision library with C++ ABI. API/ABI changes between minor versions.
- **Example dependents:** AUR computer vision applications, ML tools

#### vtk

- **Version scheme:** Semver (9.x.y)
- **Threshold:** `minor`
- **Rationale:** 3D visualization library. Tightly coupled with applications, ABI changes on minor bumps.
- **Example dependents:** paraview (if AUR), scientific visualization tools

### Databases

#### postgresql-libs

- **Version scheme:** Semver (major.minor)
- **Threshold:** `major`
- **Rationale:** PostgreSQL client library. Major version changes can affect client ABI.
- **Example dependents:** AUR applications with PostgreSQL support

### Language Runtimes

#### libffi

- **Version scheme:** Semver (3.x.y)
- **Threshold:** `minor`
- **Rationale:** Foreign Function Interface library used by Python (ctypes), Ruby (fiddle), and other languages to call C code. Critical for language interop. ABI changes affect all FFI-dependent code.
- **Example dependents:** Python packages using ctypes, Ruby packages using fiddle, GObject introspection

#### python

- **Version scheme:** Semver (3.x.y)
- **Threshold:** `minor`
- **Rationale:** Python C extensions are compiled against specific Python minor versions. The stable ABI (abi3) helps but is not universally used.
- **Example dependents:** AUR Python packages with C extensions (python-\*)

#### nodejs

- **Version scheme:** Semver (major.minor.patch), even majors are LTS
- **Threshold:** `major`
- **Rationale:** Node.js native addons (N-API/node-addon-api) link against Node. Major version changes require rebuilding native modules.
- **Example dependents:** AUR Node.js applications with native dependencies

#### ruby

- **Version scheme:** Semver (3.x.y)
- **Threshold:** `minor`
- **Rationale:** Ruby gems with C extensions compile against Ruby headers.
- **Example dependents:** AUR Ruby applications with native gems

#### lua

- **Version scheme:** Semver (5.x)
- **Threshold:** `minor`
- **Rationale:** Lua C modules link against specific Lua versions. Lua 5.1/5.2/5.3/5.4 are not ABI compatible.
- **Example dependents:** AUR applications embedding Lua, Lua C modules

## Finding New Trigger Candidates

When looking for packages that should be added to the curated trigger list, these approaches can help identify candidates:

### Local Reverse Dependency Analysis

The `contrib/find-trigger-candidates.sh` script counts reverse dependencies for all installed packages on your system:

```bash
./contrib/find-trigger-candidates.sh              # Output to dep-count.txt
./contrib/find-trigger-candidates.sh results.txt  # Custom output file
./contrib/find-trigger-candidates.sh -f           # Overwrite existing file
```

Packages with high reverse dependency counts that aren't already in the trigger list are good candidates to investigate.

### Arch Rebuild Tracking

- **arch-rebuild-order** - Tool used by maintainers for mass rebuilds
- **Arch GitLab** - Issues often document what triggered rebuilds: https://gitlab.archlinux.org/archlinux/packaging/packages
- **Mailing list** - Mass rebuild announcements explain triggers: https://lists.archlinux.org/

### rebuild-detector Analysis

The `checkrebuild` script from rebuild-detector shows what it detects:

- Broken ldd (soname changes)
- Python/Perl/Ruby/Haskell version mismatches

Packages that cause rebuilds but _aren't_ caught by checkrebuild are high-value trigger candidates. Source: https://github.com/maximbaz/rebuild-detector

### Arch Wiki

- [System maintenance](https://wiki.archlinux.org/title/System_maintenance) - Mentions rebuild scenarios
- [Pacman tips and tricks](https://wiki.archlinux.org/title/Pacman/Tips_and_tricks) - Dependency queries

### Package Statistics

- https://pkgstats.archlinux.de - Opt-in usage statistics
- Official repo metadata has required-by counts

## Adding New Triggers

When proposing a new trigger via PR:

1. **Document the versioning scheme** - How does the package version? Semver, date-based, other?
2. **Justify inclusion** - Why does this package cause ABI breaks? Is it caught by soname changes?
3. **Propose a threshold** - What's the minimum version change that typically breaks dependents?
4. **Provide examples** - List AUR packages known to break when this trigger updates
5. **Consider overlap** - Note if checkrebuild would also catch this (acceptable but worth documenting)

## Trigger List Version

The embedded trigger list has a version number that increments with each change. This is displayed in `anneal --version` output and helps track which triggers a given Anneal installation includes.
