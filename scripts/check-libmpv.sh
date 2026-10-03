#!/usr/bin/env bash
# Show which libmpv a development machine would load.
# Does not download, copy, or link a library. Distro builds are often GPL
# and are for local tests only. See docs/architecture/playback.md.
set -euo pipefail

if [[ -n "${MATINEE_LIBMPV:-}" ]]; then
  if [[ -f "$MATINEE_LIBMPV" ]]; then
    echo "MATINEE_LIBMPV=$MATINEE_LIBMPV"
    exit 0
  fi
  echo "MATINEE_LIBMPV is set but not a file: $MATINEE_LIBMPV" >&2
  exit 1
fi

case "$(uname -s)" in
  Linux)
    if command -v ldconfig >/dev/null 2>&1 && ldconfig -p | grep -q 'libmpv.so.2'; then
      ldconfig -p | grep 'libmpv.so.2'
      exit 0
    fi
    ;;
  Darwin)
    for name in libmpv.2.dylib libmpv.dylib; do
      if [[ -f "/opt/homebrew/lib/$name" || -f "/usr/local/lib/$name" ]]; then
        echo "$name"
        exit 0
      fi
    done
    ;;
  MINGW*|MSYS*|CYGWIN*)
    echo "Set MATINEE_LIBMPV to libmpv-2.dll" >&2
    exit 1
    ;;
esac

echo "libmpv was not found. Set MATINEE_LIBMPV to an LGPL build." >&2
exit 1
