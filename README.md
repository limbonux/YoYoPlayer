<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/banner-dark.png">
    <img src="assets/banner-light.png" alt="YoYo Player" width="640">
  </picture>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--or--later-0096FF" alt="License: GPL-3.0-or-later"></a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20x64-0096FF" alt="Windows 10 and 11, x64">
  <img src="https://img.shields.io/badge/Rust%20%2B%20Slint%20%2B%20libmpv-0096FF" alt="Rust, Slint and libmpv">
  <img src="https://img.shields.io/badge/status-alpha-F59E0B" alt="Status: alpha">
</p>

<p align="center">
  <img src="assets/demo.gif" alt="YoYo Player: opening a video, seeking, the playlist, the menus and mini mode" width="800">
</p>

<p align="center">No ads. No tracking. No network access.<br><a href="#العربية">العربية</a></p>

## Features

- **Plays almost everything.** Current FFmpeg through libmpv, with hardware decoding
- **Quiet, dark interface** that stays out of the way of the picture
- **Playlist that thinks like you.** Natural name order (2 before 10), and it plays the rest of the folder
- **Play modes:** single, order, loop, loop one, random
- **Subtitles:** loads the matching file by itself; size and delay keys; drop in an `.srt`
- **Mini mode, always on top, fullscreen**
- **Remembers** your volume and playback speed

### Shortcuts

| Key | Action | Key | Action |
|---|---|---|---|
| `Space` | Play / pause | `Enter` | Fullscreen |
| `←` `→` | Seek 10 s | `Ctrl` + `←` `→` | Seek 30 s |
| `↑` `↓` | Volume | `M` | Mute |
| `C` `X` `R` | Faster / slower / normal | `Shift` + `Enter` | Mini mode |
| `F3` | Playlist | `T` | Always on top |
| `[` `]` | Subtitle delay | `Ctrl` + `O` | Open a file |

## Status

This is an early alpha. Playback and the main screens work. These are **not** done yet: opening a URL, history and resume, seek thumbnails, the toolbox tools (GIF, cut, join, convert, share, bookmarks), settings pages, translations and an installer. HDR, 4K HEVC and AV1 files have not been tested.

## Build from source (Windows)

You need [Rust](https://rustup.rs), the Visual Studio C++ build tools, and [7-Zip](https://www.7-zip.org).

```powershell
powershell -ExecutionPolicy Bypass -File scripts\fetch-deps.ps1   # downloads libmpv and checks its SHA-256
cargo run -p yoyo-app
```

`scripts\release.ps1` builds the release zip. `scripts\cargo-vs.cmd` is only a workaround for machines where `cargo` picks the wrong linker.

## Project layout

| Crate | What it does |
|---|---|
| `crates/yoyo-app` | The window, menus and Slint screens |
| `crates/yoyo-core` | Playlist and settings, with no interface code |
| `crates/yoyo-engine` | The libmpv wrapper and OpenGL rendering |

## Contributing

Bug reports, ideas and pull requests are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[GPL-3.0-or-later](LICENSE). Third-party software is listed in [ATTRIBUTION.md](ATTRIBUTION.md).

---

<div dir="rtl">

## العربية

**YoYo Player** مشغّل فيديو لسطح المكتب: سريع ونظيف، بلا إعلانات ولا تتبّع ولا اتصال بالشبكة. مبني بلغة Rust وواجهة Slint ومحرك libmpv.

- يشغّل أغلب الصيغ مع فك الترميز بالعتاد
- قائمة تشغيل بترتيب طبيعي للأسماء، وتشغيل باقي المجلد تلقائياً
- تحميل الترجمة المطابقة تلقائياً، مع التحكم بحجمها وتوقيتها
- وضع مصغّر، ودائماً في الأعلى، وملء الشاشة

هذه نسخة تجريبية مبكرة. المساهمات مرحّب بها بالعربية أو الإنجليزية.

</div>
