# Matinee — Your Jellyfin library, dressed for movie night

<p align="center">
  <img src="docs/assets/hero.png" alt="Matinee neighborhood theater at dusk" width="100%" />
</p>

## Overview

Matinee is a standalone desktop client for personal [Jellyfin](https://jellyfin.org/) libraries. It pairs Jellyfin's media server and playback APIs with a warm, cinema-inspired interface designed for the couch: rich artwork, editorial home sections, focused title pages, custom playback controls, and a built-in studio for creating collector-style library artwork. Matinee connects to your existing server and library; it does not replace or bundle Jellyfin itself.

## Demo

<p align="center">
  <a href="docs/assets/matinee-demo.mp4">
    <img src="docs/assets/matinee-demo.webp" alt="Matinee desktop app home screen demonstration" width="100%" />
  </a>
</p>

<p align="center"><sub>The preview loops automatically. Select it to open the full MP4.</sub></p>

## Features

- **Cinematic home screen** — Rotating hero artwork, continue-watching titles, recent additions, favorites, featured picks, and ranked collections built from your Jellyfin library.
- **Personal release calendar** — Connect Radarr or Sonarr to see only monitored movie milestones and upcoming episodes, with a conditional Coming Soon shelf on Home.
- **Movies and series** — Browse dedicated libraries, open detailed title pages, move through seasons, and choose individual episodes.
- **Library search** — Search movies, series, and episodes without leaving the app.
- **Jellyfin playback** — Negotiate direct playback or HLS transcoding with the server and resume titles from their saved position.
- **Custom player controls** — Play, pause, seek, change volume, select audio or subtitles, choose playback quality, and enter fullscreen from a cinema-styled control surface.
- **Library actions** — Add titles to the Jellyfin favorites-based watchlist and mark them played or unplayed.
- **Expanded movie details** — Browse cast, related titles, collection context, chapters, and detailed video, audio, subtitle, and file information.
- **Poster Studio** — Select a real title, choose four concise creative directions, and generate coordinated posters, backdrops, banners, or thumbnails without writing prompts.
- **Movie-specific art direction** — Load optional `movie.mf.json` manifests beside local movie files to turn curated characters, signature objects, scenes, environments, palette hints, and visual motifs into selectable creative options.
- **Matinee house style** — Combine each title's Movie DNA with a versioned palette, typography, screen-print texture, composition, and quality system for a cohesive library.
- **Multiple image providers** — Generate through an authenticated local Codex CLI or a fal.ai API key stored in the operating-system credential vault. Higgsfield configuration is present for future endpoint support.
- **Persistent custom artwork** — Assign generated posters throughout Matinee, export copies to Pictures, or save approved movie artwork beside local media as `poster.jpg`.
- **Native tray support** — Hide Matinee to the system tray, restore the window, or quit from a branded native menu.
- **Personal settings** — Persist playback quality, preferred audio, subtitle behavior, autoplay, hero rotation, reduced motion, release integrations, image provider, and poster-metadata visibility on the device.
- **Native desktop shell** — Run in a frameless, resizable Tauri window with custom traffic lights and locally bundled typefaces and icons.

## Tech Stack

| Layer | Technology |
| --- | --- |
| Desktop shell | Tauri 2, Rust |
| Interface | React 19, TypeScript |
| Build tooling | Vite 7, pnpm |
| Playback | HTML5 video, hls.js, Jellyfin REST API |
| Image generation | Local Codex CLI, fal.ai FLUX/FLUX 2 Edit |
| Native services | Rust, OS credential vault, local artwork persistence |
| Styling | Hand-authored CSS |
| Tests | Vitest, jsdom |

## Getting Started

### Prerequisites

- A running Jellyfin server that is reachable from your computer
- Node.js 20 or newer
- pnpm 10 or newer
- The stable Rust toolchain
- The platform dependencies listed in the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- Optional: an authenticated local [Codex CLI](https://developers.openai.com/codex/cli/) or a fal.ai API key for Poster Studio generation

On Fedora, install the current Tauri Linux dependencies with:

```bash
sudo dnf check-update
sudo dnf install webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel
sudo dnf group install "c-development"
```

### Installation

Download the AppImage, Debian package, or RPM for the latest version from
[GitHub Releases](https://github.com/kernastra/matinee/releases). To run Matinee
from source instead:

```bash
git clone https://github.com/kernastra/matinee.git
cd matinee
pnpm install
pnpm tauri:dev
```

No environment file is required. On first launch, enter the root URL of your Jellyfin server—such as `http://jellyfin.local:8096`—followed by your Jellyfin username and password. Radarr, Sonarr, and image-provider credentials are configured inside Settings; Matinee stores managed API keys in the operating-system credential vault rather than the repository or browser storage.

## Usage

1. Start Matinee with `pnpm tauri:dev`.
2. Enter the full Jellyfin server address, including `http://` or `https://`, and sign in.
3. Browse the home screen or use Movies, Series, and Search to find a title.
4. Open a title for its details, then select Play or Resume. For a series, choose a season and episode first.
5. Move the pointer over the player to reveal its controls. They fade away after five seconds of inactivity.

### Follow monitored releases

1. Open **Settings → Coming soon**.
2. Enter the root address and API key for Radarr, Sonarr, or both, then select **Test & save**.
3. Open **Calendar** from the main navigation. The destination appears whenever at least one release integration is configured.
4. Use **All**, **Movies**, or **Series** to filter the agenda and month grid. Radarr movies retain separate theatrical, digital, and physical milestones.
5. Select **Refresh** to bypass the short request cache and request current monitored dates from each connected service.

Matinee reads release information only. It does not add, remove, monitor, search for, or download media, so any compatible request service can continue feeding Radarr and Sonarr independently.

### Create a custom poster

1. Open the profile menu and select **Poster Studio**.
2. Choose a movie or series from the connected Jellyfin library.
3. Select an asset type, focus, title-specific subject, and text treatment. Matinee assembles the full creative brief behind the scenes.
4. Select **Generate poster**. A completed poster is assigned to that title inside Matinee automatically.
5. Export a normal copy to Pictures, or—for movies whose Jellyfin file resolves to a local folder—save it as `poster.jpg` beside the media.

Poster Studio works from normal Jellyfin metadata when no creative manifest is available. For richer options, place a version-1 `movie.mf.json` in the movie folder. Matinee reads only the selected title's manifest; it does not independently scan or index the media library.

The **Advanced** panel exposes the assembled prompt, permits an editable copy or complete custom brief, and imports `.txt`, `.md`, and prompt-bearing `.json` files. Matinee styling can be kept or disabled for a custom prompt.

<!-- ===== Repo-Specific Sections ===== -->

## Development

| Command | Purpose |
| --- | --- |
| `pnpm tauri:dev` | Run the Vite frontend inside the Tauri desktop shell |
| `pnpm dev` | Run the frontend development server on port 1421 |
| `pnpm typecheck` | Check the TypeScript project without emitting files |
| `pnpm test` | Run the Vitest test suite once |
| `pnpm build` | Type-check and create the production frontend bundle |
| `pnpm tauri:build` | Build installable desktop bundles for the current platform |

The visual direction, component rules, interaction patterns, and Matinee color tokens live in [docs/design-spec.md](docs/design-spec.md). Poster generation uses the machine-readable house style in [`src/data/matinee-poster-style.json`](src/data/matinee-poster-style.json) and the prompt architecture documented in [docs/poster-prompt-templates.md](docs/poster-prompt-templates.md).

## Project Structure

```text
src/
├── assets/              Branded artwork, fonts, and Material Symbols
├── components/          Navigation, library, playback, settings, and Poster Studio UI
├── data/                Versioned Matinee poster-style manifest
├── lib/                 Jellyfin, settings, generation, manifest, and prompt helpers
├── App.tsx              Application routing and session orchestration
└── styles.css           Shared Matinee design system and component styles
src-tauri/
├── src/                 Native shell, image providers, keyring, and local-file commands
└── tauri.conf.json      Window, security, and bundle configuration
docs/
├── assets/              README and documentation artwork
├── design-spec.md       Product-specific visual and interaction specification
└── poster-prompt-templates.md  Poster prompt architecture and genre recipes
```

## Platform Notes

- Linux is the primary development and testing platform today. The Tauri configuration can produce other desktop targets, but macOS and Windows builds have not yet been validated.
- Matinee uses the system WebView. On Linux that means WebKitGTK 4.1; codec availability can vary by distribution.
- The `tauri:dev` and `tauri:build` scripts include renderer and AppImage compatibility flags used by the current Fedora development environment.

## Known Limitations

- Login state is stored for the current app session, so a full restart may require signing in again.
- Calendar dates depend on the metadata available in Radarr and Sonarr. A monitored title with no future release or air date will not appear until its upstream metadata is updated.
- Trailer playback remains unavailable until Jellyfin provides a supported trailer source. The More menu is available for playback, media, watchlist, played-state, progress, and title-copy actions.
- Chapter navigation works whenever Jellyfin returns chapters; preview artwork requires chapter-image extraction to be enabled and completed on the Jellyfin server.
- Playback compatibility ultimately depends on the source media, server-side FFmpeg setup, enabled Jellyfin transcoding, and codecs supported by the system WebView.
- Image generation requires a separately authenticated provider and remains subject to that provider's availability, billing, safety systems, and acceptable-use policy. A rejected generation is not retried automatically.
- Higgsfield can be selected and configured, but generation remains disabled until Matinee adopts a stable documented developer endpoint.
- `movie.mf.json` discovery and direct `poster.jpg` export currently apply to movies whose Jellyfin media path can be resolved on the same computer. Series and remote-only libraries fall back to Jellyfin metadata and normal image export.
- AI-rendered title text may be misspelled. Use **No Text** or the Advanced workflow when deterministic typography is required.
- Linux packages are produced for x86_64 systems. Windows and macOS releases have not yet been validated.

## Troubleshooting

### Matinee cannot connect to Jellyfin

Use the server's root address—not a browser page such as `/web/index.html`—and include the URL scheme and port when required. Confirm that the same address opens from the computer running Matinee.

### Playback does not start

First test the same title in Jellyfin's official web client. If it also fails there, check the server's FFmpeg configuration and transcoding permissions. If it succeeds, inspect the Matinee development console and Jellyfin server logs to see whether direct play or an HLS transcode was selected.

### Port 1421 is already in use

An earlier Vite process may still be running after the desktop window closes. Identify the process before stopping it:

```bash
ss -ltnp 'sport = :1421'
kill <pid>
pnpm tauri:dev
```

### The Linux window is blank or transparent

Launch with `pnpm tauri:dev` rather than invoking `tauri dev` directly. The project script sets the WebKit renderer compatibility flag used by Matinee.

### Poster generation is unavailable

Open Settings → Poster generation. For ChatGPT/Codex, install and authenticate the local Codex CLI, then run Matinee's provider scan. For fal.ai, add a valid API key; it is saved to the operating-system credential vault.

### A poster is rejected by the image provider

Provider safety and content-policy checks can reject either a prompt, a reference image, or the generated output. Try a more symbolic **Auto**, **Signature Element**, **Scene**, or **Environment** focus, correct overly broad Movie DNA, or select another configured provider. Matinee deliberately makes one attempt and does not bypass provider safeguards.

### Matinee cannot save `poster.jpg`

The selected Jellyfin movie must expose a media-file path that resolves on the same computer. If Jellyfin runs in Docker, make sure its `/media`, `/movies`, `/tv`, or `/shows` mount corresponds to a local media folder Matinee can access. Export to Pictures remains available when direct media-folder export is not.

### Calendar is missing or reports an unavailable integration

Calendar remains hidden until Radarr or Sonarr passes **Test & save** in Settings. Use the service's root address, including `http://` or `https://` and its port, then confirm the API key under the service's general settings. If one integration is offline, Matinee keeps showing results from the other and marks the unavailable service in the Calendar status row.

## License

Matinee is licensed under the [MIT License](LICENSE).
