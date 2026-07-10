use deadlib_render::BackendType;
use winit::{
    dpi::PhysicalSize,
    window::{Window, WindowAttributes},
};

#[cfg(target_os = "macos")]
use winit::{dpi::LogicalSize, platform::macos::WindowAttributesExtMacOS};

#[cfg(target_os = "macos")]
#[inline(always)]
const fn macos_opengl_low_dpi(backend_type: BackendType, high_dpi: bool) -> bool {
    backend_type == BackendType::OpenGL && !high_dpi
}

#[cfg(not(target_os = "macos"))]
#[inline(always)]
const fn macos_opengl_low_dpi(_backend_type: BackendType, _high_dpi: bool) -> bool {
    false
}

fn logical_px_for_physical(px: u32, scale: f64) -> u32 {
    if px == 0 {
        return 0;
    }
    ((f64::from(px) / scale.max(0.001)).round().max(1.0)) as u32
}

pub fn render_size_for_window(
    window: &Window,
    backend_type: BackendType,
    high_dpi: bool,
) -> PhysicalSize<u32> {
    render_size_for_physical(window, backend_type, high_dpi, window.inner_size())
}

pub fn render_size_for_physical(
    window: &Window,
    backend_type: BackendType,
    high_dpi: bool,
    size: PhysicalSize<u32>,
) -> PhysicalSize<u32> {
    if !macos_opengl_low_dpi(backend_type, high_dpi) {
        return size;
    }
    let scale = window.scale_factor();
    PhysicalSize::new(
        logical_px_for_physical(size.width, scale),
        logical_px_for_physical(size.height, scale),
    )
}

#[cfg(target_os = "macos")]
pub fn with_requested_window_size(
    attrs: WindowAttributes,
    backend_type: BackendType,
    high_dpi: bool,
    width: u32,
    height: u32,
) -> WindowAttributes {
    if macos_opengl_low_dpi(backend_type, high_dpi) {
        attrs.with_inner_size(LogicalSize::new(f64::from(width), f64::from(height)))
    } else {
        attrs.with_inner_size(PhysicalSize::new(width, height))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn with_requested_window_size(
    attrs: WindowAttributes,
    _backend_type: BackendType,
    _high_dpi: bool,
    width: u32,
    height: u32,
) -> WindowAttributes {
    attrs.with_inner_size(PhysicalSize::new(width, height))
}

pub fn request_window_size(
    window: &Window,
    backend_type: BackendType,
    high_dpi: bool,
    width: u32,
    height: u32,
) -> Option<PhysicalSize<u32>> {
    #[cfg(not(target_os = "macos"))]
    let _ = (backend_type, high_dpi);
    #[cfg(target_os = "macos")]
    {
        if macos_opengl_low_dpi(backend_type, high_dpi) {
            let size = LogicalSize::new(f64::from(width), f64::from(height));
            return window.request_inner_size(size);
        }
    }
    window.request_inner_size(PhysicalSize::new(width, height))
}

#[cfg(test)]
mod tests {
    use super::logical_px_for_physical;

    #[test]
    fn logical_px_preserves_zero() {
        assert_eq!(logical_px_for_physical(0, 2.0), 0);
    }

    #[test]
    fn logical_px_rounds_and_clamps_nonzero() {
        assert_eq!(logical_px_for_physical(3, 2.0), 2);
        assert_eq!(logical_px_for_physical(1, 2_000.0), 1);
    }

    #[test]
    fn logical_px_guards_bad_scale() {
        assert_eq!(logical_px_for_physical(1, 0.0), 1_000);
    }
}
