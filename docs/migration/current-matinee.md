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
| Calendar | Radarr / Sonarr monitored releases, fetched through Rust | `src-tauri/src/media_calendar.rs`, `src/lib/integrations.ts`, `Calendar.tsx`, `ComingSoonShelf.tsx` |
| Poster Studio | Guided generation (Codex CLI or fal.ai), `movie.mf.json` manifests, export, `poster.jpg` beside media | `src-tauri/src/image_generation.rs`, `src/lib/posterPrompts.ts`, `PosterStudio.tsx` |
| Secrets | OS credential vault via `keyring` (Secret Service / Keychain / Credential Manager) | `src-tauri` |
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
`fetch_integration_calendar`, `integration_key_status`,
`remove_integration_key`, `test_and_save_integration`, `close_window`,
`minimize_window`, `toggle_maximize_window`.

This Rust logic (image generation, calendar, credential handling) is the
first candidate for extraction into shared, UI-agnostic crates
(`matinee-integrations`, `matinee-studio`) that both apps can use.

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

`src-tauri/Cargo.lock` requires Rust ≥ 1.85 (edition-2024 dependency
`idna_adapter 1.2.2`). With Rust 1.90.0, `cargo check --locked` and
`cargo test --locked` in `src-tauri` pass, as do `pnpm typecheck`,
`pnpm test`, and `pnpm build`.
