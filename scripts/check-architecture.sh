#!/usr/bin/env bash
# Enforces the layering rules in AGENTS.md and docs/architecture/overview.md.
# Run from anywhere: scripts/check-architecture.sh
set -euo pipefail

cd "$(dirname "$0")/.."
status=0

fail() {
  echo "error: $*" >&2
  status=1
}

# 1. The generic framework knows nothing about Matinee or its domain.
product_terms='matinee|jellyfin|radarr|sonarr|poster|midnight navy|ticket cream|marquee|curtain burgundy|faded teal|fraunces|manrope|plex mono'
if matches=$(grep -rniE "$product_terms" crates/atelier-ui crates/atelier-app); then
  fail "product concepts found in framework crates:"
  echo "$matches" >&2
fi

# 2. Video belongs in matinee-player, never in the UI framework.
if matches=$(grep -rniE 'video|playback|libmpv|gstreamer|ffmpeg' crates/atelier-ui); then
  fail "playback concepts found in atelier-ui:"
  echo "$matches" >&2
fi

# 3. Only the framework crates may depend on GPUI directly.
for manifest in crates/*/Cargo.toml apps/*/Cargo.toml; do
  case "$manifest" in
    crates/atelier-ui/Cargo.toml | crates/atelier-app/Cargo.toml) continue ;;
  esac
  if grep -qE '^\s*gpui(\.workspace)?\s*=' "$manifest"; then
    fail "$manifest depends on gpui directly; use atelier-ui / atelier-app APIs"
  fi
done

# 4. Layer direction: ui <- app <- product. Never the reverse.
if grep -qE '^\s*(atelier-app|matinee-[a-z]+)' crates/atelier-ui/Cargo.toml; then
  fail "atelier-ui must not depend on atelier-app or Matinee crates"
fi
if grep -qE '^\s*matinee-[a-z]+' crates/atelier-app/Cargo.toml; then
  fail "atelier-app must not depend on Matinee crates"
fi

# 5. OS checks live only in atelier-app's platform module.
if matches=$(grep -RInE 'cfg!\(\s*target_os|cfg\(\s*target_os' \
  crates/atelier-ui/src crates/atelier-app/src --include='*.rs' \
  | grep -v '^crates/atelier-app/src/platform.rs:'); then
  fail "cfg(target_os) outside crates/atelier-app/src/platform.rs:"
  echo "$matches" >&2
fi

# 6. Playback does not depend on the UI stack, and foreign calls stay in one module.
if grep -qE '^\s*(gpui|atelier-ui|atelier-app|matinee-ui)(\s|\.)' crates/matinee-player/Cargo.toml; then
  fail "matinee-player must not depend on gpui, atelier-ui, atelier-app, or matinee-ui"
fi
if matches=$(grep -RInE 'unsafe[[:space:]]*(impl|fn|extern|\{)' \
  crates/matinee-player/src --include='*.rs' \
  | grep -v '^crates/matinee-player/src/engine/ffi.rs:'); then
  fail "unsafe outside crates/matinee-player/src/engine/ffi.rs:"
  echo "$matches" >&2
fi

# 7. Domain direction. Atelier stays free of Matinee crates. matinee-core
#    does not speak HTTP or know the player. matinee-jellyfin may depend on
#    matinee-core and must not depend on GPUI, Atelier, matinee-ui, or the
#    player. matinee-player stays independent of Jellyfin and the domain.
dependency_names() {
  awk '
    /^\[(dependencies|dev-dependencies|build-dependencies)\]/ { on = 1; next }
    /^\[/ { on = 0 }
    on && $0 ~ /^[A-Za-z0-9_-]+/ {
      name = $1
      sub(/\..*/, "", name)
      print name
    }
  ' "$1"
}

depends_on() {
  dependency_names "$1" | grep -qx "$2"
}

forbid_deps() {
  local manifest="$1"
  shift
  local name
  for name in "$@"; do
    if depends_on "$manifest" "$name"; then
      fail "$manifest must not depend on $name"
    fi
  done
}

forbid_deps crates/atelier-ui/Cargo.toml matinee-core matinee-jellyfin matinee-player matinee-ui
forbid_deps crates/atelier-app/Cargo.toml matinee-core matinee-jellyfin matinee-player matinee-ui
forbid_deps crates/matinee-core/Cargo.toml gpui atelier-ui atelier-app matinee-ui matinee-player matinee-jellyfin reqwest
forbid_deps crates/matinee-jellyfin/Cargo.toml gpui atelier-ui atelier-app matinee-ui matinee-player keyring
if ! depends_on crates/matinee-jellyfin/Cargo.toml matinee-core; then
  fail "matinee-jellyfin must depend on matinee-core"
fi
if ! depends_on crates/matinee-jellyfin/Cargo.toml matinee-secrets; then
  fail "matinee-jellyfin must depend on matinee-secrets for persistent sessions"
fi
forbid_deps crates/matinee-player/Cargo.toml gpui atelier-ui atelier-app matinee-ui matinee-jellyfin matinee-core
forbid_deps crates/matinee-secrets/Cargo.toml gpui atelier-ui atelier-app matinee-ui matinee-core matinee-jellyfin matinee-player matinee-integrations matinee-studio tauri
forbid_deps crates/matinee-integrations/Cargo.toml gpui atelier-ui atelier-app matinee-ui matinee-player matinee-jellyfin matinee-core matinee-studio tauri
if ! depends_on crates/matinee-integrations/Cargo.toml matinee-secrets; then
  fail "matinee-integrations must depend on matinee-secrets"
fi
forbid_deps crates/matinee-studio/Cargo.toml gpui atelier-ui atelier-app matinee-ui matinee-player matinee-jellyfin matinee-integrations matinee-core tauri
if grep -qE '^[[:space:]]*keyring([[:space:]]|\.)' src-tauri/Cargo.toml; then
  fail "src-tauri must not depend on keyring; credentials go through matinee-secrets"
fi
if ! depends_on crates/matinee-studio/Cargo.toml matinee-secrets; then
  fail "matinee-studio must depend on matinee-secrets"
fi
for crate in matinee-secrets matinee-integrations matinee-studio; do
  if ! grep -q "../crates/$crate" src-tauri/Cargo.toml; then
    fail "src-tauri must depend on $crate"
  fi
  if grep -q 'src-tauri' "crates/$crate/Cargo.toml"; then
    fail "crates/$crate must not depend on src-tauri"
  fi
done

# 8. The shipping app and spikes stay outside the workspace.
for path in src-tauri spikes; do
  if ! grep -qE "exclude = \[.*\"$path\"" Cargo.toml; then
    fail "root Cargo.toml must exclude \"$path\" from the workspace"
  fi
done

# 9. One capability declaration. The player does not build a device profile.
#    The domain crate does not speak HTTP or name a media server. Neither
#    domain nor network crate names the UI framework.
if matches=$(grep -RInE 'DeviceProfile|native_device_profile|TranscodingUrl|DirectPlayProfiles|TranscodingProfiles|SubtitleProfiles|serde_json' \
  crates/matinee-player/src crates/matinee-player/tests --include='*.rs'); then
  fail "device-profile or server JSON construction found in matinee-player:"
  echo "$matches" >&2
fi
if matches=$(grep -RInE 'reqwest|DeviceProfile|TranscodingUrl|api_key|MediaBrowser|Jellyfin|jellyfin' \
  crates/matinee-core/src --include='*.rs'); then
  fail "HTTP or media-server concepts found in matinee-core:"
  echo "$matches" >&2
fi
if matches=$(grep -RIniE 'gpui|atelier' \
  crates/matinee-core/src crates/matinee-jellyfin/src --include='*.rs'); then
  fail "UI framework names found in matinee-core or matinee-jellyfin:"
  echo "$matches" >&2
fi

# 10. The native UI does not speak HTTP itself. One application runtime
#     owns Tokio. Screens do not block on it and do not construct another.
if matches=$(grep -RInE '\breqwest\b' apps/matinee-next --include='*.rs' --include='Cargo.toml'); then
  fail "matinee-next must not name reqwest; call matinee-jellyfin on the service runtime:"
  echo "$matches" >&2
fi
# Service crates may build a runtime for their own tests. The UI and the
# native window may not. The application constructs one in runtime.rs.
if matches=$(grep -RIn 'tokio::runtime' \
  apps/matinee-next crates/atelier-ui crates/atelier-app crates/matinee-ui \
  --include='*.rs' | grep -v '^apps/matinee-next/src/runtime.rs:'); then
  fail "Tokio runtime setup is owned by apps/matinee-next/src/runtime.rs:"
  echo "$matches" >&2
fi
if matches=$(grep -RInE '\bblock_on\b' \
  apps/matinee-next crates/atelier-ui crates/atelier-app crates/matinee-ui --include='*.rs'); then
  fail "block_on in the UI layer:"
  echo "$matches" >&2
fi

# 11. The application plays through matinee-player. It does not load libmpv
#     itself. The user-facing sentence may still name that library.
if matches=$(grep -RInE 'libloading|mpv_|libmpv_sys|matinee_player::engine' apps --include='*.rs'); then
  fail "application code must use the public matinee-player API, not libmpv:"
  echo "$matches" >&2
fi

# 12. Native artwork carries no token. The application builds artwork
#     addresses with ArtworkUrls::image_request, item_request, or
#     person_request and fetches them only through its artwork loader, which
#     sends the session header. The api_key builders and the raw token stay
#     out of the native app.
if matches=$(grep -RInE '\.access_token\(\)|(urls|artwork\(\))\.(image|backdrop|backdrop_image|chapter_image|user_image|person_image)\(' \
  apps/matinee-next/src --include='*.rs'); then
  fail "token-bearing artwork URLs or the raw access token in matinee-next:"
  echo "$matches" >&2
fi
if matches=$(grep -RIn 'fetch_artwork' apps/matinee-next/src --include='*.rs' \
  | grep -v '^apps/matinee-next/src/artwork.rs:'); then
  fail "artwork is fetched only by apps/matinee-next/src/artwork.rs:"
  echo "$matches" >&2
fi

# 13. Playback planning and reporting belong to the Player. Other screens
#     open it; they do not choose sources or report progress themselves.
if matches=$(grep -RInE '\b(playback_plan|report_playback)\b' apps/matinee-next/src --include='*.rs' \
  | grep -v '^apps/matinee-next/src/player/'); then
  fail "playback planning or reporting outside apps/matinee-next/src/player:"
  echo "$matches" >&2
fi

if [ "$status" -eq 0 ]; then
  echo "architecture boundaries OK"
fi
exit "$status"
