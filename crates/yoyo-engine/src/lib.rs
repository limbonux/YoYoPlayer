//! Thin, safe wrapper around libmpv for YoYo Player.
//!
//! `libmpv-2.dll` is loaded at runtime with `libloading`, so no import
//! library is needed at build time. The render API is used with OpenGL so the
//! video is drawn inside the UI's own GL context (Slint draws the controls on
//! top of it).

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::Library;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not load libmpv from {path}: {source}")]
    Load { path: PathBuf, source: libloading::Error },
    #[error("libmpv is missing the symbol {0}")]
    Symbol(&'static str),
    #[error("mpv_create failed")]
    Create,
    #[error("mpv {call} failed: {message} ({code})")]
    Mpv { call: &'static str, code: i32, message: String },
    #[error("argument contains a NUL byte")]
    Nul,
}

pub type Result<T> = std::result::Result<T, Error>;

// Values from mpv/client.h and mpv/render.h (client API 2.6).
const MPV_FORMAT_FLAG: c_int = 3;
const MPV_FORMAT_DOUBLE: c_int = 5;
const MPV_RENDER_PARAM_INVALID: c_int = 0;
const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const MPV_RENDER_PARAM_OPENGL_FBO: c_int = 3;
const MPV_RENDER_PARAM_FLIP_Y: c_int = 4;

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct OpenGlInitParams {
    get_proc_address: unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
struct OpenGlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

type Handle = *mut c_void;

#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawEndFile {
    reason: c_int,
    error: c_int,
}

const MPV_EVENT_SHUTDOWN: c_int = 1;
const MPV_EVENT_END_FILE: c_int = 7;
const MPV_EVENT_FILE_LOADED: c_int = 8;
const MPV_END_FILE_REASON_EOF: c_int = 0;
const MPV_END_FILE_REASON_ERROR: c_int = 4;

/// What mpv reports to the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    FileLoaded,
    /// The file reached its end (only when keep-open is off; YoYo watches eof-reached instead).
    Eof,
    /// The file could not be played; the message is mpv's.
    Error(String),
    Shutdown,
}

struct Api {
    _lib: Library,
    create: unsafe extern "C" fn() -> Handle,
    initialize: unsafe extern "C" fn(Handle) -> c_int,
    terminate_destroy: unsafe extern "C" fn(Handle),
    set_option_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    set_property_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int,
    command: unsafe extern "C" fn(Handle, *mut *const c_char) -> c_int,
    get_property_string: unsafe extern "C" fn(Handle, *const c_char) -> *mut c_char,
    free: unsafe extern "C" fn(*mut c_void),
    wait_event: unsafe extern "C" fn(Handle, f64) -> *mut RawEvent,
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    render_context_create: unsafe extern "C" fn(*mut *mut c_void, Handle, *mut RenderParam) -> c_int,
    render_context_set_update_callback:
        unsafe extern "C" fn(*mut c_void, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void),
    render_context_render: unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int,
    render_context_free: unsafe extern "C" fn(*mut c_void),
}

macro_rules! sym {
    ($lib:expr, $name:literal) => {
        // SAFETY: the symbol types match the declarations in mpv/client.h and render.h.
        unsafe {
            *$lib
                .get(concat!($name, "\0").as_bytes())
                .map_err(|_| Error::Symbol($name))?
        }
    };
}

impl Api {
    fn load(path: &Path) -> Result<Self> {
        // SAFETY: loading libmpv runs its DLL initialisers, which are trusted.
        let lib = unsafe { Library::new(path) }.map_err(|source| Error::Load { path: path.to_owned(), source })?;
        Ok(Self {
            create: sym!(lib, "mpv_create"),
            initialize: sym!(lib, "mpv_initialize"),
            terminate_destroy: sym!(lib, "mpv_terminate_destroy"),
            set_option_string: sym!(lib, "mpv_set_option_string"),
            set_property_string: sym!(lib, "mpv_set_property_string"),
            get_property: sym!(lib, "mpv_get_property"),
            command: sym!(lib, "mpv_command"),
            get_property_string: sym!(lib, "mpv_get_property_string"),
            free: sym!(lib, "mpv_free"),
            wait_event: sym!(lib, "mpv_wait_event"),
            error_string: sym!(lib, "mpv_error_string"),
            render_context_create: sym!(lib, "mpv_render_context_create"),
            render_context_set_update_callback: sym!(lib, "mpv_render_context_set_update_callback"),
            render_context_render: sym!(lib, "mpv_render_context_render"),
            render_context_free: sym!(lib, "mpv_render_context_free"),
            _lib: lib,
        })
    }

    fn check(&self, call: &'static str, code: c_int) -> Result<()> {
        if code >= 0 {
            return Ok(());
        }
        // SAFETY: mpv_error_string returns a static string for any code.
        let message = unsafe { CStr::from_ptr((self.error_string)(code)) }.to_string_lossy().into_owned();
        Err(Error::Mpv { call, code, message })
    }
}

/// Owns the mpv core; destroyed when the player and every render context are gone.
struct Core {
    api: Api,
    handle: Handle,
}

// SAFETY: the mpv client API is thread-safe for a single handle.
unsafe impl Send for Core {}
unsafe impl Sync for Core {}

impl Drop for Core {
    fn drop(&mut self) {
        // SAFETY: handle came from mpv_create and is destroyed exactly once.
        unsafe { (self.api.terminate_destroy)(self.handle) }
    }
}

fn cstr(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::Nul)
}

/// The player: one mpv instance.
#[derive(Clone)]
pub struct Player {
    core: Arc<Core>,
}

impl Player {
    /// Loads `libmpv-2.dll` from `dll` and starts mpv configured for embedding.
    pub fn new(dll: &Path) -> Result<Self> {
        let api = Api::load(dll)?;
        // SAFETY: plain constructor call.
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err(Error::Create);
        }
        let core = Arc::new(Core { api, handle });
        let player = Self { core };
        for (k, v) in [
            ("vo", "libmpv"),
            ("hwdec", "auto-safe"),
            ("hr-seek", "yes"),
            ("keep-open", "yes"),
            ("idle", "yes"),
            ("osc", "no"),
            ("input-default-bindings", "no"),
            ("terminal", "no"),
            ("config", "no"),
            ("ytdl", "no"),
            ("sub-auto", "fuzzy"),
            ("tone-mapping", "auto"),
        ] {
            player.set_option(k, v)?;
        }
        // Debug aid: YOYO_MPV_LOG=<file> writes mpv's verbose log there.
        if let Ok(log) = std::env::var("YOYO_MPV_LOG") {
            player.set_option("log-file", &log)?;
            player.set_option("msg-level", "all=v")?;
        }
        let api = &player.core.api;
        // SAFETY: valid handle; called once before use.
        api.check("mpv_initialize", unsafe { (api.initialize)(player.core.handle) })?;
        Ok(player)
    }

    fn set_option(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        let api = &self.core.api;
        // SAFETY: valid handle and NUL-terminated strings.
        api.check("mpv_set_option_string", unsafe { (api.set_option_string)(self.core.handle, n.as_ptr(), v.as_ptr()) })
    }

    /// Sets a property from its string form, for example `("pause", "yes")`.
    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        let api = &self.core.api;
        // SAFETY: valid handle and NUL-terminated strings.
        api.check("mpv_set_property_string", unsafe { (api.set_property_string)(self.core.handle, n.as_ptr(), v.as_ptr()) })
    }

    /// Runs an mpv command such as `["loadfile", path]` or `["seek", "10"]`.
    pub fn command(&self, args: &[&str]) -> Result<()> {
        let owned: Vec<CString> = args.iter().map(|a| cstr(a)).collect::<Result<_>>()?;
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        let api = &self.core.api;
        // SAFETY: NULL-terminated array of valid C strings that outlive the call.
        api.check("mpv_command", unsafe { (api.command)(self.core.handle, ptrs.as_mut_ptr()) })
    }

    /// Reads a numeric property; `None` while it is unavailable (no file, for example).
    pub fn get_f64(&self, name: &str) -> Option<f64> {
        let n = cstr(name).ok()?;
        let mut out = 0f64;
        // SAFETY: MPV_FORMAT_DOUBLE writes one f64 into `out`.
        let rc = unsafe { (self.core.api.get_property)(self.core.handle, n.as_ptr(), MPV_FORMAT_DOUBLE, (&raw mut out).cast()) };
        (rc >= 0).then_some(out)
    }

    /// Reads a property in its string form; `None` while unavailable.
    pub fn get_string(&self, name: &str) -> Option<String> {
        let n = cstr(name).ok()?;
        // SAFETY: valid handle; the returned string is freed with mpv_free.
        let p = unsafe { (self.core.api.get_property_string)(self.core.handle, n.as_ptr()) };
        if p.is_null() {
            return None;
        }
        // SAFETY: mpv returns a NUL-terminated UTF-8 string.
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        // SAFETY: allocated by mpv.
        unsafe { (self.core.api.free)(p.cast()) };
        Some(s)
    }

    /// Starts a thread that forwards mpv events to `on_event` until mpv shuts down.
    /// Call [`Player::quit`] on exit so the thread ends and the core is released.
    pub fn spawn_events(&self, on_event: impl Fn(Event) + Send + 'static) -> std::thread::JoinHandle<()> {
        let core = self.core.clone();
        std::thread::Builder::new()
            .name("mpv-events".into())
            .spawn(move || loop {
                // SAFETY: valid handle; only this thread waits for events.
                let ev = unsafe { &*(core.api.wait_event)(core.handle, -1.0) };
                match ev.event_id {
                    MPV_EVENT_SHUTDOWN => {
                        on_event(Event::Shutdown);
                        return;
                    }
                    MPV_EVENT_FILE_LOADED => on_event(Event::FileLoaded),
                    MPV_EVENT_END_FILE if !ev.data.is_null() => {
                        // SAFETY: END_FILE carries an mpv_event_end_file.
                        let end = unsafe { &*(ev.data as *const RawEndFile) };
                        match end.reason {
                            MPV_END_FILE_REASON_ERROR => {
                                // SAFETY: static string for any code.
                                let msg = unsafe { CStr::from_ptr((core.api.error_string)(end.error)) }.to_string_lossy().into_owned();
                                on_event(Event::Error(msg));
                            }
                            MPV_END_FILE_REASON_EOF => on_event(Event::Eof),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            })
            .expect("spawn mpv event thread")
    }

    /// Asks mpv to shut down; the event thread then exits.
    pub fn quit(&self) {
        let _ = self.command(&["quit"]);
    }

    /// Reads a yes/no property.
    pub fn get_flag(&self, name: &str) -> Option<bool> {
        let n = cstr(name).ok()?;
        let mut out: c_int = 0;
        // SAFETY: MPV_FORMAT_FLAG writes one int into `out`.
        let rc = unsafe { (self.core.api.get_property)(self.core.handle, n.as_ptr(), MPV_FORMAT_FLAG, (&raw mut out).cast()) };
        (rc >= 0).then_some(out != 0)
    }

    /// Creates the OpenGL render context. Call it on the thread that owns the
    /// current GL context, while `get_proc_address` is valid.
    pub fn create_gl_renderer(&self, get_proc_address: &dyn Fn(&CStr) -> *const c_void) -> Result<GlRenderer> {
        unsafe extern "C" fn trampoline(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
            // SAFETY: ctx points at the `&dyn Fn` below for the duration of mpv_render_context_create.
            let f = unsafe { &*(ctx as *const &dyn Fn(&CStr) -> *const c_void) };
            // SAFETY: mpv passes a valid NUL-terminated name.
            f(unsafe { CStr::from_ptr(name) }) as *mut c_void
        }
        let mut gpa: &dyn Fn(&CStr) -> *const c_void = get_proc_address;
        let mut init = OpenGlInitParams { get_proc_address: trampoline, get_proc_address_ctx: (&raw mut gpa).cast() };
        let api_type = c"opengl";
        let mut params = [
            RenderParam { kind: MPV_RENDER_PARAM_API_TYPE, data: api_type.as_ptr() as *mut c_void },
            RenderParam { kind: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, data: (&raw mut init).cast() },
            RenderParam { kind: MPV_RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
        ];
        let mut ctx = std::ptr::null_mut();
        let api = &self.core.api;
        // SAFETY: params is a valid INVALID-terminated list; init and gpa outlive the call.
        api.check("mpv_render_context_create", unsafe { (api.render_context_create)(&mut ctx, self.core.handle, params.as_mut_ptr()) })?;
        Ok(GlRenderer { core: self.core.clone(), ctx, wake: None })
    }
}

type WakeFn = Box<dyn Fn() + Send + Sync>;

/// mpv's OpenGL renderer, bound to the UI's GL context.
pub struct GlRenderer {
    core: Arc<Core>,
    ctx: *mut c_void,
    wake: Option<Box<WakeFn>>,
}

impl GlRenderer {
    /// `wake` runs on an mpv thread whenever a new frame should be drawn.
    pub fn on_update(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        unsafe extern "C" fn cb(ctx: *mut c_void) {
            // SAFETY: ctx is the boxed WakeFn owned by this renderer, alive until free.
            let f = unsafe { &*(ctx as *const WakeFn) };
            f();
        }
        let boxed: Box<WakeFn> = Box::new(Box::new(wake));
        let ptr = (&*boxed as *const WakeFn) as *mut c_void;
        // SAFETY: the callback context stays valid until the context is freed in Drop.
        unsafe { (self.core.api.render_context_set_update_callback)(self.ctx, Some(cb), ptr) };
        self.wake = Some(boxed);
    }

    /// Draws the current frame into framebuffer `fbo` of `width`x`height` pixels.
    pub fn render(&self, fbo: i32, width: i32, height: i32, flip_y: bool) -> Result<()> {
        let mut target = OpenGlFbo { fbo, w: width, h: height, internal_format: 0 };
        let mut flip: c_int = flip_y as c_int;
        let mut params = [
            RenderParam { kind: MPV_RENDER_PARAM_OPENGL_FBO, data: (&raw mut target).cast() },
            RenderParam { kind: MPV_RENDER_PARAM_FLIP_Y, data: (&raw mut flip).cast() },
            RenderParam { kind: MPV_RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
        ];
        let api = &self.core.api;
        // SAFETY: the GL context this renderer was created on is current (UI render thread).
        api.check("mpv_render_context_render", unsafe { (api.render_context_render)(self.ctx, params.as_mut_ptr()) })
    }
}

impl Drop for GlRenderer {
    fn drop(&mut self) {
        // SAFETY: frees the context once; the update callback box is dropped afterwards.
        unsafe { (self.core.api.render_context_free)(self.ctx) };
        self.wake = None;
    }
}

/// Finds `libmpv-2.dll`: next to the executable first, then `third_party/mpv` for development.
pub fn find_libmpv() -> PathBuf {
    let name = "libmpv-2.dll";
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(name);
            if p.exists() {
                return p;
            }
        }
    }
    // Development only: a release build must not carry the build machine's paths.
    #[cfg(debug_assertions)]
    return PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../third_party/mpv/libmpv-2.dll"));
    #[cfg(not(debug_assertions))]
    PathBuf::from(name)
}
