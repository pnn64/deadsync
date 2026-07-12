use std::path::PathBuf;

/// Audio work requested by a concrete theme and executed by the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioRequest {
    PlaySfx(String),
    /// Warm loudness metadata for theme-selected preview media without
    /// exposing the analysis service or its scheduling policy to the theme.
    PrewarmReplayGain(Vec<PathBuf>),
}

/// Platform work requested by a concrete theme and executed by the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformRequest {
    /// Reveal a file or directory in the host's file explorer.
    RevealPath { path: PathBuf, kind: RevealPathKind },
}

/// How the shell should prepare a path before revealing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevealPathKind {
    File,
    Directory,
}

/// Renderer selected by a concrete theme without exposing a renderer backend
/// type through the theme-to-shell boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererChoice {
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    Vulkan,
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    VulkanWgpu,
    #[cfg(target_os = "macos")]
    Metal,
    OpenGl,
    OpenGlWgpu,
    Software,
    #[cfg(target_os = "windows")]
    DirectX,
}

/// Fullscreen policy selected by a concrete theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FullscreenChoice {
    Exclusive,
    Borderless,
}

/// Window mode selected by a concrete theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayModeChoice {
    Windowed,
    Fullscreen(FullscreenChoice),
}

/// Presentation policy selected by a concrete theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentPolicyChoice {
    Mailbox,
    Immediate,
}

/// Renderer-independent graphics changes requested by a concrete theme.
///
/// The shell maps these semantic choices to its renderer, window, and persisted
/// configuration types before applying them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphicsRequest {
    pub renderer: Option<RendererChoice>,
    pub display_mode: Option<DisplayModeChoice>,
    pub monitor: Option<usize>,
    pub resolution: Option<(u32, u32)>,
    pub vsync: Option<bool>,
    pub present_mode_policy: Option<PresentPolicyChoice>,
    pub max_fps: Option<u16>,
    pub high_dpi: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphics_request_is_a_partial_update() {
        let request = GraphicsRequest {
            display_mode: Some(DisplayModeChoice::Fullscreen(FullscreenChoice::Borderless)),
            vsync: Some(true),
            ..GraphicsRequest::default()
        };

        assert_eq!(request.renderer, None);
        assert_eq!(request.vsync, Some(true));
        assert_eq!(
            request.display_mode,
            Some(DisplayModeChoice::Fullscreen(FullscreenChoice::Borderless))
        );
    }

    #[test]
    fn audio_request_owns_its_theme_asset_key() {
        let request = AudioRequest::PlaySfx("assets/sounds/start.ogg".to_owned());
        assert_eq!(
            request,
            AudioRequest::PlaySfx("assets/sounds/start.ogg".to_owned())
        );
    }

    #[test]
    fn replaygain_prewarm_request_owns_media_paths() {
        let paths = vec![PathBuf::from("Pack/Song A/music.ogg")];
        assert_eq!(
            AudioRequest::PrewarmReplayGain(paths.clone()),
            AudioRequest::PrewarmReplayGain(paths)
        );
    }

    #[test]
    fn platform_request_carries_path_and_preparation_kind() {
        let path = PathBuf::from("save/screenshots");
        assert_eq!(
            PlatformRequest::RevealPath {
                path: path.clone(),
                kind: RevealPathKind::Directory,
            },
            PlatformRequest::RevealPath {
                path,
                kind: RevealPathKind::Directory,
            }
        );
    }
}
