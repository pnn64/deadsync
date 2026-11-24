/// Dimensions and refresh rate of an advertised host video mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoMode {
    pub width: u32,
    pub height: u32,
    pub refresh_rate_millihertz: u32,
}
