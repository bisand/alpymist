//! Print the chosen text scale and body rows for each supported size.
use alpymist_ui::chrome::Chrome;
fn main() {
    for (w, h) in [
        (640, 480),
        (800, 600),
        (1024, 600),
        (1024, 768),
        (1280, 800),
        (1366, 768),
        (1920, 1080),
        (2560, 1440),
    ] {
        let c = Chrome::for_screen(w, h);
        let bottom = i32::try_from(h).unwrap() - (c.panel.1 + c.panel.3);
        println!(
            "{w:>4}x{h:<4} scale {}  rows {:>2}  bottom {:>3}px  side {:>3}px",
            c.text_scale,
            c.body_rows(),
            bottom,
            c.panel.0
        );
    }
}
