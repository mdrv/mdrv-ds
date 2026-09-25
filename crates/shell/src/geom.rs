/// Clamp a numpad-style overlay position (1=top-left … 9=bottom-right).
pub fn clamp_position(v: u8) -> u8 {
    v.clamp(1, 9)
}
