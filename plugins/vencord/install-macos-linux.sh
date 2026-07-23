#!/usr/bin/env bash
set -euo pipefail

SOUNDGIF_REPOSITORY="https://github.com/Superwheat/soundGIF.git"
VENCORD_REPOSITORY="https://github.com/Vendicated/Vencord.git"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BUNDLED_PLUGIN="$SCRIPT_DIR/soundGif"
VENCORD_DIR="${VENCORD_DIR:-}"
NO_INJECT=0
DRY_RUN=0

usage() {
    cat <<'EOF'
Usage: install-macos-linux.sh [--vencord-dir PATH] [--no-inject] [--dry-run]

Installs or updates SoundGIF in a Vencord source build, rebuilds Vencord, and
runs Vencord's injector. Set --no-inject for Vesktop or build-only use.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --vencord-dir)
            [ "$#" -ge 2 ] || { echo "Missing path after --vencord-dir." >&2; exit 2; }
            VENCORD_DIR="$2"
            shift 2
            ;;
        --no-inject)
            NO_INJECT=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "$1 is required and was not found in PATH." >&2
        exit 1
    }
}

run() {
    printf '> '
    printf '%q ' "$@"
    printf '\n'
    if [ "$DRY_RUN" -eq 0 ]; then
        "$@"
    fi
}

require_command git
require_command node

if command -v pnpm >/dev/null 2>&1; then
    PNPM=(pnpm)
elif command -v corepack >/dev/null 2>&1; then
    PNPM=(corepack pnpm)
else
    echo "pnpm is required. Install it from https://pnpm.io/installation and run this file again." >&2
    exit 1
fi

case "$(uname -s)" in
    Darwin)
        MANAGED_ROOT="$HOME/Library/Application Support/SoundGIF"
        ;;
    Linux)
        MANAGED_ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/soundgif"
        ;;
    *)
        echo "This installer supports macOS and Linux." >&2
        exit 1
        ;;
esac

SOUNDGIF_CHECKOUT="$MANAGED_ROOT/source"
MANAGED_VENCORD="$MANAGED_ROOT/Vencord"

if [ -z "$VENCORD_DIR" ]; then
    for candidate in "$HOME/Documents/Vencord" "$HOME/Vencord" "$MANAGED_VENCORD"; do
        if [ -f "$candidate/package.json" ]; then
            VENCORD_DIR="$candidate"
            break
        fi
    done
fi
VENCORD_DIR="${VENCORD_DIR:-$MANAGED_VENCORD}"
printf 'Vencord source: %s\n' "$VENCORD_DIR"

if [ ! -d "$VENCORD_DIR/.git" ]; then
    if [ -d "$VENCORD_DIR" ] && [ -n "$(ls -A "$VENCORD_DIR" 2>/dev/null)" ]; then
        echo "$VENCORD_DIR exists but is not a Vencord Git checkout." >&2
        exit 1
    fi
    if [ "$DRY_RUN" -eq 0 ]; then
        mkdir -p "$(dirname "$VENCORD_DIR")"
    fi
    run git clone "$VENCORD_REPOSITORY" "$VENCORD_DIR"
else
    run git -C "$VENCORD_DIR" pull --ff-only
fi

SOURCE_UPDATED=0
if [ "$DRY_RUN" -eq 1 ]; then
    echo "> git clone or update $SOUNDGIF_REPOSITORY"
elif [ -d "$SOUNDGIF_CHECKOUT/.git" ]; then
    if git -C "$SOUNDGIF_CHECKOUT" pull --ff-only; then
        SOURCE_UPDATED=1
    else
        echo "Could not update SoundGIF source; trying the bundled copy." >&2
    fi
else
    mkdir -p "$(dirname "$SOUNDGIF_CHECKOUT")"
    if git clone --depth 1 "$SOUNDGIF_REPOSITORY" "$SOUNDGIF_CHECKOUT"; then
        SOURCE_UPDATED=1
    else
        echo "Could not download SoundGIF source; trying the bundled copy." >&2
    fi
fi

CACHED_PLUGIN="$SOUNDGIF_CHECKOUT/plugins/vencord/soundGif"
if [ "$SOURCE_UPDATED" -eq 1 ] && [ -f "$CACHED_PLUGIN/index.tsx" ]; then
    PLUGIN_SOURCE="$CACHED_PLUGIN"
elif [ -f "$BUNDLED_PLUGIN/index.tsx" ]; then
    PLUGIN_SOURCE="$BUNDLED_PLUGIN"
    echo "Using the SoundGIF source bundled with this installer."
elif [ "$DRY_RUN" -eq 1 ]; then
    PLUGIN_SOURCE="$CACHED_PLUGIN"
else
    echo "No SoundGIF plugin source is available." >&2
    exit 1
fi

USERPLUGINS="$VENCORD_DIR/src/userplugins"
PLUGIN_TARGET="$USERPLUGINS/soundGif"
case "$PLUGIN_TARGET" in
    "$VENCORD_DIR"/src/userplugins/soundGif) ;;
    *)
        echo "Refusing to replace an unexpected plugin path: $PLUGIN_TARGET" >&2
        exit 1
        ;;
esac

printf 'Installing plugin: %s\n' "$PLUGIN_TARGET"
if [ "$DRY_RUN" -eq 0 ]; then
    mkdir -p "$USERPLUGINS"
    rm -rf -- "$PLUGIN_TARGET"
    cp -R "$PLUGIN_SOURCE" "$PLUGIN_TARGET"
fi

run "${PNPM[@]}" --dir "$VENCORD_DIR" install --frozen-lockfile
run "${PNPM[@]}" --dir "$VENCORD_DIR" build

if [ "$NO_INJECT" -eq 0 ]; then
    echo
    echo "Vencord's installer will ask which Discord client to patch."
    run "${PNPM[@]}" --dir "$VENCORD_DIR" inject
fi

echo
echo "Restart Discord, then enable SoundGIF in Vencord's plugin settings."
echo "Run this installer again whenever SoundGIF or Vencord needs updating or repair."
