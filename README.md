# ISObubble 🫧

A bubbly, dark-mode disc installer and game launcher for Windows. ISObubble installs ISO disc
images the same way the real discs would: it reads each disc, checks it for viruses, mounts it in
a virtual drive and runs the disc's own install wizard. Installed games then go into a
Steam-style library and tray menu.

## Features

- **Install from ISO like the real disc**: the image goes into a Windows virtual DVD drive and the
  disc's own setup runs. You can also copy the disc to a folder instead.
- **Multi-disc sets**: every disc stays mounted. When setup asks for the next disc, ISObubble moves
  that disc into setup's drive and presses OK for you. You can also combine all discs into one
  virtual disc so setup never asks.
- **Reads every disc first**: a plain-English description of what's on it, the setup program, its
  publisher and version, whether it's signed, the file types and the full file list.
- **Safety scanner**: Microsoft Defender's virus database plus ISObubble's own checks for
  disguised programs, dangerous scripts, autorun tricks, code injection, keyloggers and ransomware.
- **Library and launcher**: Play buttons, detection of the game's own launcher, launch options,
  auto-inserting the disc for games that check for it, and adding games that are already installed.
- **Game options**:
  - forced borderless windowed mode, played on the monitor you pick
  - matching the game's resolution and refresh rate to that monitor
  - HDR detection for each monitor and each game
  - an .ini/.cfg settings editor that keeps backups
- **Compatibility checker**: spots DOS and 16-bit programs, old DirectX, missing runtimes and old
  copy protection. One-click fixes include Windows compatibility modes and DOSBox.
- **Crash logs**: Windows crash reports, crash dumps and game logs for each game, plus ISObubble's
  own activity log.
- **Tray, like Steam**: closing the window keeps ISObubble running in the tray. The tooltip shows
  progress, and right-clicking lists your games to launch. Only one copy runs at a time, and it can
  optionally start with Windows.
- **Updates from GitHub**: ISObubble checks this repository's Releases, then downloads,
  virus-checks and installs new versions.
- **Optional My Abandonware add-on**: browse classic games from myabandonware.com. Downloads open
  in your browser and are picked up from your Downloads folder, then scanned and installed.

## Install

1. Download `ISObubble.exe` from the [latest release](https://github.com/robwalsh147/ISObubble/releases/latest).
2. Run it, open **Settings & updates** and press **Install ISObubble on this PC**. This adds a
   Start-menu entry and desktop icon, and it can be uninstalled from Settings › Apps.

Requires Windows 10 or 11 (64-bit).

## Publishing an update

1. Raise `version` in `Cargo.toml` (for example `1.3.0`).
2. Build with `cargo build --release`.
3. Create a GitHub Release tagged `v1.3.0` and attach `target/release/isobubble.exe`, renamed to
   `ISObubble.exe`.

Installed copies pick up the new release automatically.

## Building from source

Install [Rust](https://rustup.rs), then run:

```
cargo build --release
```

The program is written in Rust with [egui/eframe](https://github.com/emilk/egui).
