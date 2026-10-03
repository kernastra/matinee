//! Minimal hand-written libmpv bindings: client API 2.x plus the software
//! (`MPV_RENDER_API_TYPE_SW`) render backend. Only what the spike exercises.
//!
//! A production `matinee-player` should generate these with bindgen (or use
//! `libmpv2-sys`) and pin the client API version it was generated against.

use std::ffi::{CStr, CString, c_char, c_int, c_ulong, c_void};
use std::ptr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};

#[allow(non_camel_case_types)]
mod ffi {
    use std::ffi::{c_char, c_int, c_ulong, c_void};

    #[repr(C)]
    pub struct mpv_handle {
        _private: [u8; 0],
    }

    #[repr(C)]
    pub struct mpv_render_context {
        _private: [u8; 0],
    }

    #[repr(C)]
    pub struct mpv_event {
        pub event_id: c_int,
        pub error: c_int,
        pub reply_userdata: u64,
        pub data: *mut c_void,
    }

    #[repr(C)]
    pub struct mpv_event_property {
        pub name: *const c_char,
        pub format: c_int,
        pub data: *mut c_void,
    }

    #[repr(C)]
    pub struct mpv_event_end_file {
        pub reason: c_int,
        pub error: c_int,
        pub playlist_entry_id: i64,
        pub playlist_insert_id: i64,
        pub playlist_insert_num_entries: c_int,
    }

    #[repr(C)]
    pub struct mpv_render_param {
        pub kind: c_int,
        pub data: *mut c_void,
    }

    pub type mpv_render_update_fn = unsafe extern "C" fn(*mut c_void);

    pub const MPV_FORMAT_FLAG: c_int = 3;
    pub const MPV_FORMAT_INT64: c_int = 4;
    pub const MPV_FORMAT_DOUBLE: c_int = 5;

    pub const MPV_RENDER_PARAM_INVALID: c_int = 0;
    pub const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
    pub const MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
    pub const MPV_RENDER_PARAM_SW_SIZE: c_int = 17;
    pub const MPV_RENDER_PARAM_SW_FORMAT: c_int = 18;
    pub const MPV_RENDER_PARAM_SW_STRIDE: c_int = 19;
    pub const MPV_RENDER_PARAM_SW_POINTER: c_int = 20;
    pub const MPV_RENDER_UPDATE_FRAME: u64 = 1;

    unsafe extern "C" {
        pub fn mpv_client_api_version() -> c_ulong;
        pub fn mpv_create() -> *mut mpv_handle;
        pub fn mpv_initialize(ctx: *mut mpv_handle) -> c_int;
        pub fn mpv_terminate_destroy(ctx: *mut mpv_handle);
        pub fn mpv_set_option_string(
            ctx: *mut mpv_handle,
            name: *const c_char,
            data: *const c_char,
        ) -> c_int;
        pub fn mpv_set_property_string(
            ctx: *mut mpv_handle,
            name: *const c_char,
            data: *const c_char,
        ) -> c_int;
        pub fn mpv_get_property_string(ctx: *mut mpv_handle, name: *const c_char) -> *mut c_char;
        pub fn mpv_get_property(
            ctx: *mut mpv_handle,
            name: *const c_char,
            format: c_int,
            data: *mut c_void,
        ) -> c_int;
        pub fn mpv_command(ctx: *mut mpv_handle, args: *mut *const c_char) -> c_int;
        pub fn mpv_free(data: *mut c_void);
        pub fn mpv_wait_event(ctx: *mut mpv_handle, timeout: f64) -> *mut mpv_event;
        pub fn mpv_error_string(error: c_int) -> *const c_char;
        pub fn mpv_event_name(event: c_int) -> *const c_char;
        pub fn mpv_render_context_create(
            res: *mut *mut mpv_render_context,
            mpv: *mut mpv_handle,
            params: *mut mpv_render_param,
        ) -> c_int;
        pub fn mpv_render_context_set_update_callback(
            ctx: *mut mpv_render_context,
            callback: Option<mpv_render_update_fn>,
            callback_ctx: *mut c_void,
        );
        pub fn mpv_render_context_update(ctx: *mut mpv_render_context) -> u64;
        pub fn mpv_render_context_render(
            ctx: *mut mpv_render_context,
            params: *mut mpv_render_param,
        ) -> c_int;
        pub fn mpv_render_context_free(ctx: *mut mpv_render_context);
    }

    pub fn version() -> c_ulong {
        unsafe { mpv_client_api_version() }
    }
}

fn check(code: c_int, what: &str) -> Result<()> {
    if code >= 0 {
        return Ok(());
    }
    let message = unsafe { CStr::from_ptr(ffi::mpv_error_string(code)) };
    bail!("{what}: {} ({code})", message.to_string_lossy())
}

fn cstring(value: &str) -> Result<CString> {
    CString::new(value).map_err(|_| anyhow!("string contains NUL: {value:?}"))
}

/// `(major, minor)` of the libmpv client API that was loaded at runtime.
pub fn client_api_version() -> (u32, u32) {
    let version: c_ulong = ffi::version();
    ((version >> 16) as u32, (version & 0xffff) as u32)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    None,
    Shutdown,
    StartFile,
    FileLoaded,
    EndFile { reason: i32, error: i32 },
    Seek,
    PlaybackRestart,
    VideoReconfig,
    AudioReconfig,
    PropertyChange(String),
    Other(String),
}

/// An mpv player instance. The libmpv client API is thread-safe, so the
/// handle can be shared between the UI thread and a render thread.
pub struct Mpv {
    handle: *mut ffi::mpv_handle,
}

unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

impl Mpv {
    /// Creates and initializes a player. `options` are applied before
    /// `mpv_initialize`, which is required for e.g. `vo=libmpv`.
    pub fn new(options: &[(&str, &str)]) -> Result<Arc<Self>> {
        let handle = unsafe { ffi::mpv_create() };
        if handle.is_null() {
            bail!("mpv_create returned NULL");
        }
        let mpv = Self { handle };
        for (name, value) in options {
            let (name_c, value_c) = (cstring(name)?, cstring(value)?);
            check(
                unsafe { ffi::mpv_set_option_string(handle, name_c.as_ptr(), value_c.as_ptr()) },
                &format!("set option {name}={value}"),
            )?;
        }
        check(unsafe { ffi::mpv_initialize(handle) }, "mpv_initialize")?;
        Ok(Arc::new(mpv))
    }

    pub fn command(&self, args: &[&str]) -> Result<()> {
        let owned = args
            .iter()
            .map(|arg| cstring(arg))
            .collect::<Result<Vec<_>>>()?;
        let mut pointers: Vec<*const c_char> = owned.iter().map(|arg| arg.as_ptr()).collect();
        pointers.push(ptr::null());
        check(
            unsafe { ffi::mpv_command(self.handle, pointers.as_mut_ptr()) },
            &format!("command {args:?}"),
        )
    }

    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        let (name_c, value_c) = (cstring(name)?, cstring(value)?);
        check(
            unsafe { ffi::mpv_set_property_string(self.handle, name_c.as_ptr(), value_c.as_ptr()) },
            &format!("set {name}={value}"),
        )
    }

    pub fn get_string(&self, name: &str) -> Result<String> {
        let name_c = cstring(name)?;
        let raw = unsafe { ffi::mpv_get_property_string(self.handle, name_c.as_ptr()) };
        if raw.is_null() {
            bail!("property {name} unavailable");
        }
        let value = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { ffi::mpv_free(raw.cast()) };
        Ok(value)
    }

    pub fn get_f64(&self, name: &str) -> Result<f64> {
        let mut value = 0f64;
        self.get_raw(
            name,
            ffi::MPV_FORMAT_DOUBLE,
            (&mut value as *mut f64).cast(),
        )?;
        Ok(value)
    }

    pub fn get_i64(&self, name: &str) -> Result<i64> {
        let mut value = 0i64;
        self.get_raw(name, ffi::MPV_FORMAT_INT64, (&mut value as *mut i64).cast())?;
        Ok(value)
    }

    pub fn get_flag(&self, name: &str) -> Result<bool> {
        let mut value: c_int = 0;
        self.get_raw(
            name,
            ffi::MPV_FORMAT_FLAG,
            (&mut value as *mut c_int).cast(),
        )?;
        Ok(value != 0)
    }

    fn get_raw(&self, name: &str, format: c_int, out: *mut c_void) -> Result<()> {
        let name_c = cstring(name)?;
        check(
            unsafe { ffi::mpv_get_property(self.handle, name_c.as_ptr(), format, out) },
            &format!("get {name}"),
        )
    }

    /// Blocks for at most `timeout` seconds. Only one thread may call this.
    pub fn wait_event(&self, timeout: f64) -> Event {
        let event = unsafe { &*ffi::mpv_wait_event(self.handle, timeout) };
        match event.event_id {
            0 => Event::None,
            1 => Event::Shutdown,
            6 => Event::StartFile,
            7 => {
                let data = unsafe { &*(event.data as *const ffi::mpv_event_end_file) };
                Event::EndFile {
                    reason: data.reason,
                    error: data.error,
                }
            }
            8 => Event::FileLoaded,
            17 => Event::VideoReconfig,
            18 => Event::AudioReconfig,
            20 => Event::Seek,
            21 => Event::PlaybackRestart,
            22 => {
                let data = unsafe { &*(event.data as *const ffi::mpv_event_property) };
                Event::PropertyChange(
                    unsafe { CStr::from_ptr(data.name) }
                        .to_string_lossy()
                        .into(),
                )
            }
            id => Event::Other(
                unsafe { CStr::from_ptr(ffi::mpv_event_name(id)) }
                    .to_string_lossy()
                    .into(),
            ),
        }
    }

    /// Pumps events until `matches` returns true, failing on load errors or timeout.
    pub fn wait_for(
        &self,
        timeout: Duration,
        matches: impl Fn(&Event) -> bool,
    ) -> Result<Duration> {
        let started = Instant::now();
        loop {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                bail!("timed out after {timeout:?}");
            }
            let event = self.wait_event(remaining.as_secs_f64().min(0.25));
            if matches(&event) {
                return Ok(started.elapsed());
            }
            match event {
                Event::EndFile { error, .. } if error < 0 => {
                    check(error, "playback ended with error")?;
                }
                Event::Shutdown => bail!("mpv shut down"),
                _ => {}
            }
        }
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe { ffi::mpv_terminate_destroy(self.handle) };
    }
}

/// mpv's software renderer: mpv decodes, scales, converts to RGB and blends
/// subtitles/OSD into a caller-owned buffer. Requires `vo=libmpv`.
pub struct SwRenderContext {
    ctx: *mut ffi::mpv_render_context,
    update_callback: Option<Box<Box<dyn Fn() + Send + Sync>>>,
    // Must outlive the render context (mpv requires freeing it first).
    _mpv: Arc<Mpv>,
}

unsafe impl Send for SwRenderContext {}

unsafe extern "C" fn trampoline(data: *mut c_void) {
    let callback = unsafe { &*(data as *const Box<dyn Fn() + Send + Sync>) };
    callback();
}

impl SwRenderContext {
    pub fn new(mpv: Arc<Mpv>) -> Result<Self> {
        let api = cstring("sw")?;
        let mut params = [
            ffi::mpv_render_param {
                kind: ffi::MPV_RENDER_PARAM_API_TYPE,
                data: api.as_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                kind: ffi::MPV_RENDER_PARAM_INVALID,
                data: ptr::null_mut(),
            },
        ];
        let mut ctx = ptr::null_mut();
        check(
            unsafe { ffi::mpv_render_context_create(&mut ctx, mpv.handle, params.as_mut_ptr()) },
            "mpv_render_context_create(sw)",
        )?;
        Ok(Self {
            ctx,
            update_callback: None,
            _mpv: mpv,
        })
    }

    /// `callback` runs on an mpv thread; it must only signal another thread.
    pub fn set_update_callback(&mut self, callback: impl Fn() + Send + Sync + 'static) {
        let boxed: Box<Box<dyn Fn() + Send + Sync>> = Box::new(Box::new(callback));
        let data = &*boxed as *const Box<dyn Fn() + Send + Sync> as *mut c_void;
        unsafe { ffi::mpv_render_context_set_update_callback(self.ctx, Some(trampoline), data) };
        self.update_callback = Some(boxed);
    }

    /// Returns true when a new video frame should be rendered.
    pub fn needs_frame(&self) -> bool {
        unsafe { ffi::mpv_render_context_update(self.ctx) & ffi::MPV_RENDER_UPDATE_FRAME != 0 }
    }

    /// Renders the current frame as `bgr0` (B, G, R, padding) into `buffer`.
    /// With `block_for_target_time`, mpv sleeps until the frame's display time,
    /// which paces a dedicated render thread but inflates naive timings.
    pub fn render_bgr0(
        &self,
        width: u32,
        height: u32,
        buffer: &mut [u8],
        block_for_target_time: bool,
    ) -> Result<()> {
        let stride = width as usize * 4;
        if buffer.len() < stride * height as usize {
            bail!("render buffer too small");
        }
        let format = cstring("bgr0")?;
        let mut size = [width as c_int, height as c_int];
        let mut stride_param = stride;
        let mut block: c_int = block_for_target_time.into();
        let mut params = [
            ffi::mpv_render_param {
                kind: ffi::MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME,
                data: (&mut block as *mut c_int).cast(),
            },
            ffi::mpv_render_param {
                kind: ffi::MPV_RENDER_PARAM_SW_SIZE,
                data: size.as_mut_ptr().cast(),
            },
            ffi::mpv_render_param {
                kind: ffi::MPV_RENDER_PARAM_SW_FORMAT,
                data: format.as_ptr() as *mut c_void,
            },
            ffi::mpv_render_param {
                kind: ffi::MPV_RENDER_PARAM_SW_STRIDE,
                data: (&mut stride_param as *mut usize).cast(),
            },
            ffi::mpv_render_param {
                kind: ffi::MPV_RENDER_PARAM_SW_POINTER,
                data: buffer.as_mut_ptr().cast(),
            },
            ffi::mpv_render_param {
                kind: ffi::MPV_RENDER_PARAM_INVALID,
                data: ptr::null_mut(),
            },
        ];
        check(
            unsafe { ffi::mpv_render_context_render(self.ctx, params.as_mut_ptr()) },
            "mpv_render_context_render",
        )
    }
}

impl Drop for SwRenderContext {
    fn drop(&mut self) {
        unsafe {
            ffi::mpv_render_context_set_update_callback(self.ctx, None, ptr::null_mut());
            ffi::mpv_render_context_free(self.ctx);
        }
    }
}

/// A track from mpv's `track-list`, as needed for audio/subtitle selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub kind: String,
    pub id: i64,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub codec: Option<String>,
    pub selected: bool,
}

pub fn tracks(mpv: &Mpv) -> Result<Vec<Track>> {
    let count = mpv.get_i64("track-list/count")?;
    (0..count)
        .map(|index| {
            let field = |name: &str| mpv.get_string(&format!("track-list/{index}/{name}")).ok();
            Ok(Track {
                kind: field("type").unwrap_or_default(),
                id: mpv.get_i64(&format!("track-list/{index}/id"))?,
                lang: field("lang"),
                title: field("title"),
                codec: field("codec"),
                selected: mpv
                    .get_flag(&format!("track-list/{index}/selected"))
                    .unwrap_or(false),
            })
        })
        .collect()
}
