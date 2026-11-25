//! Reads mpv's track list into menu items for the Sound and Subtitles menus.

use yoyo_engine::Player;

use crate::TrackItem;

pub struct Tracks {
    pub audio: Vec<TrackItem>,
    pub subs: Vec<TrackItem>,
}

pub fn read(p: &Player) -> Tracks {
    let mut out = Tracks { audio: Vec::new(), subs: Vec::new() };
    let count: usize = p.get_string("track-list/count").and_then(|s| s.parse().ok()).unwrap_or(0);
    for i in 0..count {
        let get = |k: &str| p.get_string(&format!("track-list/{i}/{k}"));
        let Some(kind) = get("type") else { continue };
        let id: i32 = get("id").and_then(|s| s.parse().ok()).unwrap_or(0);
        let lang = get("lang").unwrap_or_default();
        let title = get("title").unwrap_or_default();
        let external = get("external").is_some_and(|s| s == "yes");
        let selected = get("selected").is_some_and(|s| s == "yes");
        let mut label = format!("Track {id}");
        if !lang.is_empty() {
            label.push_str(&format!(" · {lang}"));
        }
        if !title.is_empty() {
            label.push_str(&format!(" · {title}"));
        }
        if external {
            label.push_str(" · file");
        }
        let item = TrackItem { id, title: label.into(), selected };
        match kind.as_str() {
            "audio" => out.audio.push(item),
            "sub" => out.subs.push(item),
            _ => {}
        }
    }
    out
}
