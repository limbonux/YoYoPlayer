//! UI-free logic for YoYo Player: the playlist and the saved settings.

pub mod playlist;
pub mod settings;

pub use playlist::{PlayMode, Playlist};
pub use settings::Settings;

/// File extensions YoYo treats as media when scanning a folder.
pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mkv", "webm", "avi", "mov", "wmv", "flv", "f4v", "ts", "m2ts", "mts", "mpg", "mpeg", "vob", "3gp",
    "3g2", "ogv", "rm", "rmvb", "asf", "divx", "hevc", "y4m", "mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma",
    "ape", "ac3", "dts", "mka", "amr", "aiff",
];

/// Subtitle extensions that are loaded as subtitles when dropped on the window.
pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"];

fn has_ext(path: &std::path::Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| list.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

pub fn is_media(path: &std::path::Path) -> bool {
    has_ext(path, MEDIA_EXTENSIONS)
}

pub fn is_subtitle(path: &std::path::Path) -> bool {
    has_ext(path, SUBTITLE_EXTENSIONS)
}
