# Matinee Next — Architecture Overview

Status: **Phase 0 (foundation)**. The shipping app is still the Tauri + React
app in `src/` and `src-tauri/`; it remains the reference implementation and is
not modified by this work. The Rust + GPUI workspace described here is being
built *alongside* it.

Related documents:

- [ui-framework.md](ui-framework.md) — design system, components, GPUI policy
- [platform-strategy.md](platform-strategy.md) — macOS / Windows / Linux adaptation, windows, CI
- [playback.md](playback.md) — native playback feasibility spike and recommendation (all playback findings live there)
- [../migration/current-matinee.md](../migration/current-matinee.md) — inventory of the shipping app
- [../migration/roadmap.md](../migration/roadmap.md) — phased migration plan

## Layers

The architecture separates four kinds of code. Dependencies only point down
this list.

| Layer | Crates | Knows about |
|---|---|---|
| 1. Reusable UI framework | `crates/atelier-ui` | Semantic tokens, themes, generic components, GPUI |
| 2. Reusable desktop infrastructure | `crates/atelier-app` | App lifecycle, windows, commands/shortcuts, menus, platform conventions |
| 3. Matinee presentation | `crates/matinee-ui` | Matinee brand palette and type, mapped onto layer-1 tokens; later Matinee-specific components |
| 4. Matinee domain | `crates/matinee-player` (boundary only); later `matinee-core`, `matinee-jellyfin`, … | Playback, Jellyfin, integrations, Poster Studio |

Applications live in `apps/`:

| App | Purpose |
|---|---|
| `apps/atelier-gallery` | The component catalog (Storybook / SwiftUI Previews equivalent). Product-neutral; Matinee's theme is an opt-in cargo feature (`matinee-theme`, on by default in this repo) so components can be previewed under it. |
| `apps/matinee-next` | A Phase 0 shell that boots the framework with the Matinee theme. Has no product features by design. |

```
apps/atelier-gallery ─┬─> atelier-app ──> atelier-ui ──> gpui (=0.2.2)
                      └─> matinee-ui (optional) ──> atelier-ui
apps/matinee-next ────┬─> atelier-app
                      └─> matinee-ui
matinee-player          (no dependencies yet)
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
  atelier-app/        app.rs (bootstrap, windows), command.rs, platform.rs
  matinee-ui/         Matinee theme
  matinee-player/     Playback boundary (no API yet)
apps/
  atelier-gallery/    story registry + stories/
  matinee-next/       themed shell
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
| `matinee-core`, `matinee-jellyfin`, `matinee-integrations`, `matinee-studio` | Not created | Phase 0 has no code for them; an empty crate would be a speculative boundary. They are scheduled in the roadmap. |
| — | Added `matinee-ui` | Layer 3 (Matinee-specific presentation) needs a home that is neither the generic framework nor domain logic. The Matinee theme lives here. |
| `matinee-player` | Created as an API-less boundary | Reserves the seam and documents where playback goes; the backend recommendation is in [playback.md](playback.md). |
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
cargo +1.90.0 run -p matinee-next               # Matinee-themed shell
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
