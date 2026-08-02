# Matinee Design Specification

## Direction

Matinee is a cinematic, desktop-first Jellyfin client inspired by the supplied
SaintStream homepage composition and Matinee's supplied theater brand guide. These
references override the workspace default for the content surface: midnight-navy
backgrounds, amber actions, cream typography, artwork-led heroes, and dense horizontal
media rows are product requirements rather than decorative exceptions.

## Inherited workspace rules

- Frameless Tauri window with a seamless 28px navbar
- Functional macOS-style traffic lights using canonical geometry and states
- One continuous root window surface with exact 10px outer corners
- Monospace application chrome and compact metadata
- Dark-only presentation and accessible focus states

## Explicit overrides

- The primary content surface is Midnight Navy `#111820` rather than Catppuccin's
  `#1e1e2e`, supported by Projection Room `#1b232b` and Theater Brown `#29231f`.
- Ticket Cream `#f6eedd` is the primary foreground and Marquee Amber `#e6a452`
  communicates playback, focus, selection, and warm theater-light accents.
- Curtain Burgundy `#963f47` is reserved for errors or rare emphasis; Faded Teal
  `#658184` is supporting color rather than a competing action color.
- Fraunces is reserved for editorial display headings, Manrope for interface and body
  copy, and IBM Plex Mono for metadata and compact application chrome.
- Cinematic gradients are allowed only to preserve text legibility over live
  Jellyfin artwork and to join hero imagery to the content surface.

## Product rules

- Every populated media card represents real Jellyfin data.
- Progress is always visible when a title is resumable.
- Missing artwork must degrade gracefully without breaking layout.
- Public-service concepts that Jellyfin cannot support honestly—global popularity,
  provider availability, and fake editorial rankings—are not fabricated.
- Motion is restrained, brief, and disabled by reduced-motion preferences.

## Network boundary

Matinee accepts a user-configured Jellyfin server over HTTP or HTTPS, including
LAN IP addresses, local hostnames, and reverse-proxy domains. Its Tauri CSP therefore
allows outbound image, media, and API connections over those two schemes; scripts and
all other resource classes remain restricted to the packaged application.
