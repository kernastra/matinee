# Matinee — Your Jellyfin library, dressed for movie night

<p align="center">
  <img src="docs/assets/hero.png" alt="Matinee neighborhood theater at dusk" width="100%" />
</p>

## Overview

Matinee is a standalone desktop client for personal [Jellyfin](https://jellyfin.org/) libraries. It pairs Jellyfin's media server and playback APIs with a warm, cinema-inspired interface designed for the couch: rich artwork, editorial home sections, focused title pages, and custom playback controls. Matinee connects to your existing server and library; it does not replace or bundle Jellyfin itself.

## Demo

<p align="center">
  <a href="docs/assets/matinee-demo.mp4">
    <img src="docs/assets/matinee-demo.webp" alt="Matinee desktop app home screen demonstration" width="100%" />
  </a>
</p>

<p align="center"><sub>The preview loops automatically. Select it to open the full MP4.</sub></p>

## Features

- **Cinematic home screen** — Rotating hero artwork, continue-watching titles, recent additions, favorites, featured picks, and ranked collections built from your Jellyfin library.
- **Movies and series** — Browse dedicated libraries, open detailed title pages, move through seasons, and choose individual episodes.
- **Library search** — Search movies, series, and episodes without leaving the app.
- **Jellyfin playback** — Negotiate direct playback or HLS transcoding with the server and resume titles from their saved position.
- **Custom player controls** — Play, pause, seek, change volume, select audio or subtitles, choose playback quality, and enter fullscreen from a cinema-styled control surface.
- **Library actions** — Add titles to the Jellyfin favorites-based watchlist and mark them played or unplayed.
- **Personal settings** — Persist playback quality, preferred audio, subtitle behavior, autoplay, hero rotation, and reduced-motion defaults on the device.
- **Native desktop shell** — Run in a frameless, resizable Tauri window with custom traffic lights and locally bundled typefaces and icons.

## Tech Stack

| Layer | Technology |
| --- | --- |
| Desktop shell | Tauri 2, Rust |
| Interface | React 19, TypeScript |
| Build tooling | Vite 7, pnpm |
| Playback | HTML5 video, hls.js, Jellyfin REST API |
| Styling | Hand-authored CSS |
| Tests | Vitest, jsdom |

## Getting Started

### Prerequisites

- A running Jellyfin server that is reachable from your computer
- Node.js 20 or newer
- pnpm 10 or newer
- The stable Rust toolchain
- The platform dependencies listed in the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

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

```bash
git clone https://github.com/kernastra/matinee.git
cd matinee
pnpm install
pnpm tauri:dev
```

No environment file is required. On first launch, enter the root URL of your Jellyfin server—such as `http://jellyfin.local:8096`—followed by your Jellyfin username and password.

## Usage

1. Start Matinee with `pnpm tauri:dev`.
2. Enter the full Jellyfin server address, including `http://` or `https://`, and sign in.
3. Browse the home screen or use Movies, Series, and Search to find a title.
4. Open a title for its details, then select Play or Resume. For a series, choose a season and episode first.
5. Move the pointer over the player to reveal its controls. They fade away after five seconds of inactivity.

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

The visual direction, component rules, interaction patterns, and Matinee color tokens live in [docs/design-spec.md](docs/design-spec.md).

## Project Structure

```text
src/
├── assets/              Branded artwork, fonts, and Material Symbols
├── components/          Navigation, library, details, editorial, and player UI
├── lib/jellyfin.ts      Jellyfin API, session, library, and playback helpers
├── App.tsx              Application routing and session orchestration
└── styles.css           Shared Matinee design system and component styles
src-tauri/
├── src/                 Native Tauri application entry point
└── tauri.conf.json      Window, security, and bundle configuration
docs/
├── assets/              README and documentation artwork
└── design-spec.md       Product-specific visual and interaction specification
```

## Platform Notes

- Linux is the primary development and testing platform today. The Tauri configuration can produce other desktop targets, but macOS and Windows builds have not yet been validated.
- Matinee uses the system WebView. On Linux that means WebKitGTK 4.1; codec availability can vary by distribution.
- The `tauri:dev` and `tauri:build` scripts include renderer and AppImage compatibility flags used by the current Fedora development environment.

## Known Limitations

- Login state is stored for the current app session, so a full restart requires signing in again.
- Trailer and More actions appear on title pages but are intentionally disabled until their flows are implemented.
- Playback compatibility ultimately depends on the source media, server-side FFmpeg setup, enabled Jellyfin transcoding, and codecs supported by the system WebView.
- Packaged releases and automated CI artifacts are not available yet; build the app from source.

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

## License

Matinee is licensed under the [MIT License](LICENSE).
