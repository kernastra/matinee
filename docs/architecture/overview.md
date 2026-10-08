# Matinee Next — Architecture Overview

Status: **Phase 3E (native Library) on the Phase 3A runtime, the Phase 3B Player, Phase 3C Details, and Phase 3D Home**. The shipping app is still the Tauri + React
app in `src/` and `src-tauri/`. It remains the reference implementation. Its
calendar and Poster Studio commands are thin adapters over the shared crates.
The native app opens Login, then Home; Library is a second root
destination, and Search and Calendar are the third and fourth. Details and
the Player open above Home, Library, or Search. Calendar opens no page.
Settings and Poster Studio are not built.

Related documents:

- [ui-framework.md](ui-framework.md) — design system, components, GPUI policy
- [platform-strategy.md](platform-strategy.md) — macOS / Windows / Linux adaptation, windows, CI
- [playback.md](playback.md) — native playback engine, frame surface, and licensing (all playback findings live there)
- [domain.md](domain.md) — `matinee-core` types
- [jellyfin.md](jellyfin.md) — Jellyfin client, session, and playback negotiation
- [application.md](application.md) — native runtime, Login, session startup, sign-out, and Player
- [home.md](home.md), [library.md](library.md), [details.md](details.md) — the native screens
- [secrets.md](secrets.md) — OS credential vault
- [integrations.md](integrations.md) — Radarr and Sonarr
- [studio.md](studio.md) — Poster Studio services
- [../migration/current-matinee.md](../migration/current-matinee.md) — inventory of the shipping app
- [../migration/roadmap.md](../migration/roadmap.md) — phased migration plan

## Layers

The architecture separates four kinds of code. Dependencies only point down
this list.

| Layer | Crates | Knows about |
|---|---|---|
| 1. Reusable UI framework | `crates/atelier-ui` | Semantic tokens, themes, generic components, GPUI |
| 2. Reusable desktop infrastructure | `crates/atelier-app` | App lifecycle, windows, commands/shortcuts, menus, platform conventions |
| 3. Matinee presentation | `crates/matinee-ui` | Matinee brand palette, bundled fonts, and type, mapped onto layer-1 tokens; later Matinee-specific components |
| 4. Matinee domain and services | `crates/matinee-core`, `crates/matinee-jellyfin`, `crates/matinee-player`, `crates/matinee-secrets`, `crates/matinee-integrations`, `crates/matinee-studio` | Items, the Jellyfin client, playback, the credential vault, Radarr/Sonarr, and Poster Studio. No GPUI. |

Applications live in `apps/`:

| App | Purpose |
|---|---|
| `apps/atelier-gallery` | The component catalog (Storybook / SwiftUI Previews equivalent). Product-neutral; Matinee's theme is an opt-in cargo feature (`matinee-theme`, on by default in this repo) so components can be previewed under it. |
| `apps/atelier-window-lab` | Manual inspection of native window chrome, insets, fullscreen, and scale. Not a component story. |
| `apps/matinee-next` | Native Matinee window: one service runtime, Login, Home, Library, Details, and the Player. |
| `apps/matinee-playback-lab` | Load, transport, tracks, and an external frame. Not the Matinee Player screen. |

```
apps/atelier-gallery ─┬─> atelier-app ──> atelier-ui ──> gpui (=0.2.2)
                      └─> matinee-ui (optional) ──> atelier-ui
apps/atelier-window-lab ──> atelier-app
apps/matinee-next ────┬─> atelier-app
                      ├─> matinee-ui
                      ├─> matinee-core
                      ├─> matinee-jellyfin ──> matinee-core
                      ├─> matinee-player   (libmpv at runtime, no GPUI)
                      └─> matinee-secrets
matinee-jellyfin ──────────> matinee-secrets     (session save/load only)
matinee-integrations ──────> matinee-secrets
matinee-studio ────────────> matinee-secrets
apps/matinee-playback-lab ─┬─> atelier-app
                           ├─> matinee-ui
                           └─> matinee-player   (libmpv at runtime, no GPUI)
matinee-player             (no GPUI, Atelier, domain, or Jellyfin dependency)
src-tauri ─────────────────> matinee-secrets, matinee-integrations, matinee-studio
```

The rules are enforced by `scripts/check-architecture.sh` (run in CI):
no product terms in `atelier-*`, no playback concepts in `atelier-ui`, only
`atelier-ui`/`atelier-app` may depend on `gpui`, layer direction is never
inverted, the native UI does not name `reqwest` or call `block_on`, the
Tokio runtime is constructed only in `apps/matinee-next/src/runtime.rs`,
and application code does not load libmpv itself.

## Repository layout

```
Cargo.toml            Rust workspace root (members: crates/*, apps/*)
Cargo.lock            Lockfile for the new workspace only
crates/
  atelier-ui/         tokens/, theme.rs, components/, motion.rs, bridge.rs (GPUI conversions)
  atelier-app/        app.rs, window.rs, chrome.rs, geometry.rs, command.rs, platform.rs
  matinee-ui/         Matinee theme and bundled fonts
  matinee-core/       Domain types (no HTTP, no GPUI, no player)
  matinee-jellyfin/   Jellyfin client; converts into matinee-core
  matinee-player/     Playback engine (runtime-loaded libmpv, no GPUI)
  matinee-secrets/    OS credential vault (keyring) and an in-memory test store
  matinee-integrations/ Radarr and Sonarr calendar
  matinee-studio/     Poster Studio providers, manifests, and media-folder artwork
apps/
  atelier-gallery/    story registry + stories/
  atelier-window-lab/ native window harness
  matinee-playback-lab/ playback harness (not the Player screen)
  matinee-next/       Login, Home, Library, Details, and the Player
scripts/check-architecture.sh
src/, src-tauri/      Shipping Tauri + React app (adapters call the shared crates; own Cargo.lock)
spikes/               Standalone experiments with their own [workspace] (e.g. native-playback)
```

### Coexistence with the shipping app

- `src-tauri` and `spikes` are listed in the root workspace's `exclude`, so
  Cargo treats each as its own workspace root. `src-tauri` path-depends on
  `matinee-secrets`, `matinee-integrations`, and `matinee-studio`. Those
  crates declare `rust-version = "1.90"`, so `src-tauri` does too. Its
  edition stays 2021. There is still no root `rust-toolchain.toml`.
- Workspace build output goes to the root `target/` (git-ignored); the Tauri
  app keeps `src-tauri/target/`.
- The new workspace's CI (`.github/workflows/rust-workspace.yml`) is
  path-filtered to `crates/`, `apps/`, and the workspace manifests.

### Deviations from the proposed layout

| Proposed | Decision | Why |
|---|---|---|
| `matinee-core`, `matinee-jellyfin` | Created in Phase 2A | Domain types and the Jellyfin client. See [domain.md](domain.md) and [jellyfin.md](jellyfin.md). |
| `matinee-secrets`, `matinee-integrations`, `matinee-studio` | Created in Phase 2B | Credential vault, Radarr/Sonarr, and Poster Studio. The shipping Tauri commands call them. See [secrets.md](secrets.md), [integrations.md](integrations.md), and [studio.md](studio.md). |
| — | Added `matinee-ui` | Layer 3 (Matinee-specific presentation) needs a home that is neither the generic framework nor domain logic. The Matinee theme lives here. |
| `matinee-player` | Playback engine, still UI-agnostic | Phase 1D filled the Phase 0 boundary. The design is in [playback.md](playback.md). |
| `apps/matinee-next` | Login and the service runtime | Phase 3A. The app depends on framework APIs, owns one Tokio runtime, and calls `matinee-jellyfin`. See [application.md](application.md). |

## Toolchain

- Workspace crates use **edition 2024** and declare `rust-version = "1.90"`;
  CI pins **Rust 1.90.0** (`RUST_TOOLCHAIN` in the workflow). GPUI 0.2.2
  requires edition 2024.
- There is deliberately **no root `rust-toolchain.toml`**: rustup applies a
  toolchain file to every subdirectory, which would silently change the
  compiler used for `src-tauri`. Use `cargo +1.90.0 …` or
  `rustup override set 1.90.0` locally. (Standalone spikes may pin their own
  toolchain inside `spikes/<name>/`.)
- `src-tauri` declares `rust-version = "1.90"` because the shared crates are
  edition 2024. Edition of `src-tauri` stays 2021. Rust 1.77 cannot compile
  this tree. The lockfile already needed Rust ≥ 1.85 before this phase
  (`idna_adapter` 1.2.2). CI compiles the adapter on Linux, macOS, and
  Windows; only Linux runs `cargo test` for it.

## Building and running

```bash
# Linux system libraries for GPUI (Debian/Ubuntu names; Fedora: *-devel)
sudo apt-get install pkg-config libdbus-1-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libvulkan-dev libfontconfig-dev libfreetype-dev libx11-xcb-dev libxcb1-dev

cargo +1.90.0 run -p atelier-gallery            # opens the Gallery
cargo +1.90.0 run -p atelier-gallery -- button  # opens a specific story by id
cargo +1.90.0 run -p atelier-window-lab         # native window harness
cargo +1.90.0 run -p matinee-next               # Login, then Home
cargo +1.90.0 run -p matinee-playback-lab -- --demo   # playback harness
ATELIER_REDUCED_MOTION=1 cargo +1.90.0 run -p atelier-gallery

scripts/check-architecture.sh
cargo +1.90.0 fmt --all -- --check
cargo +1.90.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.90.0 test --workspace --locked
```

On machines without a GPU, GPUI's Linux renderer needs a Vulkan driver;
Mesa's lavapipe (`mesa-vulkan-drivers`) works.

## Naming

`Atelier` is a working name. It is centralized so renaming is mechanical:

1. Package names and paths in the root `Cargo.toml` `[workspace.dependencies]`.
2. `atelier_ui::` / `atelier_app::` paths in Rust code (a single search/replace).
3. `atelier_app::FRAMEWORK_NAME` (user-visible display name).
4. Action namespace `atelier` in `atelier-app` (`actions!(atelier, …)`).
5. Asset prefix `atelier/icons/` in `atelier-ui`.
6. Product terms check in `scripts/check-architecture.sh` (unaffected).

No Kernastra concepts or dependencies are used anywhere in the workspace.
