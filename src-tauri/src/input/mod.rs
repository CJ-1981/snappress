use enigo::{Axis, Coordinate, Enigo, Mouse, Settings};

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
    // With Axis::Vertical a positive length scrolls down in enigo 0.2.
    enigo
        .scroll(ticks.abs(), Axis::Vertical)
        .map_err(|e| format!("failed to scroll: {e}"))
}
