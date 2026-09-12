//! The only `unsafe` code in Alpymist.
//!
//! Keep this module small enough to audit in one sitting. It does exactly one
//! thing: load libEGL, stand up a throwaway GL ES context, read three strings
//! out of it, and tear everything down again.

#![allow(unsafe_code)]

use khronos_egl as egl;
use std::ffi::{CStr, c_char};

/// `GL_VENDOR`, `GL_RENDERER`, `GL_VERSION` — from the GL headers.
const GL_VENDOR: u32 = 0x1F00;
const GL_RENDERER: u32 = 0x1F01;
const GL_VERSION: u32 = 0x1F02;

/// `EGL_PLATFORM_SURFACELESS_MESA`, from `EGL_MESA_platform_surfaceless`.
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

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
/// Returns `None` at the first sign of trouble; a probe that cannot answer is
/// not an error condition, it is just an absence of information.
pub(crate) fn query() -> Option<RawStrings> {
    // SAFETY: dlopen of a system library by soname. Unsound only if the host's
    // libEGL is itself malicious, in which case we have already lost.
    let lib = unsafe { libloading::Library::new("libEGL.so.1") }.ok()?;
    // SAFETY: resolves EGL entry points from the library just opened. The
    // symbols and their signatures are fixed by the EGL ABI.
    let egl = unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required_from(lib) }.ok()?;

    let display = open_display(&egl)?;
    egl.initialize(display).ok()?;

    let result = with_context(&egl, display);

    // Best-effort teardown; a failure here cannot affect the answer.
    let _ = egl.terminate(display);
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
fn open_display(egl: &egl::DynamicInstance<egl::EGL1_4>) -> Option<egl::Display> {
    if let Some(egl15) = egl.upcast::<egl::EGL1_5>() {
        // SAFETY: the surfaceless platform takes the null native display, and
        // ATTRIB_NONE is a correctly terminated empty attribute list. An
        // unsupported platform is reported as an error, not undefined behaviour.
        let display = unsafe {
            egl15.get_platform_display(
                PLATFORM_SURFACELESS_MESA,
                egl::DEFAULT_DISPLAY,
                &[egl::ATTRIB_NONE],
            )
        };
        if let Ok(display) = display {
            return Some(display);
        }
    }
    // SAFETY: EGL_DEFAULT_DISPLAY is the null native display, always valid.
    unsafe { egl.get_display(egl::DEFAULT_DISPLAY) }
}

/// Everything between `eglInitialize` and `eglTerminate`.
fn with_context(
    egl: &egl::DynamicInstance<egl::EGL1_4>,
    display: egl::Display,
) -> Option<RawStrings> {
    egl.bind_api(egl::OPENGL_ES_API).ok()?;

    let config = egl
        .choose_first_config(
            display,
            &[
                egl::SURFACE_TYPE,
                egl::PBUFFER_BIT,
                egl::RENDERABLE_TYPE,
                egl::OPENGL_ES2_BIT,
                egl::NONE,
            ],
        )
        .ok()??;

    // Ask for ES 3 first. Old drivers — exactly the hardware Alpymist targets —
    // will refuse, and we retry at ES 2 rather than reporting nothing.
    let context = [3, 2].into_iter().find_map(|version| {
        egl.create_context(
            display,
            config,
            None,
            &[egl::CONTEXT_CLIENT_VERSION, version, egl::NONE],
        )
        .ok()
    })?;

    // Surfaceless needs EGL_KHR_surfaceless_context, which old drivers lack;
    // fall back to a 1x1 pbuffer, which every pbuffer-capable config supports.
    let surface = if egl.make_current(display, None, None, Some(context)).is_ok() {
        None
    } else {
        let pbuffer = egl
            .create_pbuffer_surface(display, config, &[egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE])
            .ok()?;
        egl.make_current(display, Some(pbuffer), Some(pbuffer), Some(context))
            .ok()?;
        Some(pbuffer)
    };

    let strings = read_gl_strings(egl);

    let _ = egl.make_current(display, None, None, None);
    if let Some(pbuffer) = surface {
        let _ = egl.destroy_surface(display, pbuffer);
    }
    let _ = egl.destroy_context(display, context);
    strings
}

/// Read the three `glGetString` values from the current context.
fn read_gl_strings(egl: &egl::DynamicInstance<egl::EGL1_4>) -> Option<RawStrings> {
    let proc = egl.get_proc_address("glGetString")?;
    // SAFETY: eglGetProcAddress returned a non-null pointer for the name
    // "glGetString", whose signature is fixed by the GL ES ABI as
    // `const GLubyte *glGetString(GLenum)`. Transmuting a fn pointer to that
    // signature is the documented way to use the result.
    let gl_get_string: GlGetString = unsafe { std::mem::transmute(proc) };

    Some(RawStrings {
        version: get_string(gl_get_string, GL_VERSION)?,
        renderer: get_string(gl_get_string, GL_RENDERER)?,
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
