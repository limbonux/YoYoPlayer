//! Menu contents for the flat menus (ui/menu.slint).
//! Slint asks for a menu by kind; this builds the rows (with current checks and
//! tracks), and `run` turns a picked id into the matching action.

use slint::{Image, Model, ModelRc, SharedString, VecModel};

use crate::{App, MenuGroup, MenuLook, MenuRow};

/// Menu icons, embedded so they work wherever the program is installed.
macro_rules! menu_icons {
    ($($name:literal),* $(,)?) => {
        fn icon_svg(name: &str) -> Option<&'static [u8]> {
            match name {
                $($name => Some(include_bytes!(concat!("../ui/icons/", $name, ".svg")).as_slice()),)*
                _ => None,
            }
        }
    };
}

menu_icons!(
    "m-exit",
    "m-file",
    "m-help",
    "m-playback",
    "m-privacy",
    "m-settings",
    "m-tools",
    "mode-loop-all",
    "mode-loop-one",
    "mode-order",
    "mode-random",
    "mode-single",
    "tb-bookmark",
    "tb-capture",
    "tb-convert",
    "tb-cut",
    "tb-gif",
    "tb-join",
    "tb-share",
    "tb-thumbs",
);

fn icon(name: &str) -> Image {
    let svg = icon_svg(name);
    debug_assert!(svg.is_some(), "menu icon {name} is not in menu_icons!");
    svg.and_then(|data| Image::load_from_svg_data(data).ok()).unwrap_or_default()
}

#[derive(Default)]
struct Builder {
    groups: Vec<Vec<MenuRow>>,
}

fn item(id: &str, text: &str, shortcut: &str) -> MenuRow {
    MenuRow {
        id: id.into(),
        text: text.into(),
        shortcut: shortcut.into(),
        icon: Image::default(),
        has_icon: false,
        checked: false,
        enabled: true,
        separator: false,
        submenu: -1,
    }
}

fn check(mut r: MenuRow, on: bool) -> MenuRow {
    r.checked = on;
    r
}

fn enabled(mut r: MenuRow, on: bool) -> MenuRow {
    r.enabled = on;
    r
}

fn with_icon(mut r: MenuRow, name: &str) -> MenuRow {
    r.icon = icon(name);
    r.has_icon = true;
    r
}

fn sep() -> MenuRow {
    let mut r = item("", "", "");
    r.separator = true;
    r
}

fn header(text: &str) -> MenuRow {
    enabled(item("header", text, ""), false)
}

impl Builder {
    fn sub(&mut self, text: &str, rows: Vec<MenuRow>) -> MenuRow {
        self.groups.push(rows);
        let mut r = item("", text, "");
        r.submenu = (self.groups.len() - 1) as i32;
        r
    }
}

fn s(x: &SharedString) -> String {
    x.to_string()
}

/// Fills the app's menu properties for `kind`. Returns false for an unknown kind.
pub fn build(app: &App, kind: &str) -> bool {
    let mut b = Builder::default();
    let has_file = app.get_has_file();
    let mode = app.get_play_mode();
    let mode_rows = || {
        [
            ("Single", "mode-single"),
            ("Order", "mode-order"),
            ("Loop", "mode-loop-all"),
            ("Single Loop", "mode-loop-one"),
            ("Random", "mode-random"),
        ]
        .iter()
        .enumerate()
        .map(|(i, (t, ic))| with_icon(check(item(&format!("mode:{i}"), t, ""), mode == i as i32), ic))
        .collect::<Vec<_>>()
    };
    let speed_rows = || {
        vec![
            item("speed+", "Faster (+0.1×)", "C"),
            item("speed-", "Slower (−0.1×)", "X"),
            item("speed1", "Normal Speed", "R"),
            sep(),
            item("fwd30", "Forward 30 Seconds", "Ctrl + →"),
            item("back30", "Back 30 Seconds", "Ctrl + ←"),
            sep(),
            enabled(item("next", "Next File", "PgDn"), app.get_playlist().row_count() > 1),
            enabled(item("prev", "Previous File", "PgUp"), app.get_playlist().row_count() > 1),
        ]
    };
    let tools_rows = || {
        vec![
            enabled(with_icon(item("tb-capture", "Capture", "Alt + A"), "tb-capture"), has_file),
            enabled(with_icon(item("tb-gif", "GIF", ""), "tb-gif"), has_file),
            enabled(with_icon(item("tb-thumbs", "Thumbnails", "Alt + S"), "tb-thumbs"), has_file),
            enabled(with_icon(item("tb-cut", "Cut", ""), "tb-cut"), has_file),
            with_icon(item("tb-join", "Joiner", ""), "tb-join"),
            with_icon(item("tb-convert", "Converter", ""), "tb-convert"),
            with_icon(item("tb-share", "Share", ""), "tb-share"),
            enabled(with_icon(item("tb-bookmark", "Bookmark", ""), "tb-bookmark"), has_file),
        ]
    };

    let (rows, look, width): (Vec<MenuRow>, MenuLook, f32) = match kind {
        "open" => (
            vec![item("open-folder", "Open Folder", "Ctrl + F"), item("open-url", "Open URL", "Ctrl + U")],
            MenuLook::Navy,
            150.0,
        ),
        "mode" => (mode_rows(), MenuLook::Grey, 132.0),
        "toolbox" => {
            let mut v = vec![header("- ToolBox Menu -")];
            v.extend(tools_rows());
            (v, MenuLook::Toolbox, 126.0)
        }
        "logo" => {
            let file = b.sub(
                "File",
                vec![
                    item("open-file", "Open File", "Ctrl + O"),
                    item("open-folder", "Open Folder", "Ctrl + F"),
                    item("open-url", "Open URL", "Ctrl + U"),
                    sep(),
                    enabled(item("sub-load", "Load Subtitle", "Ctrl + S"), has_file),
                ],
            );
            let mut playback = vec![
                enabled(item("pause", if app.get_playing() { "Pause" } else { "Play" }, "Space"), has_file),
                enabled(item("stop", "Stop", ""), has_file),
                sep(),
            ];
            playback.extend(mode_rows());
            let playback = b.sub("Playback", playback);
            let mut tools = vec![
                check(item("ontop", "Always on Top", "T"), app.get_on_top()),
                check(item("mini", "Minimal Screen", "Shift + Enter"), app.get_mini()),
                check(item("playlist", "PlayList", "F3"), app.get_playlist_visible()),
                check(item("control-panel", "Control Panel", "F4"), app.get_control_panel_visible()),
                sep(),
            ];
            tools.extend(tools_rows());
            let tools = b.sub("Tools", tools);
            let privacy = b.sub("Privacy", vec![enabled(item("pl-clear", "Clear PlayList", ""), app.get_playlist().row_count() > 0)]);
            let settings = b.sub(
                "Settings",
                vec![
                    check(item("control-panel", "Control Panel", "F4"), app.get_control_panel_visible()),
                    check(item("autohide", "Hide Controls While Playing", ""), app.get_autohide_windowed()),
                ],
            );
            let help = b.sub("Help", vec![item("about", "About YoYo Player", "")]);
            (
                vec![
                    with_icon(file, "m-file"),
                    with_icon(playback, "m-playback"),
                    with_icon(tools, "m-tools"),
                    with_icon(privacy, "m-privacy"),
                    with_icon(settings, "m-settings"),
                    with_icon(help, "m-help"),
                    with_icon(item("exit", "Exit", "Alt + F4"), "m-exit"),
                ],
                MenuLook::Grey,
                165.0,
            )
        }
        "context" => {
            let play_mode = b.sub("Play Mode", mode_rows());
            let playback = b.sub("Playback", speed_rows());
            let video = b.sub(
                "Video",
                vec![
                    item("aspect:no", "Default", ""),
                    item("aspect:4:3", "4:3", ""),
                    item("aspect:16:9", "16:9", ""),
                    item("aspect:2.33:1", "21:9", ""),
                    sep(),
                    item("rotate", "Rotate Right 90°", ""),
                ],
            );
            let mut sound = vec![
                item("mute", if app.get_muted() { "Unmute" } else { "Mute" }, "M"),
                item("vol+", "Volume Up", "↑"),
                item("vol-", "Volume Down", "↓"),
            ];
            let audio: Vec<_> = app.get_audio_tracks().iter().collect();
            if !audio.is_empty() {
                sound.push(sep());
                for t in audio {
                    sound.push(check(item(&format!("aid:{}", t.id), &s(&t.title), ""), t.selected));
                }
            }
            let sound = b.sub("Sound", sound);
            let mut subs = vec![item("sub-load", "Load Subtitle", "Ctrl + S"), check(item("sid:0", "Off", ""), !app.get_sub_selected())];
            for t in app.get_sub_tracks().iter() {
                subs.push(check(item(&format!("sid:{}", t.id), &s(&t.title), ""), t.selected));
            }
            subs.extend([
                sep(),
                item("sub+", "Bigger", "Shift + ↑"),
                item("sub-", "Smaller", "Shift + ↓"),
                item("subd+", "Delay +0.1 s", "]"),
                item("subd-", "Delay −0.1 s", "["),
            ]);
            let subtitles = b.sub("Subtitles", subs);
            let tools = b.sub("Tools", tools_rows());
            let windowed = !app.get_fullscreen() && !app.get_mini();
            (
                vec![
                    item("open-file", "Open File", "Ctrl + O"),
                    item("open-folder", "Open Folder", "Ctrl + F"),
                    item("open-url", "Open URL", "Ctrl + U"),
                    enabled(check(item("fullscreen", "Full Screen", "Enter"), app.get_fullscreen()), has_file),
                    enabled(check(item("normal", "Normal Screen", ""), windowed && has_file), has_file),
                    enabled(check(item("mini", "Minimal Screen", "Shift + Enter"), app.get_mini()), has_file),
                    check(item("ontop", "Always on Top", "T"), app.get_on_top()),
                    play_mode,
                    playback,
                    enabled(video, has_file),
                    enabled(sound, has_file),
                    enabled(subtitles, has_file),
                    tools,
                    check(item("playlist", "PlayList", "F3"), app.get_playlist_visible()),
                    check(item("control-panel", "Control Panel", "F4"), app.get_control_panel_visible()),
                    item("settings", "Settings", "F5"),
                    enabled(item("file-props", "File Properties", ""), has_file),
                ],
                MenuLook::Grey,
                210.0,
            )
        }
        "playlist" => {
            let play_mode = b.sub("Play Mode", mode_rows());
            let sort_by = b.sub("Sort by", vec![item("pl-sort", "Name", "")]);
            (
                vec![
                    item("add-file", "Add File", ""),
                    item("open-folder", "Add Folder", ""),
                    enabled(item("pl-clear", "Clear PlayList", ""), app.get_playlist().row_count() > 0),
                    play_mode,
                    sort_by,
                ],
                MenuLook::Grey,
                175.0,
            )
        }
        k if k.starts_with("row:") => {
            let i = &k[4..];
            let play_mode = b.sub("Play Mode", mode_rows());
            let sort_by = b.sub("Sort by", vec![item("pl-sort", "Name", "")]);
            (
                vec![
                    item("add-file", "Add File", ""),
                    item("open-folder", "Add Folder", ""),
                    item(&format!("pl-remove:{i}"), "Delete selected items", ""),
                    item("pl-clear", "Clear PlayList", ""),
                    play_mode,
                    sort_by,
                ],
                MenuLook::Grey,
                175.0,
            )
        }
        _ => return false,
    };

    app.set_menu_rows(ModelRc::new(VecModel::from(rows)));
    app.set_menu_groups(ModelRc::new(VecModel::from(
        b.groups.into_iter().map(|g| MenuGroup { rows: ModelRc::new(VecModel::from(g)) }).collect::<Vec<_>>(),
    )));
    app.set_menu_look(look);
    app.set_menu_width(width);
    true
}

/// Runs the action for a picked menu id by invoking the same callbacks the buttons use.
pub fn run(app: &App, id: &str) {
    let num = |p: &str| id.strip_prefix(p).and_then(|v| v.parse::<i32>().ok());
    if let Some(m) = num("mode:") {
        return app.invoke_set_play_mode(m);
    }
    if let Some(t) = num("aid:") {
        return app.invoke_select_audio(t);
    }
    if let Some(t) = num("sid:") {
        return app.invoke_select_sub(t);
    }
    if let Some(i) = num("pl-play:") {
        return app.invoke_play_index(i);
    }
    if let Some(i) = num("pl-remove:") {
        return app.invoke_remove_index(i);
    }
    if let Some(a) = id.strip_prefix("aspect:") {
        app.set_current_aspect(a.into());
        return app.invoke_set_aspect(a.into());
    }
    match id {
        "open-file" | "open-url" => app.invoke_open_file(),
        "add-file" => app.invoke_add_files(),
        "open-folder" => app.invoke_open_folder(),
        "sub-load" => app.invoke_load_subtitle(),
        "pause" => app.invoke_toggle_pause(),
        "stop" => app.invoke_stop(),
        "fullscreen" => app.invoke_toggle_fullscreen(),
        "normal" => {
            if app.get_fullscreen() {
                app.invoke_toggle_fullscreen();
            } else if app.get_mini() {
                app.invoke_toggle_mini();
            }
        }
        "mini" => app.invoke_toggle_mini(),
        "ontop" => app.invoke_toggle_on_top(),
        "playlist" => app.invoke_toggle_playlist(),
        "control-panel" | "settings" => app.set_control_panel_visible(!app.get_control_panel_visible()),
        "autohide" => app.invoke_toggle_autohide_windowed(),
        "speed+" => app.invoke_speed_by(0.1),
        "speed-" => app.invoke_speed_by(-0.1),
        "speed1" => app.invoke_speed_reset(),
        "fwd30" => app.invoke_seek_by(30.0),
        "back30" => app.invoke_seek_by(-30.0),
        "next" => app.invoke_next(),
        "prev" => app.invoke_previous(),
        "rotate" => app.invoke_rotate(),
        "mute" => app.invoke_toggle_mute(),
        "vol+" => app.invoke_volume_by(5.0),
        "vol-" => app.invoke_volume_by(-5.0),
        "sub+" => app.invoke_sub_scale_by(0.1),
        "sub-" => app.invoke_sub_scale_by(-0.1),
        "subd+" => app.invoke_sub_delay_by(0.1),
        "subd-" => app.invoke_sub_delay_by(-0.1),
        "pl-sort" => app.invoke_sort_playlist(),
        "pl-clear" => app.invoke_clear_playlist(),
        "tb-capture" => app.invoke_screenshot(),
        "tb-gif" | "tb-thumbs" | "tb-cut" | "tb-join" | "tb-convert" | "tb-share" | "tb-bookmark" | "file-props" => {
            app.set_control_panel_visible(true);
        }
        "about" => app.invoke_about(),
        "exit" => app.invoke_close_window(),
        _ => {}
    }
}
