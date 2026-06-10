pub use deadsync_present::{actors, anim, cache, color, density, font, runtime};

pub mod compose;
pub mod dsl;

#[macro_export]
macro_rules! rgba {
    ($hex:literal $(,)?) => {
        $crate::engine::present::color::rgba_hex($hex)
    };
}

#[macro_export]
macro_rules! rgba_const {
    ($name:ident, $hex:literal $(,)?) => {
        const $name: [f32; 4] = $crate::engine::present::color::rgba_hex($hex);
    };
    ($vis:vis $name:ident, $hex:literal $(,)?) => {
        $vis const $name: [f32; 4] = $crate::engine::present::color::rgba_hex($hex);
    };
}
