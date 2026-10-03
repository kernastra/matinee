# Matinee Next — Architecture Overview

Status: **Phase 2A (domain and Jellyfin client) on the Phase 1D player**. The shipping app is still the Tauri + React
app in `src/` and `src-tauri/`; it remains the reference implementation and is
not modified by this work. The Rust + GPUI workspace described here is being
built *alongside* it.

Related documents:

- [ui-framework.md](ui-framework.md) — design system, components, GPUI policy
- [platform-strategy.md](platform-strategy.md) — macOS / Windows / Linux adaptation, windows, CI
- [playback.md](playback.md) — native playback engine, frame surface, and licensing (all playback findings live there)
- [domain.md](domain.md) — `matinee-core` types
- [jellyfin.md](jellyfin.md) — Jellyfin client, session, and playback negotiation
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
| 4. Matinee domain | `crates/matinee-core`, `crates/matinee-jellyfin`, `crates/matinee-player` | Items and progress, the Jellyfin client, and playback. Integrations and Poster Studio are later. |

Applications live in `apps/`:

| App | Purpose |
|---|---|
| `apps/atelier-gallery` | The component catalog (Storybook / SwiftUI Previews equivalent). Product-neutral; Matinee's theme is an opt-in cargo feature (`matinee-theme`, on by default in this repo) so components can be previewed under it. |
| `apps/atelier-window-lab` | Manual inspection of native window chrome, insets, fullscreen, and scale. Not a component story. |
| `apps/matinee-next` | A featureless shell: Matinee theme, native window, generic toolbar and sidebar. No player. It links `matinee-core` and does not call it. |
| `apps/matinee-playback-lab` | Load, transport, tracks, and an external frame. Not the Matinee Player screen. |

```
apps/atelier-gallery ─┬─> atelier-app ──> atelier-ui ──> gpui (=0.2.2)
                      └─> matinee-ui (optional) ──> atelier-ui
apps/atelier-window-lab ──> atelier-app
apps/matinee-next ────┬─> atelier-app
                      ├─> matinee-ui
                      └─> matinee-core
matinee-jellyfin ──────────> matinee-core
apps/matinee-playback-lab ─┬─> atelier-app
                           ├─> matinee-ui
                           └─> matinee-player   (libmpv at runtime, no GPUI)
matinee-player             (no GPUI, Atelier, domain, or Jellyfin dependency)
```

The rules are enforced by `scripts/check-architecture.sh` (run in CI):
no product terms in `atelier-*`, no playback concepts in `atelier-ui`, only
`atelier-ui`/`atelier-app` may depend on `gpui`, and layer direction is never
inverted.

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
apps/
  atelier-gallery/    story registry + stories/
  atelier-window-lab/ native window harness
  matinee-playback-lab/ playback harness (not the Player screen)
  matinee-next/       themed shell, still featureless
scripts/check-architecture.sh
src/, src-tauri/      Shipping Tauri + React app (unchanged; src-tauri has its own Cargo.lock)
spikes/               Standalone experiments with their own [workspace] (e.g. native-playback)
```

### Coexistence with the shipping app

- `src-tauri` and `spikes` are listed in the root workspace's `exclude`, so
  Cargo treats each as its own workspace root. `src-tauri/Cargo.lock` is
  untouched and the Tauri build, `pnpm` scripts, and the existing security
  workflow behave exactly as before.
- Workspace build output goes to the root `target/` (git-ignored); the Tauri
  app keeps `src-tauri/target/`.
- The new workspace's CI (`.github/workflows/rust-workspace.yml`) is
  path-filtered to `crates/`, `apps/`, and the workspace manifests.

### Deviations from the proposed layout

| Proposed | Decision | Why |
|---|---|---|
| `matinee-core`, `matinee-jellyfin` | Created in Phase 2A | Domain types and the Jellyfin client. See [domain.md](domain.md) and [jellyfin.md](jellyfin.md). |
| `matinee-integrations`, `matinee-studio` | Not created | No current consumer. An empty crate would be a speculative boundary. |
| — | Added `matinee-ui` | Layer 3 (Matinee-specific presentation) needs a home that is neither the generic framework nor domain logic. The Matinee theme lives here. |
| `matinee-player` | Playback engine, still UI-agnostic | Phase 1D filled the Phase 0 boundary. The design is in [playback.md](playback.md). |
| `apps/matinee-next` | Minimal shell only | Proves an app can boot on the framework with the Matinee theme without depending on GPUI directly. |

## Toolchain

- Workspace crates use **edition 2024** and declare `rust-version = "1.90"`;
  CI pins **Rust 1.90.0** (`RUST_TOOLCHAIN` in the workflow). GPUI 0.2.2
  requires edition 2024.
- There is deliberately **no root `rust-toolchain.toml`**: rustup applies a
  toolchain file to every subdirectory, which would silently change the
  compiler used for `src-tauri`. Use `cargo +1.90.0 …` or
  `rustup override set 1.90.0` locally. (Standalone spikes may pin their own
  toolchain inside `spikes/<name>/`.)
- Note: `src-tauri/Cargo.lock` already requires Rust ≥ 1.85 (it resolves
  `idna_adapter 1.2.2`, an edition-2024 crate). That predates this work.

## Building and running

```bash
# Linux system libraries for GPUI (Debian/Ubuntu names; Fedora: *-devel)
sudo apt-get install pkg-config libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libvulkan-dev libfontconfig-dev libfreetype-dev libx11-xcb-dev libxcb1-dev

cargo +1.90.0 run -p atelier-gallery            # opens the Gallery
cargo +1.90.0 run -p atelier-gallery -- button  # opens a specific story by id
cargo +1.90.0 run -p atelier-window-lab         # native window harness
cargo +1.90.0 run -p matinee-next               # Matinee-themed shell
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
