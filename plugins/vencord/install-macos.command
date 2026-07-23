#!/usr/bin/env bash
set -euo pipefail

SOUNDGIF_REPOSITORY="https://github.com/Superwheat/soundGIF.git"
VENCORD_REPOSITORY="https://github.com/Vendicated/Vencord.git"
MANAGED_ROOT="$HOME/Library/Application Support/SoundGIF"
SOUNDGIF_CHECKOUT="$MANAGED_ROOT/source"
MANAGED_VENCORD="$MANAGED_ROOT/Vencord"
AUTO_DIR="$MANAGED_ROOT/auto"
AUTO_SCRIPT="$AUTO_DIR/install-macos.command"
AUTO_LOG="$MANAGED_ROOT/auto-update.log"
LAUNCH_AGENT="$HOME/Library/LaunchAgents/com.superwheat.soundgif-vencord.plist"
INTERACTIVE=1

finish() {
    local exit_code=$?
    trap - EXIT
    if [ "$INTERACTIVE" -eq 1 ]; then
        echo
        if [ "$exit_code" -ne 0 ]; then
            echo "The operation failed."
        fi
        read -r -p "Press Return to close."
    fi
    exit "$exit_code"
}
trap finish EXIT

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "$1 is required and was not found in PATH." >&2
        return 1
    }
}

run() {
    printf '> '
    printf '%q ' "$@"
    printf '\n'
    "$@"
}

setup_tools() {
    require_command git
    require_command node

    if command -v pnpm >/dev/null 2>&1; then
        PNPM=(pnpm)
    elif command -v corepack >/dev/null 2>&1; then
        PNPM=(corepack pnpm)
    else
        echo "pnpm is required: https://pnpm.io/installation" >&2
        return 1
    fi
}

resolve_vencord_dir() {
    VENCORD_DIR="${VENCORD_DIR:-}"
    if [ -z "$VENCORD_DIR" ]; then
        for candidate in "$HOME/Documents/Vencord" "$HOME/Vencord" "$MANAGED_VENCORD"; do
            if [ -f "$candidate/package.json" ]; then
                VENCORD_DIR="$candidate"
                break
            fi
        done
    fi
    VENCORD_DIR="${VENCORD_DIR:-$MANAGED_VENCORD}"
}

update_checkout() {
    local directory="$1"
    local repository="$2"
    local mode="$3"

    if [ ! -d "$directory/.git" ]; then
        if [ -d "$directory" ] && [ -n "$(ls -A "$directory" 2>/dev/null)" ]; then
            echo "$directory exists but is not a Git checkout." >&2
            return 1
        fi
        mkdir -p "$(dirname "$directory")"
        run git clone "$repository" "$directory"
        return
    fi

    if ! run git -C "$directory" pull --ff-only; then
        if [ "$mode" = "auto" ]; then
            echo "Update failed; the automatic check will retry later." >&2
            return 0
        fi
        return 1
    fi
}

run_vencord_installer() {
    local action="$1"
    local branch="$2"
    (
        cd "$VENCORD_DIR"
        run node scripts/runInstaller.mjs -- "--$action" --branch "$branch"
    )
}

is_patched() {
    local branch="$1"
    local app_name="Discord.app"
    case "$branch" in
        ptb) app_name="Discord PTB.app" ;;
        canary) app_name="Discord Canary.app" ;;
    esac

    [ -f "/Applications/$app_name/Contents/Resources/_app.asar" ] ||
        [ -f "$HOME/Applications/$app_name/Contents/Resources/_app.asar" ]
}

perform_update() {
    local mode="$1"
    local branch="${2:-stable}"
    local old_vencord_head=""
    local old_soundgif_head=""
    local new_vencord_head=""
    local new_soundgif_head=""
    local needs_build=0

    setup_tools
    resolve_vencord_dir

    old_vencord_head="$(git -C "$VENCORD_DIR" rev-parse HEAD 2>/dev/null || true)"
    old_soundgif_head="$(git -C "$SOUNDGIF_CHECKOUT" rev-parse HEAD 2>/dev/null || true)"

    printf 'Vencord source: %s\n' "$VENCORD_DIR"
    update_checkout "$VENCORD_DIR" "$VENCORD_REPOSITORY" "$mode"
    update_checkout "$SOUNDGIF_CHECKOUT" "$SOUNDGIF_REPOSITORY" "$mode"

    local plugin_source="$SOUNDGIF_CHECKOUT/plugins/vencord/soundGif"
    local userplugins="$VENCORD_DIR/src/userplugins"
    local plugin_target="$userplugins/soundGif"

    [ -f "$plugin_source/index.tsx" ] || {
        echo "SoundGIF plugin source was not found after updating." >&2
        return 1
    }
    [ -f "$VENCORD_DIR/package.json" ] || {
        echo "The selected folder is not a Vencord source checkout." >&2
        return 1
    }
    case "$plugin_target" in
        "$VENCORD_DIR"/src/userplugins/soundGif) ;;
        *)
            echo "Refusing to replace an unexpected plugin path: $plugin_target" >&2
            return 1
            ;;
    esac

    new_vencord_head="$(git -C "$VENCORD_DIR" rev-parse HEAD)"
    new_soundgif_head="$(git -C "$SOUNDGIF_CHECKOUT" rev-parse HEAD)"
    [ "$mode" != "auto" ] && needs_build=1
    [ "$old_vencord_head" != "$new_vencord_head" ] && needs_build=1
    [ "$old_soundgif_head" != "$new_soundgif_head" ] && needs_build=1
    [ ! -f "$plugin_target/index.tsx" ] && needs_build=1
    ! cmp -s "$plugin_source/index.tsx" "$plugin_target/index.tsx" && needs_build=1
    ! cmp -s "$plugin_source/styles.css" "$plugin_target/styles.css" && needs_build=1

    if [ "$needs_build" -eq 1 ]; then
        printf 'Installing plugin: %s\n' "$plugin_target"
        mkdir -p "$userplugins"
        rm -rf -- "$plugin_target"
        cp -R "$plugin_source" "$plugin_target"
        run "${PNPM[@]}" --dir "$VENCORD_DIR" install --frozen-lockfile
        run "${PNPM[@]}" --dir "$VENCORD_DIR" build
    else
        echo "Source is already current."
    fi

    case "$mode" in
        interactive)
            run "${PNPM[@]}" --dir "$VENCORD_DIR" inject
            ;;
        direct)
            run_vencord_installer install "$branch"
            is_patched "$branch" || {
                echo "Vencord did not patch Discord $branch." >&2
                return 1
            }
            ;;
        auto)
            if is_patched "$branch"; then
                echo "Discord $branch is already patched."
            else
                echo "Discord $branch is not patched. Repairing it now."
                run_vencord_installer repair "$branch"
                is_patched "$branch" || {
                    echo "Vencord did not repair Discord $branch." >&2
                    return 1
                }
            fi
            ;;
    esac
}

choose_branch() {
    echo
    echo "Which Discord version should be kept patched?"
    echo "[1] Stable"
    echo "[2] PTB"
    echo "[3] Canary"
    echo "[4] Cancel"
    echo
    read -r -p "Choose a version: " branch_choice
    case "$branch_choice" in
        1) AUTO_BRANCH="stable" ;;
        2) AUTO_BRANCH="ptb" ;;
        3) AUTO_BRANCH="canary" ;;
        *) return 1 ;;
    esac
}

enable_auto() {
    local branch="$1"
    local auto_script_xml
    local auto_log_xml
    mkdir -p "$AUTO_DIR" "$(dirname "$LAUNCH_AGENT")"
    cp "$0" "$AUTO_SCRIPT"
    chmod +x "$AUTO_SCRIPT"
    auto_script_xml="$(printf '%s' "$AUTO_SCRIPT" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')"
    auto_log_xml="$(printf '%s' "$AUTO_LOG" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')"

    cat > "$LAUNCH_AGENT" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.superwheat.soundgif-vencord</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/bash</string>
        <string>$auto_script_xml</string>
        <string>--auto-run</string>
        <string>$branch</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>StartInterval</key>
    <integer>900</integer>
    <key>StandardOutPath</key>
    <string>$auto_log_xml</string>
    <key>StandardErrorPath</key>
    <string>$auto_log_xml</string>
</dict>
</plist>
EOF

    launchctl bootout "gui/$(id -u)" "$LAUNCH_AGENT" >/dev/null 2>&1 || true
    launchctl bootstrap "gui/$(id -u)" "$LAUNCH_AGENT"
    echo
    echo "Automatic updates and repatching are enabled for Discord $branch."
    echo "The check runs at sign-in and every 15 minutes."
}

disable_auto() {
    launchctl bootout "gui/$(id -u)" "$LAUNCH_AGENT" >/dev/null 2>&1 || true
    rm -f -- "$LAUNCH_AGENT" "$AUTO_SCRIPT"
    echo
    echo "Automatic updates and repatching are disabled."
}

if [ "${1:-}" = "--auto-run" ]; then
    INTERACTIVE=0
    mkdir -p "$MANAGED_ROOT"
    perform_update auto "${2:-stable}" >> "$AUTO_LOG" 2>&1
    exit
fi

while true; do
    clear
    echo "SoundGIF for Vencord"
    echo
    echo "[1] Install or update now"
    echo "[2] Install or update and enable automatic updates/repatching"
    echo "[3] Disable automatic updates/repatching"
    echo "[4] Exit"
    echo
    read -r -p "Choose an option: " menu_choice

    case "$menu_choice" in
        1)
            perform_update interactive stable
            echo
            echo "Done. Restart Discord and enable SoundGIF in Vencord's plugin settings."
            exit
            ;;
        2)
            choose_branch || continue
            perform_update direct "$AUTO_BRANCH"
            enable_auto "$AUTO_BRANCH"
            echo
            echo "Done. Restart Discord and enable SoundGIF in Vencord's plugin settings."
            exit
            ;;
        3)
            disable_auto
            exit
            ;;
        4)
            exit
            ;;
    esac
done
