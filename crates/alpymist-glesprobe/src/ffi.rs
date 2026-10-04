//! The `unsafe` code in Alpymist, but for `alpymist-pam`'s.
//!
//! Keep this module small enough to audit in one sitting. It does exactly one
//! thing: load libEGL, stand up a throwaway GL ES context, read three strings
//! out of it, and tear everything down again.

#![allow(unsafe_code)]

use crate::ProbeError;
use std::ffi::{CStr, c_char, c_uint, c_void};
use std::ptr;

/// `GL_VENDOR`, `GL_RENDERER`, `GL_VERSION` — from the GL headers.
const GL_VENDOR: u32 = 0x1F00;
const GL_RENDERER: u32 = 0x1F01;
const GL_VERSION: u32 = 0x1F02;

/// EGL's own types, from `EGL/egl.h` and `EGL/eglplatform.h`. A display, a
/// config, a context and a surface are handles only EGL looks inside; none of
/// them is null when it is real.
type Display = *mut c_void;
type Config = *mut c_void;
type Context = *mut c_void;
type Surface = *mut c_void;
type Int = i32;
type Boolean = c_uint;
type Enum = c_uint;

/// `EGL_PLATFORM_SURFACELESS_MESA`, from `EGL_MESA_platform_surfaceless`.
const PLATFORM_SURFACELESS_MESA: Enum = 0x31DD;

/// The constants used, from `EGL/egl.h`.
const PBUFFER_BIT: Int = 0x0001;
const OPENGL_ES2_BIT: Int = 0x0004;
const SURFACE_TYPE: Int = 0x3033;
const NONE: Int = 0x3038;
const RENDERABLE_TYPE: Int = 0x3040;
const HEIGHT: Int = 0x3056;
const WIDTH: Int = 0x3057;
const CONTEXT_CLIENT_VERSION: Int = 0x3098;
const OPENGL_ES_API: Enum = 0x30A0;

/// The entry points used, each as the EGL 1.4 ABI declares it. Only
/// `eglGetPlatformDisplay` is newer, and so may be missing.
struct Egl {
    get_platform_display:
        Option<unsafe extern "system" fn(Enum, *mut c_void, *const isize) -> Display>,
    get_display: unsafe extern "system" fn(*mut c_void) -> Display,
    initialize: unsafe extern "system" fn(Display, *mut Int, *mut Int) -> Boolean,
    terminate: unsafe extern "system" fn(Display) -> Boolean,
    bind_api: unsafe extern "system" fn(Enum) -> Boolean,
    choose_config:
        unsafe extern "system" fn(Display, *const Int, *mut Config, Int, *mut Int) -> Boolean,
    create_context: unsafe extern "system" fn(Display, Config, Context, *const Int) -> Context,
    destroy_context: unsafe extern "system" fn(Display, Context) -> Boolean,
    create_pbuffer_surface: unsafe extern "system" fn(Display, Config, *const Int) -> Surface,
    destroy_surface: unsafe extern "system" fn(Display, Surface) -> Boolean,
    make_current: unsafe extern "system" fn(Display, Surface, Surface, Context) -> Boolean,
    get_proc_address: unsafe extern "system" fn(*const c_char) -> *const c_void,
    /// The library they are in: they are good for as long as it is open.
    _library: libloading::Library,
}

impl Egl {
    /// Resolve the entry points from `library`; `None` when one that EGL 1.4
    /// promises is not there.
    fn load(library: libloading::Library) -> Option<Self> {
        // SAFETY: each symbol is looked up by the name the EGL ABI gives it
        // and given the signature that ABI fixes for that name. The pointers
        // are copied out of their `Symbol`s and kept beside the library, which
        // is dropped last, so none outlives the code it points into.
        unsafe {
            Some(Self {
                get_platform_display: library.get(b"eglGetPlatformDisplay\0").ok().map(|s| *s),
                get_display: *library.get(b"eglGetDisplay\0").ok()?,
                initialize: *library.get(b"eglInitialize\0").ok()?,
                terminate: *library.get(b"eglTerminate\0").ok()?,
                bind_api: *library.get(b"eglBindAPI\0").ok()?,
                choose_config: *library.get(b"eglChooseConfig\0").ok()?,
                create_context: *library.get(b"eglCreateContext\0").ok()?,
                destroy_context: *library.get(b"eglDestroyContext\0").ok()?,
                create_pbuffer_surface: *library.get(b"eglCreatePbufferSurface\0").ok()?,
                destroy_surface: *library.get(b"eglDestroySurface\0").ok()?,
                make_current: *library.get(b"eglMakeCurrent\0").ok()?,
                get_proc_address: *library.get(b"eglGetProcAddress\0").ok()?,
                _library: library,
            })
        }
    }
}

/// Signature of `glGetString`, resolved at runtime via `eglGetProcAddress`.
type GlGetString = unsafe extern "system" fn(u32) -> *const c_char;

/// The three strings we came for, still unparsed.
pub(crate) struct RawStrings {
    pub version: String,
    pub renderer: String,
    pub vendor: String,
}

/// Load libEGL, create a context, and read the GL strings.
///
/// Each failure is reported as the step it happened at, so a user landing on an
/// unexpected tier can tell "Mesa is not installed" from "this GPU has no
/// working driver". `say` is told each step as it is taken and how it went,
/// for whoever is reading along.
pub(crate) fn query(say: &mut dyn FnMut(&str)) -> Result<RawStrings, ProbeError> {
    // SAFETY: dlopen of a system library by soname. Unsound only if the host's
    // libEGL is itself malicious, in which case we have already lost.
    let library = unsafe { libloading::Library::new("libEGL.so.1") }.map_err(|e| {
        say(&format!("dlopen libEGL.so.1: FAILED {e}"));
        ProbeError::NoLibrary(e.to_string())
    })?;
    say("dlopen libEGL.so.1: ok");
    let Some(egl) = Egl::load(library) else {
        say("EGL 1.4 entry points: MISSING");
        return Err(ProbeError::MissingSymbols);
    };
    say("EGL 1.4 entry points: ok");

    let display = open_display(&egl, say).ok_or(ProbeError::NoDisplay)?;
    let (mut major, mut minor) = (0, 0);
    // SAFETY: `display` is one EGL just returned, and the two pointers are to
    // integers that live until the call is over.
    if unsafe { (egl.initialize)(display, &raw mut major, &raw mut minor) } == 0 {
        say("eglInitialize: FAILED");
        return Err(ProbeError::InitFailed);
    }
    say(&format!("eglInitialize: ok, EGL {major}.{minor}"));

    let result = with_context(&egl, display, say);

    // Best-effort teardown; a failure here cannot affect the answer.
    // SAFETY: `display` was initialised above and is not used after this.
    unsafe { (egl.terminate)(display) };
    result
}

/// Open an EGL display without needing a display server.
///
/// The surfaceless platform is tried first: it is the only one that works on a
/// machine with no compositor running, which is exactly where this probe runs
/// (the installer, and the first-boot service). On a machine that does have a
/// GPU it still opens the real render node, so the answer is not biased towards
/// software rendering — a headless container reports llvmpipe because llvmpipe
/// is genuinely all it has.
///
/// Falls back to `eglGetDisplay` for drivers too old to offer the extension.
fn open_display(egl: &Egl, say: &mut dyn FnMut(&str)) -> Option<Display> {
    if let Some(get_platform_display) = egl.get_platform_display {
        // SAFETY: the surfaceless platform takes the null native display, and
        // a null attribute list is an empty one. An unsupported platform is
        // reported as no display, not undefined behaviour.
        let display = unsafe {
            get_platform_display(PLATFORM_SURFACELESS_MESA, ptr::null_mut(), ptr::null())
        };
        if !display.is_null() {
            say("eglGetPlatformDisplay(surfaceless): ok");
            return Some(display);
        }
        say("eglGetPlatformDisplay(surfaceless): FAILED");
    } else {
        say("eglGetPlatformDisplay: not in this libEGL");
    }
    // SAFETY: EGL_DEFAULT_DISPLAY is the null native display, always valid.
    let display = unsafe { (egl.get_display)(ptr::null_mut()) };
    say(if display.is_null() {
        "eglGetDisplay(default): FAILED"
    } else {
        "eglGetDisplay(default): ok"
    });
    (!display.is_null()).then_some(display)
}

/// Everything between `eglInitialize` and `eglTerminate`.
fn with_context(
    egl: &Egl,
    display: Display,
    say: &mut dyn FnMut(&str),
) -> Result<RawStrings, ProbeError> {
    // SAFETY: takes an enumerated value and nothing else.
    if unsafe { (egl.bind_api)(OPENGL_ES_API) } == 0 {
        say("eglBindAPI(GL ES): FAILED");
        return Err(ProbeError::InitFailed);
    }

    let attributes = [
        SURFACE_TYPE,
        PBUFFER_BIT,
        RENDERABLE_TYPE,
        OPENGL_ES2_BIT,
        NONE,
    ];
    let mut config: Config = ptr::null_mut();
    let mut count: Int = 0;
    // SAFETY: the attribute list ends in EGL_NONE; there is room for the one
    // config asked for, and for the count of those written.
    let chosen = unsafe {
        (egl.choose_config)(
            display,
            attributes.as_ptr(),
            &raw mut config,
            1,
            &raw mut count,
        )
    };
    if chosen == 0 || count < 1 || config.is_null() {
        say("eglChooseConfig: NO MATCHING CONFIG");
        return Err(ProbeError::NoConfig);
    }
    say("eglChooseConfig: ok");

    // Ask for ES 3 first. Old drivers — exactly the hardware Alpymist targets —
    // will refuse, and we retry at ES 2 rather than reporting nothing.
    let context = [3, 2]
        .into_iter()
        .find_map(|version| {
            let attributes = [CONTEXT_CLIENT_VERSION, version, NONE];
            // SAFETY: `display` and `config` are EGL's own; no context is
            // shared with, which is what the null one says; the attribute
            // list ends in EGL_NONE.
            let context = unsafe {
                (egl.create_context)(display, config, ptr::null_mut(), attributes.as_ptr())
            };
            say(&format!(
                "eglCreateContext(ES {version}): {}",
                if context.is_null() { "FAILED" } else { "ok" }
            ));
            (!context.is_null()).then_some(context)
        })
        .ok_or(ProbeError::NoContext)?;

    let strings = with_current(egl, display, config, context, say);

    // SAFETY: `context` was made above, is no longer current, and is not used
    // after this.
    unsafe { (egl.destroy_context)(display, context) };
    strings
}

/// Make `context` current, read the strings, and let it go again.
fn with_current(
    egl: &Egl,
    display: Display,
    config: Config,
    context: Context,
    say: &mut dyn FnMut(&str),
) -> Result<RawStrings, ProbeError> {
    let none: Surface = ptr::null_mut();
    // Surfaceless needs EGL_KHR_surfaceless_context, which old drivers lack;
    // fall back to a 1x1 pbuffer, which every pbuffer-capable config supports.
    // SAFETY: `display` and `context` are EGL's own, and no surface is what
    // the null ones say. A driver that cannot do it says so.
    let surface = if unsafe { (egl.make_current)(display, none, none, context) } != 0 {
        say("eglMakeCurrent(surfaceless): ok");
        none
    } else {
        say("eglMakeCurrent(surfaceless): FAILED, trying a pbuffer");
        let attributes = [WIDTH, 1, HEIGHT, 1, NONE];
        // SAFETY: `display` and `config` are EGL's own, and the attribute list
        // ends in EGL_NONE.
        let pbuffer = unsafe { (egl.create_pbuffer_surface)(display, config, attributes.as_ptr()) };
        if pbuffer.is_null() {
            say("eglCreatePbufferSurface: FAILED");
            return Err(ProbeError::NoCurrent);
        }
        // SAFETY: as above, with the surface just made.
        if unsafe { (egl.make_current)(display, pbuffer, pbuffer, context) } == 0 {
            say("eglMakeCurrent(pbuffer): FAILED");
            // SAFETY: the surface was made above and is not used after this.
            unsafe { (egl.destroy_surface)(display, pbuffer) };
            return Err(ProbeError::NoCurrent);
        }
        say("eglMakeCurrent(pbuffer): ok");
        pbuffer
    };

    let strings = read_gl_strings(egl, say);

    // SAFETY: nothing current is what the three nulls say; the surface, where
    // there is one, was made above and is not used after this.
    unsafe {
        (egl.make_current)(display, none, none, ptr::null_mut());
        if !surface.is_null() {
            (egl.destroy_surface)(display, surface);
        }
    }
    strings
}

/// Read the three `glGetString` values from the current context.
fn read_gl_strings(egl: &Egl, say: &mut dyn FnMut(&str)) -> Result<RawStrings, ProbeError> {
    // SAFETY: the name is a NUL-terminated string that lives for the call.
    let proc = unsafe { (egl.get_proc_address)(c"glGetString".as_ptr()) };
    if proc.is_null() {
        say("eglGetProcAddress(glGetString): NULL");
        return Err(ProbeError::NoStrings);
    }
    // SAFETY: eglGetProcAddress returned a non-null pointer for the name
    // "glGetString", whose signature is fixed by the GL ES ABI as
    // `const GLubyte *glGetString(GLenum)`. Transmuting a fn pointer to that
    // signature is the documented way to use the result.
    let gl_get_string: GlGetString = unsafe { std::mem::transmute(proc) };

    let version = get_string(gl_get_string, GL_VERSION);
    let renderer = get_string(gl_get_string, GL_RENDERER);
    for (name, value) in [("GL_VERSION", &version), ("GL_RENDERER", &renderer)] {
        say(&format!("  {name}: {}", value.as_deref().unwrap_or("NULL")));
    }
    Ok(RawStrings {
        version: version.ok_or(ProbeError::NoStrings)?,
        renderer: renderer.ok_or(ProbeError::NoStrings)?,
        vendor: get_string(gl_get_string, GL_VENDOR).unwrap_or_default(),
    })
}

/// Call `glGetString` and copy the result into an owned `String`.
fn get_string(gl_get_string: GlGetString, name: u32) -> Option<String> {
    // SAFETY: a context is current (the caller just made one so), and
    // `name` is one of the three constants GL guarantees to accept.
    let ptr = unsafe { gl_get_string(name) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: GL guarantees a NUL-terminated static string, valid for the
    // lifetime of the context, which outlives this copy.
    let bytes = unsafe { CStr::from_ptr(ptr) };
    Some(bytes.to_string_lossy().into_owned())
}
