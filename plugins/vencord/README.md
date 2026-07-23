# SoundGIF for Vencord

Each installer is standalone. It opens a menu with these choices:

1. Install or update now.
2. Install or update and enable automatic updates/repatching.
3. Disable automatic updates/repatching.

The automatic option checks at sign-in and every 15 minutes. It pulls new SoundGIF and Vencord
source, rebuilds only when either source changed, and checks whether the chosen Discord branch is
still patched. If Discord replaced the patch, it runs Vencord's repair command without asking for
the branch again.

## Windows

Download and double-click `install-windows.cmd`.

## macOS

Download and double-click `install-macos.command`. If macOS opens it as text, run
`bash install-macos.command` in Terminal.

## Linux

Open this folder in a terminal and run:

```sh
bash install-linux.sh
```

Each file works without the rest of this archive. It uses an existing Vencord source checkout from
`Documents/Vencord` or `~/Vencord` when one exists. Otherwise it creates a managed checkout in the
current user's application-data folder. Git, Node.js, and pnpm are required.

Automatic checks use a per-user scheduled task on Windows, a LaunchAgent on macOS, or a systemd
user timer on Linux. Logs are stored in the SoundGIF application-data folder as
`auto-update.log`. Use option 3 in the installer to remove the scheduled job.

The Vencord installer asks which Discord client to patch during a normal one-time install.
Automatic setup asks for Stable, PTB, or Canary in the SoundGIF menu. Restart Discord afterward,
open Vencord's plugin settings, and enable **SoundGIF**.

For Vesktop, set `SOUNDGIF_NO_INJECT=1` before running the installer, then point Vesktop's
**Vencord Location** setting to the generated Vencord `dist` folder.
