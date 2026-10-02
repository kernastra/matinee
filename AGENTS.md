# AGENTS.md

Guidance for anyone (human or agent) changing this repository.

## Two apps live here

- **Shipping Matinee:** Tauri 2 + React in `src/` and `src-tauri/`. This is
  the **reference implementation**. Do not casually remove or rewrite
  working behavior, replace package scripts, delete CSS, or change production
  behavior while working on the native app.
- **Matinee Next:** the Rust + GPUI workspace in `crates/` and `apps/` (root
  `Cargo.toml`), built alongside the shipping app. Start with
  [docs/architecture/overview.md](docs/architecture/overview.md).

**Do not start a migration phase** ([docs/migration/roadmap.md](docs/migration/roadmap.md))
without explicit instruction.

## Layering rules (Matinee Next)

1. The generic framework (`atelier-ui`, `atelier-app`) contains **no Matinee
   business concepts**: no Jellyfin, movies, libraries, Poster Studio, or
   Matinee colors or fonts. Brand values live in `crates/matinee-ui`.
2. Applications depend on **framework APIs** (`atelier_ui::prelude`,
   `atelier_app`) rather than raw GPUI wherever practical. Only `atelier-ui`
   and `atelier-app` may list `gpui` as a dependency.
3. Use **semantic tokens** (`ColorRole`, `TextRole`, `Space`, `Radius`,
   `Elevation`, `MotionDuration`/`Spring`), never arbitrary colors, sizes,
   or durations.
4. **Behavior belongs in reusable components** (states, focus, keyboard
   activation, motion), not in app call sites.
5. **Platform differences are centralized** in `atelier-app`
   (`platform.rs`, `command.rs`). No `cfg(target_os)` in components or apps.
6. **Accessibility and reduced motion are first-class.** Themes must pass
   `Theme::validate()`. Animations go through `atelier_ui::motion` (which
   returns `None` under reduced motion). Icon-only controls require a label.
7. **Every new reusable component gets a Gallery story**
   (`apps/atelier-gallery/src/story.rs` + `stories/`).
8. **Avoid speculative abstraction.** No crate, trait, or option without a
   current consumer.
9. **Video belongs in `matinee-player`**, never in `atelier-ui`. The engine
   decision (libmpv, frames painted through a GPUI `RenderImage`) and its
   licensing constraints (LGPL builds only) are in
   [docs/architecture/playback.md](docs/architecture/playback.md). Put playback
   findings there, not in new documents.
10. GPUI is pinned (`=0.2.2`). Bump it only in a dedicated PR, following
    [docs/architecture/ui-framework.md](docs/architecture/ui-framework.md#gpui-dependency-policy).

## Checks

Shipping app:

```bash
pnpm install --frozen-lockfile && pnpm typecheck && pnpm test && pnpm build
(cd src-tauri && cargo check --locked && cargo test --locked)   # Rust >= 1.85
```

Matinee Next (Rust 1.90.0; no root `rust-toolchain.toml`, by design):

```bash
scripts/check-architecture.sh
cargo +1.90.0 fmt --all -- --check
cargo +1.90.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.90.0 test --workspace --locked
cargo +1.90.0 run -p atelier-gallery
```

Standalone experiments live in `spikes/<name>/` with their own `[workspace]`
and are excluded from the root workspace.
