<p align="center">
  <img src="docs/presentation/assets/flashget-256.png" alt="" width="96" height="96">
</p>

<h1 align="center">FlashGet</h1>

<p align="center">
  The download manager for Windows.<br>
  Multiple connections per file, videos from over a thousand sites, automatic resume, queue, scheduler and browser extension.
</p>

<p align="center">
  <a href="https://github.com/jstephane98/FlashGet/releases/latest"><b>Download for Windows</b></a>
  &nbsp;·&nbsp;
  <a href="docs/presentation/FlashGet-presentation.html">Presentation page</a>
  &nbsp;·&nbsp;
  <a href="https://github.com/jstephane98/FlashGet/releases">Release notes</a>
  &nbsp;·&nbsp;
  <a href="https://buymeacoffee.com/joelkamo">Buy me a coffee</a>
</p>

<p align="center">
  <img src="docs/presentation/assets/hero-telechargements.webp" alt="FlashGet running three downloads, each one on eight connections" width="900">
</p>

> FlashGet's interface is in French for now. This README and the presentation page are in English.

## What FlashGet does

**Fast downloads**
- Each file is split into several parts downloaded in parallel, up to 32 connections.
- After a dropped connection or a server error, FlashGet retries with a growing delay and resumes where it stopped.
- Global speed limit, many links pasted at once, and a destination folder per file type.

**Video and audio**
- Over a thousand sites through [yt-dlp](https://github.com/yt-dlp/yt-dlp), updated automatically and verified with a SHA-256 checksum.
- Pick the quality before downloading, audio only as MP3, M4A or Opus, subtitles, clips, whole playlists.
- HLS and DASH streams, on demand or live (recorded while they air).

**Stay organized**
- A "Completed" library as a list or a grid, with thumbnails, durations and a built-in player.
- A queue you reorder by drag and drop, with priorities.
- A scheduler: time window, days, speed limit, then sleep or shut down when everything is done.

**Browser extension** (Chrome, Edge, Brave)
- Browser downloads land in FlashGet with the cookies and the originating page.
- Real connection status, ignored sites shared with the app, and "Retry" that resends the capture context.

**Day to day**
- Dark, light or system theme. System tray, notifications on success and failure, launch at startup.
- Signed FlashGet updates: the app lets you know, nothing installs without your click.

<p align="center">
  <img src="docs/presentation/assets/nouveau.webp" alt="The New page: formats found for a video, with the recommended quality" width="440">
  <img src="docs/presentation/assets/bibliotheque.webp" alt="The Completed library as a thumbnail grid" width="440">
</p>

## Install

1. Download `FlashGet_<version>_x64-setup.exe` from the [latest release](https://github.com/jstephane98/FlashGet/releases/latest).
2. Run it. It installs for your account only, without administrator rights. Windows SmartScreen may show a warning because the installer isn't signed with a Windows code-signing certificate: choose "More info", then "Run anyway".
3. For the extension, open FlashGet, go to **Paramètres › Extension navigateur** (Settings › Browser extension) and follow the three steps. The extension folder ships with FlashGet.

Requirements: Windows 10 or 11, 64-bit.

**Coming from FlashGet 0.1.0?** Close it before installing 2.0. On first launch, your downloads, settings, extension token and YouTube sign-in are carried over automatically. Then reload the extension from your browser's extensions page.

## Support the project

FlashGet is free and built by one person. If it saves you time, you can [buy me a coffee](https://buymeacoffee.com/joelkamo). Bug reports and ideas are welcome in the [issues](https://github.com/jstephane98/FlashGet/issues).

<p align="center">
  <a href="https://buymeacoffee.com/joelkamo"><img src="docs/presentation/assets/bmc-thanks.gif" alt="Thanks for your support" width="180"></a>
  &nbsp;&nbsp;&nbsp;&nbsp;
  <a href="https://buymeacoffee.com/joelkamo"><img src="docs/presentation/assets/bmc-qr.png" alt="QR code to buymeacoffee.com/joelkamo" width="143" height="141"></a>
</p>

<p align="center">
  <a href="https://buymeacoffee.com/joelkamo"><img src="https://img.shields.io/badge/Buy%20me%20a%20coffee-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black" alt="Buy me a coffee"></a>
</p>

## Build from source

### Prerequisites
- [Node.js](https://nodejs.org/) 20 or newer.
- [Rust](https://www.rust-lang.org/tools/install) (stable) and the [Tauri 2 prerequisites for Windows](https://v2.tauri.app/start/prerequisites/) (Microsoft C++ Build Tools, WebView2).
- The two bundled programs, placed in `src-tauri/bin/` (they are not in the repository):
  - `ffmpeg-x86_64-pc-windows-msvc.exe`: see [src-tauri/bin/README.md](src-tauri/bin/README.md);
  - `yt-dlp-x86_64-pc-windows-msvc.exe`: the `yt-dlp.exe` from the [latest yt-dlp release](https://github.com/yt-dlp/yt-dlp/releases/latest), renamed.

### Commands

```powershell
npm install
npm run tauri dev
```

| Command | Purpose |
|---|---|
| `npm run tauri dev` | Runs FlashGet in development mode |
| `npm test` | UI and extension tests (Vitest) |
| `npx tsc --noEmit` | TypeScript type check |
| `cargo test --workspace` (in `src-tauri`) | Engine and app tests (Rust) |
| `npm run release` | Builds the signed installer and `latest.json` (see [docs/RELEASE.md](docs/RELEASE.md)) |

On a machine with little memory, limit parallelism: `cargo test --workspace -j 1` and `npm test -- --maxWorkers=2`.

### Publishing a release

The full procedure (signing key, environment variables, upload to GitHub Releases) is described in [docs/RELEASE.md](docs/RELEASE.md) (in French).

## Repository layout

| Folder | Contents |
|---|---|
| `src/` | React 19 + TypeScript + Tailwind interface |
| `src-tauri/src/` | Tauri 2 app: commands, queue, scheduler, extension bridge, updates |
| `src-tauri/crates/download-core/` | Download engine in Rust, independent of Tauri: segments, resume, HLS and DASH, SQLite storage |
| `extension/` | Browser extension (Manifest V3) |
| `scripts/` | Release script and logo rendering (`scripts/brand/`) |
| `docs/` | Specs, plans, release guide and presentation page |

## Bundled components

FlashGet ships with [yt-dlp](https://github.com/yt-dlp/yt-dlp) and [FFmpeg](https://ffmpeg.org/). Screenshots in this repository show demo data; the *Big Buck Bunny* image is © Blender Foundation, licensed under [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/).
