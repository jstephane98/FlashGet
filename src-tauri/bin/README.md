# ffmpeg sidecar binary

This directory must contain the ffmpeg binary that is bundled as a **Tauri sidecar**
(declared as `"externalBin": ["bin/ffmpeg"]` in `tauri.conf.json`).

## Required file

Tauri resolves a sidecar named `bin/ffmpeg` to a platform-specific file
`ffmpeg-<host-target-triple>[.exe]`. On this machine the host triple is:

```
x86_64-pc-windows-msvc
```

So you must place the following file here before running `tauri build` or any
smoke-test that invokes the remuxer:

```
src-tauri/bin/ffmpeg-x86_64-pc-windows-msvc.exe
```

(On other platforms, name it `ffmpeg-<triple>` matching `rustc -Vv` `host:`, e.g.
`ffmpeg-x86_64-unknown-linux-gnu`, `ffmpeg-aarch64-apple-darwin`, ...)

## Where to get it

Use a **static LGPL** Windows build of ffmpeg, for example:

- gyan.dev: https://www.gyan.dev/ffmpeg/builds/ (the "release-essentials" LGPL build)
- BtbN: https://github.com/BtbN/FFmpeg-Builds/releases (pick an `lgpl` static build)

Download, extract `ffmpeg.exe`, and rename/copy it to the filename above.

## Notes

- The actual binary is **large** and is **git-ignored** (see `src-tauri/.gitignore`):
  only this `README.md` is committed; `bin/ffmpeg-*` is excluded.
- `cargo build` / `cargo check` do **not** require this binary — `externalBin` is
  only validated at `tauri build` / bundle time.
- Only the container-remux feature (`-c copy`) needs ffmpeg; the segment
  concatenation is done in the core crate.
