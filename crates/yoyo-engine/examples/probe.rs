//! Prints a media file's duration as mpv reports it: `cargo run -p yoyo-engine --example probe -- <file>`
use yoyo_engine::{Event, Player, find_libmpv};

fn main() {
    let path = std::env::args().nth(1).expect("usage: probe <file>");
    let p = Player::new(&find_libmpv()).expect("start mpv");
    p.set("vo", "null").ok();
    p.set("ao", "null").ok();
    p.set("pause", "yes").ok();
    let (tx, rx) = std::sync::mpsc::channel();
    let _events = p.spawn_events(move |e| {
        let _ = tx.send(e);
    });
    p.command(&["loadfile", &path]).expect("loadfile");
    while let Ok(e) = rx.recv() {
        if e == Event::FileLoaded {
            break;
        }
    }
    let d = p.get_f64("duration").unwrap_or(-1.0);
    println!("duration = {d:.3} s  (floor {:02}:{:02}, round {:02}:{:02})", (d as u64) / 60, (d as u64) % 60, (d.round() as u64) / 60, (d.round() as u64) % 60);
    p.quit();
}
