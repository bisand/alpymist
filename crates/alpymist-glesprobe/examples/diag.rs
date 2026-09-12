//! Verbose walk through the EGL probe, for diagnosing failures on real hardware.
#![allow(unsafe_code, missing_docs)]
use khronos_egl as egl;

// Deliberately one linear sequence: it mirrors the order EGL calls happen in,
// so the last line printed is the step that failed.
#[allow(clippy::too_many_lines)]
fn main() {
    let lib = match unsafe { libloading::Library::new("libEGL.so.1") } {
        Ok(l) => {
            println!("dlopen libEGL.so.1: ok");
            l
        }
        Err(e) => {
            println!("dlopen libEGL.so.1: FAILED {e}");
            return;
        }
    };
    let inst = match unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required_from(lib) } {
        Ok(i) => {
            println!("load EGL1_4 symbols: ok");
            i
        }
        Err(e) => {
            println!("load EGL1_4 symbols: FAILED {e}");
            return;
        }
    };
    println!(
        "upcast to EGL1_5: {}",
        if inst.upcast::<egl::EGL1_5>().is_some() {
            "ok"
        } else {
            "NOT AVAILABLE"
        }
    );

    let display = if let Some(e15) = inst.upcast::<egl::EGL1_5>() {
        match unsafe { e15.get_platform_display(0x31DD, egl::DEFAULT_DISPLAY, &[egl::ATTRIB_NONE]) }
        {
            Ok(d) => {
                println!("get_platform_display(SURFACELESS): ok");
                Some(d)
            }
            Err(e) => {
                println!("get_platform_display(SURFACELESS): FAILED {e}");
                None
            }
        }
    } else {
        None
    };

    let display = display.or_else(|| {
        let d = unsafe { inst.get_display(egl::DEFAULT_DISPLAY) };
        println!(
            "get_display(DEFAULT): {}",
            if d.is_some() { "ok" } else { "FAILED" }
        );
        d
    });
    let Some(display) = display else { return };

    match inst.initialize(display) {
        Ok((maj, min)) => println!("eglInitialize: ok, EGL {maj}.{min}"),
        Err(e) => {
            println!("eglInitialize: FAILED {e}");
            return;
        }
    }
    println!("bind_api: {:?}", inst.bind_api(egl::OPENGL_ES_API));

    let config = match inst.choose_first_config(
        display,
        &[
            egl::SURFACE_TYPE,
            egl::PBUFFER_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES2_BIT,
            egl::NONE,
        ],
    ) {
        Ok(Some(c)) => {
            println!("choose_first_config: ok");
            c
        }
        Ok(None) => {
            println!("choose_first_config: NO MATCHING CONFIG");
            return;
        }
        Err(e) => {
            println!("choose_first_config: FAILED {e}");
            return;
        }
    };
    for v in [3, 2] {
        match inst.create_context(
            display,
            config,
            None,
            &[egl::CONTEXT_CLIENT_VERSION, v, egl::NONE],
        ) {
            Ok(ctx) => {
                println!("create_context(ES{v}): ok");
                println!(
                    "make_current(surfaceless): {:?}",
                    inst.make_current(display, None, None, Some(ctx))
                );
                let p = inst.get_proc_address("glGetString");
                println!(
                    "get_proc_address(glGetString): {}",
                    if p.is_some() { "ok" } else { "NULL" }
                );
                if let Some(p) = p {
                    let f: unsafe extern "system" fn(u32) -> *const std::ffi::c_char =
                        unsafe { std::mem::transmute(p) };
                    for (n, name) in [(0x1F02u32, "GL_VERSION"), (0x1F01, "GL_RENDERER")] {
                        let ptr = unsafe { f(n) };
                        if ptr.is_null() {
                            println!("  {name}: NULL");
                        } else {
                            println!(
                                "  {name}: {}",
                                unsafe { std::ffi::CStr::from_ptr(ptr) }.to_string_lossy()
                            );
                        }
                    }
                }
                return;
            }
            Err(e) => println!("create_context(ES{v}): FAILED {e}"),
        }
    }
}
