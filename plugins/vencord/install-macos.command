#!/usr/bin/env bash

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
bash "$SCRIPT_DIR/install-macos-linux.sh" "$@"
EXIT_CODE=$?

echo
if [ "$EXIT_CODE" -ne 0 ]; then
    echo "SoundGIF installation failed."
else
    echo "SoundGIF installation finished."
fi
read -r -p "Press Return to close."
exit "$EXIT_CODE"
