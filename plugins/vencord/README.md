# SoundGIF for Vencord

Each installer is standalone. It downloads or updates the required source, copies SoundGIF into
Vencord's `userplugins` folder, rebuilds Vencord, and runs Vencord's injector. Run the same file
again to update or repair the custom build.

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

The Vencord installer asks which Discord client to patch. Restart Discord afterward, open
Vencord's plugin settings, and enable **SoundGIF**.

For Vesktop, set `SOUNDGIF_NO_INJECT=1` before running the installer, then point Vesktop's
**Vencord Location** setting to the generated Vencord `dist` folder.
