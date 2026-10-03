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

# 6. The shipping app and spikes stay outside the workspace.
for path in src-tauri spikes; do
  if ! grep -qE "exclude = \[.*\"$path\"" Cargo.toml; then
    fail "root Cargo.toml must exclude \"$path\" from the workspace"
  fi
done

if [ "$status" -eq 0 ]; then
  echo "architecture boundaries OK"
fi
exit "$status"
