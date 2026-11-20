# ISObubble 🫧

**A bubbly, dark-mode disc installer and game launcher for Windows.**

ISObubble installs disc images (ISO files) the way the real disc would: it mounts the image in a
virtual DVD drive and runs the disc's own setup program. Installed games then appear in a
Steam-style library with a tray menu for quick launching.

[![Latest release](https://img.shields.io/github/v/release/robwalsh147/ISObubble?label=download)](https://github.com/robwalsh147/ISObubble/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-lightgrey)
![Language](https://img.shields.io/badge/built%20with-Rust-orange)

---

## Download

Grab **`ISObubble.exe`** from the [latest release](https://github.com/robwalsh147/ISObubble/releases/latest).

No installer is needed to try it: just run the `.exe`. To add ISObubble to your Start menu and
desktop, see [Install](#install).

**Requirements:** Windows 10 or Windows 11 (64-bit).

---

## Features

- **Install from ISO like a real disc.** The image is mounted in a Windows virtual DVD drive, and the
  disc's own setup runs. You can also copy the disc's contents to a folder instead.
- **Multi-disc sets.** Every disc stays mounted. When setup asks for the next disc, ISObubble swaps it
  into setup's drive and confirms the prompt. You can also merge all discs into one virtual disc so
  setup never asks.
- **Reads each disc first.** Shows a plain-English description of the disc, the setup program, its
  publisher and version, whether it is signed, the file types it contains, and the full file list.
- **Safety scanner.** Uses Microsoft Defender's virus database, plus ISObubble's own checks for
  disguised programs, risky scripts, autorun tricks, code injection, keyloggers and ransomware.
- **Library and launcher.** Play buttons, detection of a game's own launcher, per-game launch options,
  and the ability to add games that are already installed.
- **Per-game display options:**
  - forced borderless windowed mode, on the monitor you choose
  - matching the game's resolution and refresh rate to that monitor
  - HDR detection for each monitor and each game
  - an `.ini` / `.cfg` settings editor that keeps backups
- **Compatibility checker.** Flags DOS and 16-bit programs, old DirectX versions, missing runtimes and
  old copy protection, with one-click fixes such as Windows compatibility modes and DOSBox.
- **Crash logs.** Collects Windows crash reports, crash dumps and game logs for each game, plus
  ISObubble's own activity log.
- **Lives in the tray, like Steam.** Closing the window keeps ISObubble running in the tray. The tray
  tooltip shows progress, and right-clicking lists your games. Only one copy runs at a time, and it
  can optionally start with Windows.
- **Updates from GitHub.** ISObubble checks this repository's Releases, then downloads, virus-checks
  and installs new versions.
- **Optional My Abandonware add-on.** Browse classic games from myabandonware.com. Downloads open in
  your browser and are picked up from your Downloads folder, then scanned and installed.

---

## Install

1. Download `ISObubble.exe` from the [latest release](https://github.com/robwalsh147/ISObubble/releases/latest).
2. Run it, open **Settings & updates**, and press **Install ISObubble on this PC**.
   This adds a Start-menu entry and a desktop icon. You can remove it later from **Settings › Apps**.

---

## Building from source

You'll need [Rust](https://rustup.rs) with the MSVC toolchain on Windows.

```bash
git clone https://github.com/robwalsh147/ISObubble.git
cd ISObubble
cargo build --release
```

The executable is written to `target/release/isobubble.exe`.

The interface is built with [egui / eframe](https://github.com/emilk/egui).

---

## Releasing a new version

1. Raise `version` in `Cargo.toml` (for example, `1.3.0`).
2. Build with `cargo build --release`.
3. Create a GitHub Release tagged `v1.3.0` and attach the built executable, renamed to
   `ISObubble.exe`.

Installed copies pick up new releases automatically.

---

## Using ISObubble responsibly

ISObubble does not include any games or disc images. Only install discs and images that you own or
have the right to use.

---

## Contributing

Bug reports, feature requests and pull requests are welcome. Please open an issue first for larger
changes so we can talk about the approach.

---

## License

ISObubble is free and open source software, released under the [MIT License](LICENSE).

---

*Made by RW.*
