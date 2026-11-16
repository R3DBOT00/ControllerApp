//! Graphite/green theme and logical-pixel sizing, independent of device scale.
pub const GREEN: [u8; 3] = [132, 202, 36];
pub const TEXT: [u8; 3] = [239, 243, 235];
pub const MUTED: [u8; 3] = [154, 164, 152];
pub const PANEL: [u8; 3] = [27, 30, 28];
pub const KEY: [u8; 3] = [46, 50, 46];
pub const SPECIAL: [u8; 3] = [54, 59, 51];
pub const BORDER: [u8; 3] = [67, 74, 63];

/// GDK already reports logical pixels: do not multiply by monitor scale again.
/// The compositor may further constrain this request to its available area.
pub fn overlay_size(monitor_width: i32, monitor_height: i32) -> (i32, i32) {
    let width = (monitor_width - 32).clamp(1, 850);
    let desired_height = if width < 760 { 294 } else { 270 };
    (width, desired_height.min((monitor_height - 100).max(1)))
}

#[derive(Clone, Copy)]
pub struct Metrics {
    pub scale: f64,
    pub inset: f64,
    pub grid_top: f64,
    pub grid_bottom: f64,
    pub footer_top: f64,
    pub narrow: bool,
}
impl Metrics {
    pub fn new(width: f64, height: f64) -> Self {
        let scale = (width / 700.).min(height / 270.).min(1.);
        let narrow = width < 760.;
        let footer_height = if narrow { 50. } else { 28. } * scale;
        let footer_top = height - 12. * scale - footer_height;
        Self {
            scale,
            inset: 20. * scale,
            grid_top: 77. * scale,
            grid_bottom: footer_top - 8. * scale,
            footer_top,
            narrow,
        }
    }
}
