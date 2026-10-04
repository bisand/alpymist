//! Verbose walk through the EGL probe, for diagnosing failures on real hardware.
//!
//! The probe itself, saying each step as it takes it: the last line printed
//! before the answer is the step that failed.

fn main() {
    match alpymist_glesprobe::diagnose(&mut |line| println!("{line}")) {
        Ok(gles) => println!("{gles:?}"),
        Err(e) => println!("no answer: {e}"),
    }
}
