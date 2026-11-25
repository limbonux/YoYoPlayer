# Third-party software

YoYo Player is licensed under GPL-3.0-or-later (see `LICENSE`). It is built on:

| Component | License | Source |
|---|---|---|
| libmpv (mpv) | GPL-2.0-or-later / LGPL-2.1-or-later, depending on the build | https://github.com/mpv-player/mpv |
| FFmpeg and the codec libraries inside the libmpv build (x264, x265 and others) | GPL / LGPL and their own licenses | https://ffmpeg.org |
| The Windows libmpv build | built by the mpv-winbuild-cmake project; exact archive and SHA-256 in `scripts/fetch-deps.ps1` | https://github.com/shinchiro/mpv-winbuild-cmake |
| Slint | GPL-3.0-only (one of its license options) | https://github.com/slint-ui/slint |
| Rust crates | see `Cargo.lock` and each crate | https://crates.io |

The YoYo Player logo and icons were drawn for this project and share its license.
