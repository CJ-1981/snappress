use enigo::{Coordinate, Enigo, Mouse, Settings};

fn new_enigo() -> Result<Enigo, String> {
    Enigo::new(&Settings::default()).map_err(|e| format!("input injection unavailable: {e}"))
}

/// Move the absolute cursor to screen coordinates (physical pixels).
/// On macOS this requires the Accessibility permission.
pub fn move_cursor(x: i32, y: i32) -> Result<(), String> {
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("failed to move cursor: {e}"))
}

/// Scroll the mouse wheel down by the given number of wheel ticks.
pub fn scroll_down(ticks: i32) -> Result<(), String> {
    let mut enigo = new_enigo()?;
    // Negative vertical value = wheel down (content moves up) in enigo.
    enigo
        .scroll(0, -ticks.abs())
        .map_err(|e| format!("failed to scroll: {e}"))
}
