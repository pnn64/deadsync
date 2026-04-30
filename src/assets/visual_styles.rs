pub const HEARTS_SELECT_COLOR: &str = "visual_styles/hearts/select_color.png";
pub const HEARTS_SHARED_BACKGROUND: &str = "visual_styles/hearts/shared_background.png";

#[inline(always)]
pub const fn select_color_texture_key() -> &'static str {
    HEARTS_SELECT_COLOR
}

#[inline(always)]
pub const fn shared_background_texture_key() -> &'static str {
    HEARTS_SHARED_BACKGROUND
}
