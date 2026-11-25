// YoYo Player, Phase 1: the core player.
// Slint draws the interface; libmpv draws the video underneath (OpenGL underlay, ADR 0001).
#![windows_subsystem = "windows"]

mod menus;
mod tracks;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};
use yoyo_core::{PlayMode, Playlist, Settings, is_media, is_subtitle};
use yoyo_engine::{Event, GlRenderer, Player, find_libmpv};

slint::include_modules!();

const MEDIA_FILTER: &[&str] = &[
    "mp4", "m4v", "mkv", "webm", "avi", "mov", "wmv", "flv", "ts", "m2ts", "mts", "mpg", "mpeg", "vob", "3gp", "ogv",
    "rm", "rmvb", "asf", "mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma", "ape", "mka",
];
const SUB_FILTER: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"];
// Logical sizes of the docked bars; must match theme/tokens.slint.
const TITLE_BAR: f32 = 40.0;
const CONTROL_BAR: f32 = 50.0;
const PLAYLIST: f32 = 260.0;
const MINI_SIZE: (f64, f64) = (480.0, 270.0);

fn fmt_hms(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn mode_to_int(m: PlayMode) -> i32 {
    match m {
        PlayMode::Single => 0,
        PlayMode::Order => 1,
        PlayMode::LoopAll => 2,
        PlayMode::LoopOne => 3,
        PlayMode::Random => 4,
    }
}

fn int_to_mode(i: i32) -> PlayMode {
    match i {
        0 => PlayMode::Single,
        2 => PlayMode::LoopAll,
        3 => PlayMode::LoopOne,
        4 => PlayMode::Random,
        _ => PlayMode::Order,
    }
}

struct State {
    player: Player,
    playlist: Playlist,
    settings: Settings,
    fullscreen: bool,
    mini: bool,
    normal_size: Option<winit::dpi::PhysicalSize<u32>>,
    last_activity: Instant,
    margins: [f32; 3],
    rotation: i64,
    eof_handled: bool,
    renderer_ready: bool,
    pending: Vec<PathBuf>,
    dropped: Vec<PathBuf>,
    cursor_hidden: bool,
}

type S = Rc<RefCell<State>>;

struct Ui {
    app: slint::Weak<App>,
    st: S,
    osd_timer: Rc<Timer>,
}

impl Ui {
    fn app(&self) -> Option<App> {
        self.app.upgrade()
    }

    fn osd(&self, text: impl Into<String>, ms: u64) {
        let Some(app) = self.app() else { return };
        app.set_osd_text(text.into().into());
        let weak = self.app.clone();
        self.osd_timer.start(TimerMode::SingleShot, Duration::from_millis(ms), move || {
            if let Some(app) = weak.upgrade() {
                app.set_osd_text("".into());
            }
        });
    }

    fn player(&self) -> Player {
        self.st.borrow().player.clone()
    }

    fn refresh_playlist(&self) {
        let Some(app) = self.app() else { return };
        let st = self.st.borrow();
        let cur = st.playlist.current();
        let active_dur = if app.get_has_file() {
            let d = st.player.get_f64("duration").unwrap_or(0.0);
            if d > 0.0 { fmt_hms(d) } else { String::new() }
        } else {
            String::new()
        };
        let rows: Vec<PlaylistRow> = st
            .playlist
            .items()
            .iter()
            .enumerate()
            .map(|(i, p)| PlaylistRow {
                name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into(),
                duration: if Some(i) == cur && !active_dur.is_empty() { active_dur.clone().into() } else { "00:00:00".into() },
                current: Some(i) == cur,
            })
            .collect();
        app.set_playlist(ModelRc::new(VecModel::from(rows)));
    }

    /// Plays playlist item `index`.
    fn play_index(&self, index: usize) {
        let Some(app) = self.app() else { return };
        let path = {
            let mut st = self.st.borrow_mut();
            let Some(p) = st.playlist.select(index).map(Path::to_path_buf) else { return };
            st.eof_handled = false;
            st.rotation = 0;
            p
        };
        let player = self.player();
        if let Err(e) = player.command(&["loadfile", &path.to_string_lossy()]) {
            self.osd(format!("Can't open this file: {e}"), 4000);
            return;
        }
        let _ = player.set("pause", "no");
        let _ = player.set("video-rotate", "0");
        app.set_has_file(true);
        app.set_file_name(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into());
        self.refresh_playlist();
    }

    /// Opens what the user picked or dropped: files, a folder, subtitles.
    fn open_paths(&self, paths: Vec<PathBuf>) {
        if !self.st.borrow().renderer_ready {
            // mpv needs its render context before a file loads (ADR 0001, pitfall 1).
            self.st.borrow_mut().pending.extend(paths);
            return;
        }
        let (subs, rest): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| is_subtitle(p));
        let media: Vec<PathBuf> = rest.iter().filter(|p| p.is_file() && is_media(p)).cloned().collect();
        let dirs: Vec<PathBuf> = rest.iter().filter(|p| p.is_dir()).cloned().collect();

        let start = {
            let mut st = self.st.borrow_mut();
            if media.len() == 1 && dirs.is_empty() {
                // Fix X03: one file queues its whole folder in natural order.
                st.playlist.open_with_folder(&media[0]);
                st.playlist.current()
            } else if media.is_empty() && dirs.len() == 1 {
                st.playlist.open_folder(&dirs[0]);
                st.playlist.current()
            } else if !media.is_empty() || !dirs.is_empty() {
                st.playlist.clear();
                let mut all = media.clone();
                for d in &dirs {
                    all.extend(yoyo_core::playlist::scan_folder(d));
                }
                st.playlist.add(all)
            } else {
                None
            }
        };
        if let Some(i) = start {
            self.play_index(i);
        } else if !rest.is_empty() && subs.is_empty() {
            self.osd("Nothing playable was found there", 2500);
        }
        if !subs.is_empty() {
            if self.app().is_some_and(|a| a.get_has_file()) {
                for s in &subs {
                    let _ = self.player().command(&["sub-add", &s.to_string_lossy(), "select"]);
                }
                self.osd(format!("Subtitle loaded: {}", subs[0].file_name().unwrap_or_default().to_string_lossy()), 2000);
                self.refresh_tracks();
            } else {
                self.osd("Open a video first, then drop its subtitle", 2500);
            }
        }
    }

    fn refresh_tracks(&self) {
        let Some(app) = self.app() else { return };
        let t = tracks::read(&self.player());
        app.set_sub_selected(t.subs.iter().any(|x| x.selected));
        app.set_audio_tracks(ModelRc::new(VecModel::from(t.audio)));
        app.set_sub_tracks(ModelRc::new(VecModel::from(t.subs)));
    }

    fn set_fullscreen(&self, on: bool) {
        let Some(app) = self.app() else { return };
        if self.st.borrow().mini && on {
            self.toggle_mini();
        }
        app.window().set_fullscreen(on);
        app.set_fullscreen(on);
        self.st.borrow_mut().fullscreen = on;
        self.activity();
    }

    fn toggle_mini(&self) {
        let Some(app) = self.app() else { return };
        let mini = !self.st.borrow().mini;
        if mini && self.st.borrow().fullscreen {
            self.set_fullscreen(false);
        }
        app.set_mini(mini);
        self.st.borrow_mut().mini = mini;
        let prev = self.st.borrow().normal_size;
        app.window().with_winit_window(|w| {
            if mini {
                self.st.borrow_mut().normal_size = Some(w.inner_size());
                let _ = w.request_inner_size(winit::dpi::LogicalSize::new(MINI_SIZE.0, MINI_SIZE.1));
            } else if let Some(size) = prev {
                let _ = w.request_inner_size(size);
            }
        });
        self.activity();
    }

    /// Mouse moved: show the controls again.
    fn activity(&self) {
        let mut st = self.st.borrow_mut();
        st.last_activity = Instant::now();
        if let Some(app) = self.app.upgrade() {
            app.set_controls_visible(true);
            if st.cursor_hidden {
                app.window().with_winit_window(|w| w.set_cursor_visible(true));
                st.cursor_hidden = false;
            }
        }
    }

    /// Keeps the picture out from under docked bars (ADR 0001, pitfall 2).
    fn update_margins(&self, app: &App) {
        let size = app.window().size();
        let sf = app.window().scale_factor();
        let (w, h) = (size.width.max(1) as f32, size.height.max(1) as f32);
        let st = self.st.borrow();
        let docked = !st.fullscreen && !st.mini;
        let bars = docked && !st.settings.autohide_controls_windowed;
        let top = if docked { TITLE_BAR * sf / h } else { 0.0 };
        let bottom = if bars { CONTROL_BAR * sf / h } else { 0.0 };
        let right = if st.settings.playlist_visible && docked { PLAYLIST * sf / w } else { 0.0 };
        let m = [top, bottom, right];
        drop(st);
        if m != self.st.borrow().margins {
            let p = self.player();
            let _ = p.set("video-margin-ratio-top", &format!("{top:.5}"));
            let _ = p.set("video-margin-ratio-bottom", &format!("{bottom:.5}"));
            let _ = p.set("video-margin-ratio-right", &format!("{right:.5}"));
            self.st.borrow_mut().margins = m;
        }
    }

    fn save_settings(&self) {
        let _ = self.st.borrow().settings.save();
    }
}

fn main() -> Result<()> {
    slint::BackendSelector::new().require_opengl().select().context("no OpenGL-capable Slint backend")?;

    let dll = find_libmpv();
    let player = Player::new(&dll).with_context(|| format!("starting mpv from {}", dll.display()))?;
    let settings = Settings::load();

    // Fix X05: crisp, readable subtitles by default.
    for (k, v) in [
        ("sub-font", "Segoe UI"),
        ("sub-font-size", "44"),
        ("sub-border-size", "2.6"),
        ("sub-shadow-offset", "1"),
        ("sub-color", "#FFFFFFFF"),
        ("sub-border-color", "#E6000000"),
        ("volume-max", "150"),
    ] {
        let _ = player.set(k, v);
    }
    let _ = player.set("sub-scale", &format!("{}", settings.subtitle_scale));
    let _ = player.set("volume", &format!("{}", settings.volume));
    let _ = player.set("mute", if settings.muted { "yes" } else { "no" });
    if settings.remember_speed {
        let _ = player.set("speed", &format!("{}", settings.speed));
    }

    let app = App::new()?;
    app.set_play_mode(mode_to_int(settings.play_mode));
    app.set_playlist_visible(settings.playlist_visible);
    app.set_on_top(settings.always_on_top);
    app.set_autohide_windowed(settings.autohide_controls_windowed);
    app.set_volume(settings.volume as f32);
    app.set_muted(settings.muted);

    let st: S = Rc::new(RefCell::new(State {
        player: player.clone(),
        playlist: Playlist::default(),
        settings,
        fullscreen: false,
        mini: false,
        normal_size: None,
        last_activity: Instant::now(),
        margins: [-1.0; 3],
        rotation: 0,
        eof_handled: false,
        renderer_ready: false,
        pending: Vec::new(),
        dropped: Vec::new(),
        cursor_hidden: false,
    }));
    let ui = Rc::new(Ui { app: app.as_weak(), st: st.clone(), osd_timer: Rc::new(Timer::default()) });

    // mpv events arrive on their own thread; the UI timer drains them.
    let (tx, rx) = mpsc::channel::<Event>();
    let events_thread = player.spawn_events(move |e| {
        let _ = tx.send(e);
    });

    // --- rendering ------------------------------------------------------------
    let renderer: Rc<RefCell<Option<GlRenderer>>> = Rc::new(RefCell::new(None));
    {
        let renderer = renderer.clone();
        let ui = ui.clone();
        app.window().set_rendering_notifier(move |state, api| match state {
            slint::RenderingState::RenderingSetup => {
                if let slint::GraphicsAPI::NativeOpenGL { get_proc_address } = api {
                    match ui.player().create_gl_renderer(get_proc_address) {
                        Ok(mut r) => {
                            let w = ui.app.clone();
                            r.on_update(move || {
                                let _ = w.upgrade_in_event_loop(|app| app.window().request_redraw());
                            });
                            *renderer.borrow_mut() = Some(r);
                            ui.st.borrow_mut().renderer_ready = true;
                        }
                        Err(e) => eprintln!("mpv renderer: {e}"),
                    }
                }
            }
            slint::RenderingState::BeforeRendering => {
                if let (Some(r), Some(app)) = (renderer.borrow().as_ref(), ui.app()) {
                    let size = app.window().size();
                    if let Err(e) = r.render(0, size.width as i32, size.height as i32, true) {
                        eprintln!("mpv render: {e}");
                    }
                }
            }
            slint::RenderingState::RenderingTeardown => {
                renderer.borrow_mut().take();
            }
            _ => {}
        })?;
    }

    // --- window events: drag and drop, mouse activity ------------------------
    {
        let ui = ui.clone();
        app.window().on_winit_window_event(move |_, event| {
            match event {
                winit::event::WindowEvent::CursorMoved { .. } => ui.activity(),
                winit::event::WindowEvent::HoveredFile(_) => {
                    if let Some(a) = ui.app() {
                        a.set_drop_hover(true);
                    }
                }
                winit::event::WindowEvent::HoveredFileCancelled => {
                    if let Some(a) = ui.app() {
                        a.set_drop_hover(false);
                    }
                }
                winit::event::WindowEvent::DroppedFile(p) => {
                    ui.st.borrow_mut().dropped.push(p.clone());
                    if let Some(a) = ui.app() {
                        a.set_drop_hover(false);
                    }
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }

    // --- callbacks ------------------------------------------------------------
    macro_rules! on {
        ($setter:ident, |$ui:ident $(, $arg:ident)*| $body:expr) => {{
            let $ui = ui.clone();
            app.$setter(move |$($arg),*| $body);
        }};
    }

    on!(on_open_file, |ui| {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Open media")
            .add_filter("Media", MEDIA_FILTER)
            .add_filter("Subtitles", SUB_FILTER)
            .add_filter("All files", &["*"])
            .pick_files()
        {
            ui.open_paths(files);
        }
    });
    on!(on_open_folder, |ui| {
        if let Some(dir) = rfd::FileDialog::new().set_title("Open folder").pick_folder() {
            ui.open_paths(vec![dir]);
        }
    });
    on!(on_add_files, |ui| {
        if let Some(files) = rfd::FileDialog::new().add_filter("Media", MEDIA_FILTER).pick_files() {
            let first = ui.st.borrow_mut().playlist.add(files);
            let playing = ui.app().is_some_and(|a| a.get_has_file());
            ui.refresh_playlist();
            if let (Some(i), false) = (first, playing) {
                ui.play_index(i);
            }
        }
    });
    on!(on_load_subtitle, |ui| {
        if let Some(f) = rfd::FileDialog::new().add_filter("Subtitles", SUB_FILTER).pick_file() {
            ui.open_paths(vec![f]);
        }
    });
    on!(on_toggle_pause, |ui| {
        let _ = ui.player().command(&["cycle", "pause"]);
    });
    on!(on_stop, |ui| {
        let _ = ui.player().command(&["stop"]);
        if let Some(a) = ui.app() {
            a.set_has_file(false);
            a.set_file_name("".into());
            a.set_position(0.0);
            a.set_duration(0.0);
        }
    });
    on!(on_seek_to, |ui, p| {
        let _ = ui.player().command(&["seek", &format!("{}", p * 100.0), "absolute-percent+exact"]);
    });
    on!(on_seek_by, |ui, s| {
        let p = ui.player();
        let _ = p.command(&["seek", &format!("{s}"), "relative+exact"]);
        let pos = p.get_f64("time-pos").unwrap_or(0.0) + s as f64;
        let t = pos.max(0.0) as u64;
        ui.osd(format!("{} {}s   {:02}:{:02}:{:02}", if s < 0.0 { "Back" } else { "Forward" }, s.abs(), t / 3600, t / 60 % 60, t % 60), 1500);
    });
    on!(on_volume_by, |ui, d| {
        let p = ui.player();
        let _ = p.set("mute", "no");
        let v = (p.get_f64("volume").unwrap_or(80.0) + d as f64).clamp(0.0, 150.0);
        let _ = p.set("volume", &format!("{v}"));
        {
            let mut st = ui.st.borrow_mut();
            st.settings.volume = v;
            st.settings.muted = false;
        }
        if let Some(a) = ui.app() {
            a.set_volume(v as f32);
            a.set_muted(false);
        }
        ui.osd(format!("Volume: {}%", v.round()), 1500);
    });
    on!(on_set_volume, |ui, v| {
        let p = ui.player();
        let v = (v as f64).clamp(0.0, 150.0);
        let _ = p.set("mute", "no");
        let _ = p.set("volume", &format!("{v}"));
        {
            let mut st = ui.st.borrow_mut();
            st.settings.volume = v;
            st.settings.muted = false;
        }
        if let Some(a) = ui.app() {
            a.set_volume(v as f32);
            a.set_muted(false);
        }
        ui.osd(format!("Volume: {}%", v.round()), 1500);
    });
    on!(on_toggle_mute, |ui| {
        let p = ui.player();
        let _ = p.command(&["cycle", "mute"]);
        let m = p.get_flag("mute").unwrap_or(false);
        ui.st.borrow_mut().settings.muted = m;
        if let Some(a) = ui.app() {
            a.set_muted(m);
        }
        ui.osd(if m { "Muted" } else { "Sound on" }, 1500);
    });
    on!(on_speed_by, |ui, d| {
        let p = ui.player();
        let v = ((p.get_f64("speed").unwrap_or(1.0) + d as f64) * 10.0).round() / 10.0;
        let v = v.clamp(0.25, 4.0);
        let _ = p.set("speed", &format!("{v}"));
        ui.st.borrow_mut().settings.speed = v;
        if let Some(a) = ui.app() {
            a.set_speed(v as f32);
        }
        ui.osd(format!("Speed: {v:.1}x"), 1500);
    });
    on!(on_set_speed, |ui, v| {
        let p = ui.player();
        let v = ((v as f64) * 10.0).round() / 10.0;
        let v = v.clamp(0.5, 2.0);
        let _ = p.set("speed", &format!("{v}"));
        ui.st.borrow_mut().settings.speed = v;
        if let Some(a) = ui.app() {
            a.set_speed(v as f32);
        }
        ui.osd(format!("Speed: {v:.1}x"), 1500);
    });
    on!(on_speed_reset, |ui| {
        let _ = ui.player().set("speed", "1");
        ui.st.borrow_mut().settings.speed = 1.0;
        if let Some(a) = ui.app() {
            a.set_speed(1.0);
        }
        ui.osd("Speed: 1.0x", 1500);
    });
    on!(on_toggle_fullscreen, |ui| {
        let on = !ui.st.borrow().fullscreen;
        ui.set_fullscreen(on);
    });
    on!(on_toggle_mini, |ui| ui.toggle_mini());
    on!(on_toggle_on_top, |ui| {
        let on = {
            let mut st = ui.st.borrow_mut();
            st.settings.always_on_top = !st.settings.always_on_top;
            st.settings.always_on_top
        };
        if let Some(a) = ui.app() {
            a.set_on_top(on);
        }
        ui.osd(if on { "Always on top" } else { "Not on top" }, 1500);
        ui.save_settings();
    });
    on!(on_toggle_playlist, |ui| {
        let on = {
            let mut st = ui.st.borrow_mut();
            st.settings.playlist_visible = !st.settings.playlist_visible;
            st.settings.playlist_visible
        };
        if let Some(a) = ui.app() {
            a.set_playlist_visible(on);
        }
        ui.save_settings();
    });
    on!(on_toggle_autohide_windowed, |ui| {
        let on = {
            let mut st = ui.st.borrow_mut();
            st.settings.autohide_controls_windowed = !st.settings.autohide_controls_windowed;
            st.settings.autohide_controls_windowed
        };
        if let Some(a) = ui.app() {
            a.set_autohide_windowed(on);
        }
        ui.save_settings();
    });
    on!(on_next, |ui| {
        let mode = ui.st.borrow().settings.play_mode;
        let n = ui.st.borrow_mut().playlist.next(mode);
        if let Some(i) = n {
            ui.play_index(i);
        }
    });
    on!(on_previous, |ui| {
        let n = ui.st.borrow_mut().playlist.previous();
        if let Some(i) = n {
            ui.play_index(i);
        }
    });
    on!(on_play_index, |ui, i| ui.play_index(i as usize));
    on!(on_remove_index, |ui, i| {
        let was_current = ui.st.borrow().playlist.current() == Some(i as usize);
        ui.st.borrow_mut().playlist.remove(i as usize);
        ui.refresh_playlist();
        if was_current {
            let _ = ui.player().command(&["stop"]);
            if let Some(a) = ui.app() {
                a.set_has_file(false);
                a.set_file_name("".into());
            }
        }
    });
    on!(on_clear_playlist, |ui| {
        ui.st.borrow_mut().playlist.clear();
        ui.refresh_playlist();
    });
    on!(on_sort_playlist, |ui| {
        ui.st.borrow_mut().playlist.sort_by_name();
        ui.refresh_playlist();
    });
    on!(on_set_play_mode, |ui, m| {
        let mode = int_to_mode(m);
        ui.st.borrow_mut().settings.play_mode = mode;
        if let Some(a) = ui.app() {
            a.set_play_mode(m);
        }
        let label = ["Single file", "In order", "Repeat list", "Repeat file", "Shuffle"][m.clamp(0, 4) as usize];
        ui.osd(format!("Play mode: {label}"), 1500);
        ui.save_settings();
    });
    on!(on_select_audio, |ui, id| {
        let _ = ui.player().set("aid", &id.to_string());
        ui.refresh_tracks();
    });
    on!(on_select_sub, |ui, id| {
        let _ = ui.player().set("sid", &if id == 0 { "no".to_string() } else { id.to_string() });
        ui.refresh_tracks();
    });
    on!(on_sub_scale_by, |ui, d| {
        let p = ui.player();
        let v = (p.get_f64("sub-scale").unwrap_or(1.0) + d as f64).clamp(0.3, 3.0);
        let _ = p.set("sub-scale", &format!("{v:.2}"));
        ui.st.borrow_mut().settings.subtitle_scale = v;
        ui.osd(format!("Subtitle size {}%", (v * 100.0).round()), 1500);
    });
    on!(on_sub_delay_by, |ui, d| {
        let p = ui.player();
        let v = p.get_f64("sub-delay").unwrap_or(0.0) + d as f64;
        let _ = p.set("sub-delay", &format!("{v:.2}"));
        ui.osd(format!("Subtitle delay {v:+.1} s"), 1500);
    });
    on!(on_set_aspect, |ui, a| {
        let _ = ui.player().set("video-aspect-override", a.as_str());
        ui.osd(format!("Aspect {}", if a.as_str() == "no" { "auto" } else { a.as_str() }), 1500);
    });
    on!(on_rotate, |ui| {
        let r = {
            let mut st = ui.st.borrow_mut();
            st.rotation = (st.rotation + 90) % 360;
            st.rotation
        };
        let _ = ui.player().set("video-rotate", &r.to_string());
        ui.osd(format!("Rotated {r}°"), 1500);
    });
    on!(on_screenshot, |ui| {
        if ui.app().is_some_and(|a| a.get_has_file()) {
            let _ = ui.player().command(&["screenshot"]);
            ui.osd("Screenshot captured", 2000);
        } else {
            ui.osd("No file playing", 1800);
        }
    });
    on!(on_start_drag, |ui| {
        if let Some(a) = ui.app() {
            a.window().with_winit_window(|w| {
                let _ = w.drag_window();
            });
        }
    });
    on!(on_close_window, |_ui| {
        let _ = slint::quit_event_loop();
    });
    on!(on_about, |ui| {
        ui.osd(format!("YoYo Player {} · GPL-3.0 · built on libmpv and Slint", env!("CARGO_PKG_VERSION")), 4000);
    });

    {
        let weak = app.as_weak();
        app.on_request_menu(move |kind, x, y| {
            if let Some(app) = weak.upgrade() {
                if menus::build(&app, kind.as_str()) {
                    app.invoke_open_menu(x, y);
                }
            }
        });
    }
    {
        let weak = app.as_weak();
        app.on_menu_picked(move |id| {
            if let Some(app) = weak.upgrade() {
                menus::run(&app, id.as_str());
            }
        });
    }

    // --- the UI clock: state, events, auto-hide, end of file -----------------
    let clock = Timer::default();
    {
        let ui = ui.clone();
        let tick = Cell::new(0u32);
        clock.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let Some(app) = ui.app() else { return };
            tick.set(tick.get().wrapping_add(1));

            // files opened before the renderer existed, and files dropped on the window
            let queued: Vec<PathBuf> = {
                let mut st = ui.st.borrow_mut();
                let mut v = Vec::new();
                if st.renderer_ready {
                    v.append(&mut st.pending);
                }
                v.append(&mut st.dropped);
                v
            };
            if !queued.is_empty() {
                ui.open_paths(queued);
            }

            // mpv events
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    Event::FileLoaded => {
                        ui.refresh_tracks();
                        ui.refresh_playlist();
                        ui.st.borrow_mut().eof_handled = false;
                    }
                    Event::Error(msg) => {
                        let name = ui.st.borrow().playlist.current_path().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        ui.osd(format!("Can't play {name}: {msg}"), 5000);
                        app.set_has_file(false);
                        app.set_file_name("".into());
                    }
                    Event::Eof | Event::Shutdown => {}
                }
            }

            let p = ui.player();
            if app.get_has_file() {
                app.set_position(p.get_f64("time-pos").unwrap_or(0.0) as f32);
                app.set_duration(p.get_f64("duration").unwrap_or(0.0) as f32);
                app.set_playing(!p.get_flag("pause").unwrap_or(true));
                if tick.get() % 3 == 0 {
                    app.set_speed(p.get_f64("speed").unwrap_or(1.0) as f32);
                    app.set_volume(p.get_f64("volume").unwrap_or(0.0) as f32);
                    app.set_muted(p.get_flag("mute").unwrap_or(false));
                }

                // end of file → next, per play mode (keep-open=yes keeps the last frame)
                if p.get_flag("eof-reached").unwrap_or(false) && !ui.st.borrow().eof_handled {
                    ui.st.borrow_mut().eof_handled = true;
                    let mode = ui.st.borrow().settings.play_mode;
                    let next = ui.st.borrow_mut().playlist.next_after_end(mode);
                    match next {
                        Some(i) if mode == PlayMode::LoopOne => {
                            let _ = p.command(&["seek", "0", "absolute"]);
                            let _ = p.set("pause", "no");
                            ui.st.borrow_mut().eof_handled = false;
                            let _ = i;
                        }
                        Some(i) => ui.play_index(i),
                        None => ui.osd("End of playlist", 2500),
                    }
                }
            } else {
                app.set_playing(false);
            }

            // auto-hide the controls (fullscreen, mini, or windowed when X08 is on)
            let (hide_ok, idle, hidden_cursor) = {
                let st = ui.st.borrow();
                let hide_ok = st.fullscreen || st.mini || st.settings.autohide_controls_windowed;
                (hide_ok, st.last_activity.elapsed() >= Duration::from_millis(st.settings.controls_autohide_ms), st.cursor_hidden)
            };
            let should_hide = hide_ok && idle && app.get_playing() && !app.get_playlist_visible();
            if should_hide && app.get_controls_visible() {
                app.set_controls_visible(false);
                if ui.st.borrow().fullscreen && !hidden_cursor {
                    app.window().with_winit_window(|w| w.set_cursor_visible(false));
                    ui.st.borrow_mut().cursor_hidden = true;
                }
            }

            ui.update_margins(&app);
        });
    }

    // Files given on the command line (Explorer "Open with") play once the renderer is ready.
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if !args.is_empty() {
        ui.open_paths(args);
    }

    // Debug aid: YOYO_DEBUG_MENU=<kind>[@x,y] opens that menu shortly after start (for screenshots).
    let debug_menu = Timer::default();
    if let Ok(spec) = std::env::var("YOYO_DEBUG_MENU") {
        let weak = app.as_weak();
        debug_menu.start(TimerMode::SingleShot, Duration::from_millis(1200), move || {
            let Some(app) = weak.upgrade() else { return };
            match spec.as_str() {
                "playlist-open" => app.set_playlist_visible(true),
                "playlist-list" => {
                    app.set_playlist_visible(true);
                    app.set_playlist_thumb_view(false);
                }
                "cp" => app.set_control_panel_visible(true),
                _ => {
                    let (kind, pos) = spec.split_once('@').unwrap_or((spec.as_str(), "10,44"));
                    let (x, y) = pos.split_once(',').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?))).unwrap_or((10.0, 44.0));
                    if menus::build(&app, kind) {
                        app.invoke_open_menu(x, y);
                    }
                }
            }
        });
    }

    app.run()?;

    // remember speed, volume, mode for next time (fix X04)
    {
        let mut st = st.borrow_mut();
        if let Some(v) = st.player.get_f64("speed") {
            st.settings.speed = v;
        }
        let _ = st.settings.save();
    }
    player.quit();
    let _ = events_thread.join();
    let _ = app.get_playlist().row_count();
    Ok(())
}
