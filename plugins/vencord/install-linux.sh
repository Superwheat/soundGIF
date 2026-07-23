#!/usr/bin/env bash
set -euo pipefail

SOUNDGIF_REPOSITORY="https://github.com/Superwheat/soundGIF.git"
VENCORD_REPOSITORY="https://github.com/Vendicated/Vencord.git"
MANAGED_ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/soundgif"
VENCORD_DIR="${VENCORD_DIR:-}"
SOUNDGIF_CHECKOUT="$MANAGED_ROOT/source"
MANAGED_VENCORD="$MANAGED_ROOT/Vencord"

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
    "$@"
}

require_command git
require_command node

if command -v pnpm >/dev/null 2>&1; then
    PNPM=(pnpm)
elif command -v corepack >/dev/null 2>&1; then
    PNPM=(corepack pnpm)
else
    echo "pnpm is required: https://pnpm.io/installation" >&2
    exit 1
fi

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
    mkdir -p "$(dirname "$VENCORD_DIR")"
    run git clone "$VENCORD_REPOSITORY" "$VENCORD_DIR"
else
    run git -C "$VENCORD_DIR" pull --ff-only
fi

if [ ! -d "$SOUNDGIF_CHECKOUT/.git" ]; then
    mkdir -p "$(dirname "$SOUNDGIF_CHECKOUT")"
    run git clone --depth 1 "$SOUNDGIF_REPOSITORY" "$SOUNDGIF_CHECKOUT"
else
    run git -C "$SOUNDGIF_CHECKOUT" pull --ff-only
fi

PLUGIN_SOURCE="$SOUNDGIF_CHECKOUT/plugins/vencord/soundGif"
USERPLUGINS="$VENCORD_DIR/src/userplugins"
PLUGIN_TARGET="$USERPLUGINS/soundGif"

[ -f "$PLUGIN_SOURCE/index.tsx" ] || {
    echo "SoundGIF plugin source was not found after updating." >&2
    exit 1
}
[ -f "$VENCORD_DIR/package.json" ] || {
    echo "The selected folder is not a Vencord source checkout." >&2
    exit 1
}
case "$PLUGIN_TARGET" in
    "$VENCORD_DIR"/src/userplugins/soundGif) ;;
    *)
        echo "Refusing to replace an unexpected plugin path: $PLUGIN_TARGET" >&2
        exit 1
        ;;
esac

printf 'Installing plugin: %s\n' "$PLUGIN_TARGET"
mkdir -p "$USERPLUGINS"
rm -rf -- "$PLUGIN_TARGET"
cp -R "$PLUGIN_SOURCE" "$PLUGIN_TARGET"

run "${PNPM[@]}" --dir "$VENCORD_DIR" install --frozen-lockfile
run "${PNPM[@]}" --dir "$VENCORD_DIR" build

if [ "${SOUNDGIF_NO_INJECT:-0}" != "1" ]; then
    echo
    echo "Vencord's installer will ask which Discord client to patch."
    run "${PNPM[@]}" --dir "$VENCORD_DIR" inject
fi

echo
echo "Restart Discord, then enable SoundGIF in Vencord's plugin settings."
echo "Run this file again to update or repair the custom Vencord build."
