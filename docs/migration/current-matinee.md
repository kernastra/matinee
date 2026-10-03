# Current Matinee (reference implementation)

The shipping app (v0.5.6) is the behavioral reference for Matinee Next. Nothing
here is removed or rewritten during migration until a native replacement
reaches parity and is explicitly approved. Inventory verified against the repo
at Phase 0.

## Stack

| Area | Technology | Location |
|---|---|---|
| Shell | Tauri 2.11 (Rust), frameless transparent window, tray icon, `macOSPrivateApi` | `src-tauri/` |
| UI | React 19.2, TypeScript 5.9, Vite 7, pnpm 11 | `src/` |
| Styling | Hand-authored CSS (~1.2k lines), `@fontsource` Fraunces / Manrope / IBM Plex Mono, Material Symbols TTF | `src/styles.css`, `src/assets/` |
| Playback | HTML5 `<video>` + hls.js (dynamic import), Jellyfin direct play and HLS transcode | `src/components/Player.tsx` |
| Jellyfin | REST client (auth, libraries, items, playback info, progress, played/watchlist) | `src/lib/jellyfin.ts` |
| Calendar | Radarr / Sonarr monitored releases. Rust normalizes and caches them. TypeScript keeps date labels and a synchronous Home helper. | `crates/matinee-integrations`, `src-tauri/src/media_calendar.rs`, `src/lib/integrations.ts` |
| Poster Studio | Guided generation (Codex CLI or fal.ai), `movie.mf.json` manifests, export, `poster.jpg` beside media. Prompt composition stays in TypeScript. | `crates/matinee-studio`, `src-tauri/src/image_generation.rs`, `src/lib/posterPrompts.ts` |
| Secrets | OS credential vault via `keyring` (Secret Service / Keychain / Credential Manager), called through `matinee-secrets` | `crates/matinee-secrets` |
| Settings | `localStorage` (`matinee.settings.v1`), including reduced motion | `src/lib/settings.ts` |
| Window chrome | Custom 28px navbar, macOS-style traffic lights on all platforms, 10px window corners; Tauri commands close/minimize/maximize | `WindowChrome.tsx`, `src-tauri/src/window.rs` |
| Tests | Vitest + jsdom (5 files, 46 tests at Phase 0); Rust test harness | `src/lib/*.test.ts` |

## Surfaces

Login, Home (rotating hero, editorial shelves, Coming Soon), Library (movies and
series), Search overlay, Details and Series Details (cast, related,
collections, chapters, technical details, seasons and episodes), Player,
Calendar, Settings (playback, integrations, image providers), Poster Studio.

## Tauri command surface

`assign_generated_poster`, `export_generated_image`,
`export_poster_to_media_folder`, `generate_poster_image`,
`list_custom_posters`, `load_movie_manifest`, `provider_key_status`,
`remove_provider_key`, `save_provider_key`, `scan_local_image_provider`,
`fetch_integration_calendar`, `fetch_upcoming_releases`,
`clear_integration_calendar_cache`, `integration_key_status`,
`remove_integration_key`, `test_and_save_integration`, `close_window`,
`minimize_window`, `toggle_maximize_window`.

The calendar, Poster Studio, and credential commands are adapters. The
implementations live in `matinee-integrations`, `matinee-studio`, and
`matinee-secrets`. `src-tauri` declares Rust 1.90 so it can compile those
crates. Its edition stays 2021.

## Design sources of truth

- `docs/design-spec.md`: palette (Midnight Navy, Ticket Cream, Marquee Amber,
  Curtain Burgundy, Faded Teal, Projection Room, Theater Brown), type families
  and the 64/40/26/17/16/13/11 scale, product rules (real data only, visible
  progress, graceful missing artwork, restrained motion, honest availability
  language).
- `src/styles.css` `:root`: implemented values (for example `--muted #a89e8d`
  and `--accent-hover #f0b76d`), mirrored in `crates/matinee-ui`.

## Behaviors to preserve in any migration

- Resume and progress always visible for resumable titles.
- Missing artwork degrades without layout breakage.
- Reduced motion disables decorative motion.
- Partial integration failures do not hide other providers' results.
- Movie availability language distinguishes theatrical / digital / physical.
- Network boundary: user-configured Jellyfin over HTTP or HTTPS (LAN IPs, local
  hostnames, reverse proxies).
- Linux/Fedora is the primary platform; macOS and Windows builds of the Tauri
  app are not validated.

## Known build note

`src-tauri` declares `rust-version = "1.90"` because it compiles the shared
service crates. Those crates are edition 2024, which Rust 1.77 cannot parse.
Rust 1.90 is required for the shipping adapter and for the workspace. The
previous `rust-version = "1.77.2"` does not build this tree. The services are
not forked to keep the old compiler. The lockfile already needed Rust ≥ 1.85
(`idna_adapter` 1.2.2) before Phase 2B. CI installs Rust 1.90.0 for the
workspace and for `src-tauri`. There is still no root `rust-toolchain.toml`;
use `cargo +1.90.0` in `src-tauri` the same way as the workspace. Linux
builds of `keyring` need `libdbus-1-dev` at compile time. Unit tests do not
start a Secret Service daemon. Linux CI compiles and tests the adapter.
macOS and Windows CI compile `src-tauri` (`cargo check --locked`) and do not
launch Tauri or write to Keychain or Credential Manager.

The security workflow already failed on `main` before this phase. The pins
that restore it are separate from the service split: `rustls` 0.23.45 in
`src-tauri` (RUSTSEC-2026-0285), Vitest 4.1.11, Vite 7.3.6, and a
`pnpm-workspace.yaml` override for `nanoid` 3.3.18. They are not a general
dependency upgrade.
