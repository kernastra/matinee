//! libmpv client API 2.x and the software render API, loaded at runtime.
//!
//! Every foreign call in the crate lives in this module. Callers use [`Mpv`]
//! and [`SwRender`]. The library must outlive both.
//!
//! `mpv_render_context_render` is always invoked with
//! `MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME = 0`. Blocking until the frame's
//! target time belongs on the caller, not inside the render call; leaving it
//! on made a 24 fps frame look like a 39 ms render in the feasibility spike.

use std::ffi::{CStr, CString, c_char, c_int, c_ulong, c_void};
use std::ptr;
use std::sync::{Arc, Mutex};

use crate::error::PlayerError;

#[derive(Clone, Copy)]
pub struct Api {
    client_api_version: unsafe extern "C" fn() -> c_ulong,
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut c_void),
    set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    set_property_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    get_property_string: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char,
    get_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    command: unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int,
    free: unsafe extern "C" fn(*mut c_void),
    wait_event: unsafe extern "C" fn(*mut c_void, f64) -> *mut c_void,
    wakeup: unsafe extern "C" fn(*mut c_void),
    set_wakeup_callback: unsafe extern "C" fn(*mut c_void, Option<WakeFn>, *mut c_void),
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    observe_property: unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut RenderParam) -> c_int,
    render_set_update_callback: unsafe extern "C" fn(*mut c_void, Option<WakeFn>, *mut c_void),
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render: unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut c_void),
}

type WakeFn = unsafe extern "C" fn(*mut c_void);

#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct Event {
    event_id: c_int,
    error: c_int,
    _reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct EventProperty {
    name: *const c_char,
    _format: c_int,
    _data: *mut c_void,
}

#[repr(C)]
struct EventEndFile {
    reason: c_int,
    error: c_int,
    _playlist_entry_id: i64,
    _playlist_insert_id: i64,
    _playlist_insert_num_entries: c_int,
}

const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;
const FORMAT_NONE: c_int = 0;

const PARAM_INVALID: c_int = 0;
const PARAM_API_TYPE: c_int = 1;
const PARAM_BLOCK: c_int = 12;
const PARAM_SW_SIZE: c_int = 17;
const PARAM_SW_FORMAT: c_int = 18;
const PARAM_SW_STRIDE: c_int = 19;
const PARAM_SW_POINTER: c_int = 20;
const UPDATE_FRAME: u64 = 1;

pub fn bind(library: &libloading::Library) -> Result<Api, PlayerError> {
    Ok(Api {
        client_api_version: symbol(library, b"mpv_client_api_version\0")?,
        create: symbol(library, b"mpv_create\0")?,
        initialize: symbol(library, b"mpv_initialize\0")?,
        terminate_destroy: symbol(library, b"mpv_terminate_destroy\0")?,
        set_option_string: symbol(library, b"mpv_set_option_string\0")?,
        set_property_string: symbol(library, b"mpv_set_property_string\0")?,
        get_property_string: symbol(library, b"mpv_get_property_string\0")?,
        get_property: symbol(library, b"mpv_get_property\0")?,
        command: symbol(library, b"mpv_command\0")?,
        free: symbol(library, b"mpv_free\0")?,
        wait_event: symbol(library, b"mpv_wait_event\0")?,
        wakeup: symbol(library, b"mpv_wakeup\0")?,
        set_wakeup_callback: symbol(library, b"mpv_set_wakeup_callback\0")?,
        error_string: symbol(library, b"mpv_error_string\0")?,
        observe_property: symbol(library, b"mpv_observe_property\0")?,
        render_create: symbol(library, b"mpv_render_context_create\0")?,
        render_set_update_callback: symbol(library, b"mpv_render_context_set_update_callback\0")?,
        render_update: symbol(library, b"mpv_render_context_update\0")?,
        render: symbol(library, b"mpv_render_context_render\0")?,
        render_free: symbol(library, b"mpv_render_context_free\0")?,
    })
}

/// Open a shared library by soname or filesystem path.
pub fn open_library(
    name: impl AsRef<std::ffi::OsStr>,
) -> Result<libloading::Library, libloading::Error> {
    // SAFETY: `name` is a soname or a path the caller already chose. Symbols
    // are resolved by name in `bind`. The `Library` is kept until after
    // `mpv_terminate_destroy`.
    unsafe { libloading::Library::new(name) }
}

/// The mpv handle, moved once from the owner thread to the render thread.
///
/// The render thread uses it only to create a render context. The owner joins
/// that thread before `mpv_terminate_destroy`.
pub struct MpvHandle(*mut c_void);

// SAFETY: the pointer is created on the owner thread, sent exactly once, and
// the render thread is joined before the handle is destroyed. No other thread
// reads it.
unsafe impl Send for MpvHandle {}

impl MpvHandle {
    pub fn as_ptr(&self) -> *mut c_void {
        self.0
    }
}

fn symbol<T: Copy>(library: &libloading::Library, name: &[u8]) -> Result<T, PlayerError> {
    // SAFETY: `name` is a NUL-terminated literal. The pointer is copied out of
    // the symbol guard and stays valid for as long as `library` is loaded,
    // which the engine keeps alive until after the handle is destroyed.
    let found = unsafe { library.get::<T>(name) }.map_err(|error| {
        let label = String::from_utf8_lossy(name)
            .trim_end_matches('\0')
            .to_string();
        PlayerError::LibraryIncompatible {
            path: String::new(),
            detail: format!("missing {label}: {error}"),
        }
    })?;
    Ok(*found)
}

pub fn client_version(api: &Api) -> (u32, u32) {
    // SAFETY: the version query has no arguments and no caller-owned state.
    let version = unsafe { (api.client_api_version)() };
    ((version >> 16) as u32, (version & 0xffff) as u32)
}

fn check(api: &Api, code: c_int, what: &str) -> Result<(), PlayerError> {
    if code >= 0 {
        return Ok(());
    }
    // SAFETY: mpv_error_string returns a static string for every error code.
    let message = unsafe { CStr::from_ptr((api.error_string)(code)) }.to_string_lossy();
    Err(PlayerError::Engine(format!("{what}: {message} ({code})")))
}

fn cstring(value: &str) -> Result<CString, PlayerError> {
    CString::new(value).map_err(|_| PlayerError::InvalidCommand(format!("NUL in {value:?}")))
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    None,
    Shutdown,
    StartFile,
    FileLoaded,
    EndFile { reason: i32, error: i32 },
    Seek,
    PlaybackRestart,
    VideoReconfig,
    AudioReconfig,
    Property(String),
    Other(i32),
}

/// Interrupts `mpv_wait_event` from the command sender.
///
/// `mpv_wakeup` is the one client call made off the owner thread. libmpv
/// documents it as safe to call while another thread is in `mpv_wait_event`.
/// The slot is cleared, under the same mutex, before the handle is destroyed.
#[derive(Clone)]
pub struct Wakeup {
    slot: Arc<Mutex<Option<WakeSlot>>>,
}

struct WakeSlot {
    api: Api,
    handle: *mut c_void,
}

unsafe impl Send for WakeSlot {}

impl Wakeup {
    fn new() -> Self {
        Self {
            slot: Arc::new(Mutex::new(None)),
        }
    }

    pub fn poke(&self) {
        let guard = self
            .slot
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(slot) = guard.as_ref() {
            // SAFETY: the handle is still initialized. The owner clears this
            // slot before `mpv_terminate_destroy`, and that clear takes the
            // same mutex, so it cannot overlap this call.
            unsafe { (slot.api.wakeup)(slot.handle) };
        }
    }
}

pub struct Mpv {
    handle: *mut c_void,
    api: Api,
    wakeup: Wakeup,
    _wakeup_data: Option<Box<Box<dyn Fn() + Send>>>,
}

unsafe impl Send for Mpv {}

impl Mpv {
    pub fn create(api: Api, options: &[(&str, &str)]) -> Result<Self, PlayerError> {
        // SAFETY: mpv_create returns a new handle or null. No other pointer is borrowed.
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err(PlayerError::Initialization(
                "mpv_create returned null".into(),
            ));
        }
        let mpv = Self {
            handle,
            api,
            wakeup: Wakeup::new(),
            _wakeup_data: None,
        };
        for (name, value) in options {
            let name_c = cstring(name)?;
            let value_c = cstring(value)?;
            // SAFETY: both strings are NUL-terminated and owned for the call.
            // The handle is the one just created and not yet shared.
            let code =
                unsafe { (api.set_option_string)(handle, name_c.as_ptr(), value_c.as_ptr()) };
            check(&api, code, &format!("option {name}"))?;
        }
        // SAFETY: options were set before initialize, which libmpv requires for `vo`.
        check(&api, unsafe { (api.initialize)(handle) }, "mpv_initialize")?;
        Ok(mpv)
    }

    pub fn handle(&self) -> MpvHandle {
        MpvHandle(self.handle)
    }

    pub fn wakeup_handle(&self) -> Wakeup {
        self.wakeup.clone()
    }

    pub fn arm_wakeup(&mut self) {
        *self
            .wakeup
            .slot
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(WakeSlot {
            api: self.api,
            handle: self.handle,
        });
    }

    pub fn silence_wakeup(&self) {
        *self
            .wakeup
            .slot
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = None;
    }

    /// `callback` runs on an mpv thread and must not call back into mpv.
    pub fn set_wakeup_callback(&mut self, callback: impl Fn() + Send + 'static) {
        let boxed: Box<Box<dyn Fn() + Send>> = Box::new(Box::new(callback));
        let data = &*boxed as *const Box<dyn Fn() + Send> as *mut c_void;
        // SAFETY: `data` points at `boxed`, stored in `self` for the handle lifetime.
        // The trampoline only invokes that callback. It is cleared in Drop before destroy.
        unsafe { (self.api.set_wakeup_callback)(self.handle, Some(wake_trampoline), data) };
        self._wakeup_data = Some(boxed);
    }

    pub fn command(&self, args: &[&str]) -> Result<(), PlayerError> {
        let owned = args
            .iter()
            .map(|arg| cstring(arg))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pointers: Vec<*const c_char> = owned
            .iter()
            .map(CString::as_c_str)
            .map(CStr::as_ptr)
            .collect();
        pointers.push(ptr::null());
        // SAFETY: the pointer array is NUL-terminated and each string outlives the call.
        let code = unsafe { (self.api.command)(self.handle, pointers.as_mut_ptr()) };
        check(&self.api, code, &format!("command {args:?}"))
    }

    pub fn set(&self, name: &str, value: &str) -> Result<(), PlayerError> {
        let name_c = cstring(name)?;
        let value_c = cstring(value)?;
        // SAFETY: both CStrings outlive the call. The handle belongs to this owner thread.
        let code = unsafe {
            (self.api.set_property_string)(self.handle, name_c.as_ptr(), value_c.as_ptr())
        };
        check(&self.api, code, &format!("set {name}"))
    }

    pub fn get_string(&self, name: &str) -> Result<String, PlayerError> {
        let name_c = cstring(name)?;
        // SAFETY: the returned allocation is freed with mpv_free before this function returns.
        let raw = unsafe { (self.api.get_property_string)(self.handle, name_c.as_ptr()) };
        if raw.is_null() {
            return Err(PlayerError::Engine(format!("property {name} unavailable")));
        }
        let value = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.api.free)(raw.cast()) };
        Ok(value)
    }

    pub fn get_f64(&self, name: &str) -> Result<f64, PlayerError> {
        let mut value = 0f64;
        self.get_raw(name, FORMAT_DOUBLE, (&mut value as *mut f64).cast())?;
        Ok(value)
    }

    pub fn get_i64(&self, name: &str) -> Result<i64, PlayerError> {
        let mut value = 0i64;
        self.get_raw(name, FORMAT_INT64, (&mut value as *mut i64).cast())?;
        Ok(value)
    }

    pub fn get_flag(&self, name: &str) -> Result<bool, PlayerError> {
        let mut value: c_int = 0;
        self.get_raw(name, FORMAT_FLAG, (&mut value as *mut c_int).cast())?;
        Ok(value != 0)
    }

    pub fn observe(&self, name: &str, reply: u64) -> Result<(), PlayerError> {
        let name_c = cstring(name)?;
        // SAFETY: the name outlives the call. Format NONE asks for notifications without a value copy.
        let code = unsafe {
            (self.api.observe_property)(self.handle, reply, name_c.as_ptr(), FORMAT_NONE)
        };
        check(&self.api, code, &format!("observe {name}"))
    }

    fn get_raw(&self, name: &str, format: c_int, out: *mut c_void) -> Result<(), PlayerError> {
        let name_c = cstring(name)?;
        // SAFETY: `out` points at a caller-owned value of the requested format.
        let code = unsafe { (self.api.get_property)(self.handle, name_c.as_ptr(), format, out) };
        check(&self.api, code, &format!("get {name}"))
    }

    /// Only the owner thread calls this.
    pub fn wait_event(&self, timeout: f64) -> EngineEvent {
        // SAFETY: the returned event is borrowed only until the next wait on this handle.
        // Fields are copied out before this function returns.
        let event = unsafe { &*(self.api.wait_event)(self.handle, timeout).cast::<Event>() };
        match event.event_id {
            0 => EngineEvent::None,
            1 => EngineEvent::Shutdown,
            6 => EngineEvent::StartFile,
            7 => {
                let end = unsafe { &*event.data.cast::<EventEndFile>() };
                EngineEvent::EndFile {
                    reason: end.reason,
                    error: end.error,
                }
            }
            8 => EngineEvent::FileLoaded,
            17 => EngineEvent::VideoReconfig,
            18 => EngineEvent::AudioReconfig,
            20 => EngineEvent::Seek,
            21 => EngineEvent::PlaybackRestart,
            22 => {
                let property = unsafe { &*event.data.cast::<EventProperty>() };
                let name = unsafe { CStr::from_ptr(property.name) }
                    .to_string_lossy()
                    .into_owned();
                EngineEvent::Property(name)
            }
            id => EngineEvent::Other(id),
        }
    }
}

unsafe extern "C" fn wake_trampoline(data: *mut c_void) {
    // SAFETY: `data` is the box stored on `Mpv` and is not freed until the callback is cleared.
    let callback = unsafe { &*(data as *const Box<dyn Fn() + Send>) };
    callback();
}

impl Drop for Mpv {
    fn drop(&mut self) {
        self.silence_wakeup();
        if !self.handle.is_null() {
            // SAFETY: the wakeup callback is cleared before the handle is destroyed,
            // so the trampoline cannot run against freed callback state.
            unsafe {
                (self.api.set_wakeup_callback)(self.handle, None, ptr::null_mut());
                (self.api.terminate_destroy)(self.handle);
            }
            self.handle = ptr::null_mut();
        }
    }
}

/// Software renderer. Create it before `loadfile`, and free it before the handle.
pub struct SwRender {
    ctx: *mut c_void,
    api: Api,
    _callback: Option<Box<Box<dyn Fn() + Send + Sync>>>,
}

unsafe impl Send for SwRender {}

impl SwRender {
    pub fn new(api: Api, mpv: *mut c_void) -> Result<Self, PlayerError> {
        let kind = cstring("sw")?;
        let mut params = [
            RenderParam {
                kind: PARAM_API_TYPE,
                data: kind.as_ptr() as *mut c_void,
            },
            RenderParam {
                kind: PARAM_INVALID,
                data: ptr::null_mut(),
            },
        ];
        let mut ctx = ptr::null_mut();
        // SAFETY: `mpv` is a live handle, `params` is terminated, and the API
        // string outlives the call. The context is freed in Drop.
        let code = unsafe { (api.render_create)(&mut ctx, mpv, params.as_mut_ptr()) };
        check(&api, code, "mpv_render_context_create")?;
        if ctx.is_null() {
            return Err(PlayerError::Initialization(
                "render context was null".into(),
            ));
        }
        Ok(Self {
            ctx,
            api,
            _callback: None,
        })
    }

    /// `callback` runs on an mpv thread and must only signal another thread.
    pub fn set_update_callback(&mut self, callback: impl Fn() + Send + Sync + 'static) {
        let boxed: Box<Box<dyn Fn() + Send + Sync>> = Box::new(Box::new(callback));
        let data = &*boxed as *const Box<dyn Fn() + Send + Sync> as *mut c_void;
        // SAFETY: `boxed` is stored on self and outlives the context. The trampoline
        // does not touch mpv.
        unsafe { (self.api.render_set_update_callback)(self.ctx, Some(render_trampoline), data) };
        self._callback = Some(boxed);
    }

    pub fn needs_frame(&self) -> bool {
        // SAFETY: update is called only from the render thread that owns this context.
        let flags = unsafe { (self.api.render_update)(self.ctx) };
        flags & UPDATE_FRAME != 0
    }

    /// Render `bgr0` into `buffer`. Does not block for the frame's target time.
    pub fn render_bgr0(
        &self,
        width: u32,
        height: u32,
        buffer: &mut [u8],
    ) -> Result<(), PlayerError> {
        let stride = (width as usize)
            .checked_mul(4)
            .ok_or_else(|| PlayerError::Engine("render stride overflow".into()))?;
        let needed = stride
            .checked_mul(height as usize)
            .ok_or_else(|| PlayerError::Engine("render buffer overflow".into()))?;
        if buffer.len() < needed {
            return Err(PlayerError::Engine("render buffer too small".into()));
        }
        let format = cstring("bgr0")?;
        let mut size = [width as c_int, height as c_int];
        let mut stride_param = stride;
        let mut block: c_int = 0;
        let mut params = [
            RenderParam {
                kind: PARAM_BLOCK,
                data: (&mut block as *mut c_int).cast(),
            },
            RenderParam {
                kind: PARAM_SW_SIZE,
                data: size.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_SW_FORMAT,
                data: format.as_ptr() as *mut c_void,
            },
            RenderParam {
                kind: PARAM_SW_STRIDE,
                data: (&mut stride_param as *mut usize).cast(),
            },
            RenderParam {
                kind: PARAM_SW_POINTER,
                data: buffer.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_INVALID,
                data: ptr::null_mut(),
            },
        ];
        // SAFETY: the buffer is writable for `stride * height` bytes. Size, format,
        // stride, and pointer match the software render contract. `block` is 0 so
        // the call returns without sleeping until the presentation timestamp.
        let code = unsafe { (self.api.render)(self.ctx, params.as_mut_ptr()) };
        check(&self.api, code, "mpv_render_context_render")
    }
}

unsafe extern "C" fn render_trampoline(data: *mut c_void) {
    // SAFETY: `data` is the box stored on `SwRender` until the callback is cleared.
    let callback = unsafe { &*(data as *const Box<dyn Fn() + Send + Sync>) };
    callback();
}

impl Drop for SwRender {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            // SAFETY: the update callback is cleared before the context is freed,
            // which is what libmpv requires before the player handle is destroyed.
            unsafe {
                (self.api.render_set_update_callback)(self.ctx, None, ptr::null_mut());
                (self.api.render_free)(self.ctx);
            }
            self.ctx = ptr::null_mut();
        }
    }
}
