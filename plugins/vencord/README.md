# SoundGIF for Vencord

These installers create or update a Vencord source build, copy SoundGIF into its `userplugins`
folder, rebuild Vencord, and run Vencord's injector. Run the same installer again to update or
repair the custom build.

## Windows

Double-click `install-windows.cmd`.

## macOS

Double-click `install-macos.command`. If macOS opens it as text, right-click it, choose **Open
With > Terminal**, and run it again.

## Linux

Open this folder in a terminal and run:

```sh
bash install-macos-linux.sh
```

The scripts use an existing Vencord source checkout from `Documents/Vencord` or `~/Vencord` when
one exists. Otherwise they create a managed checkout in the current user's application-data
folder. Git, Node.js, and pnpm are required.

The Vencord installer asks which Discord client to patch. Restart Discord afterward, open
Vencord's plugin settings, and enable **SoundGIF**.

For Vesktop, run the script with `--no-inject`, then point Vesktop's **Vencord Location** setting
to the generated Vencord `dist` folder.
