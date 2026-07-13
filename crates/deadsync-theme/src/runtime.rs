use std::path::PathBuf;

/// Audio-backend-neutral music segment selected by a concrete theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioCut {
    pub start_sec: f64,
    pub length_sec: f64,
    pub fade_in_sec: f64,
    pub fade_out_sec: f64,
}

impl Default for AudioCut {
    fn default() -> Self {
        Self {
            start_sec: 0.0,
            length_sec: f64::INFINITY,
            fade_in_sec: 0.0,
            fade_out_sec: 0.0,
        }
    }
}

/// Audio work requested by a concrete theme and executed by the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioRequest {
    PlaySfx(String),
    PlayMusic {
        path: PathBuf,
        cut: AudioCut,
        looping: bool,
        rate: f32,
    },
    StopMusic,
    SetMusicRate(f32),
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

impl RendererChoice {
    #[cfg(all(
        target_os = "windows",
        not(target_vendor = "win7"),
        not(target_pointer_width = "32")
    ))]
    pub const ALL: &'static [Self] = &[
        Self::OpenGl,
        Self::Vulkan,
        Self::DirectX,
        Self::OpenGlWgpu,
        Self::VulkanWgpu,
        Self::Software,
    ];
    #[cfg(all(
        target_os = "windows",
        any(target_vendor = "win7", target_pointer_width = "32")
    ))]
    pub const ALL: &'static [Self] = &[
        Self::OpenGl,
        Self::DirectX,
        Self::OpenGlWgpu,
        Self::Software,
    ];
    #[cfg(all(target_os = "macos", not(target_pointer_width = "32")))]
    pub const ALL: &'static [Self] = &[
        Self::OpenGl,
        Self::Vulkan,
        Self::Metal,
        Self::OpenGlWgpu,
        Self::VulkanWgpu,
        Self::Software,
    ];
    #[cfg(all(
        not(any(target_os = "windows", target_os = "macos")),
        not(target_pointer_width = "32")
    ))]
    pub const ALL: &'static [Self] = &[
        Self::OpenGl,
        Self::Vulkan,
        Self::OpenGlWgpu,
        Self::VulkanWgpu,
        Self::Software,
    ];
    #[cfg(all(not(target_os = "windows"), target_pointer_width = "32"))]
    pub const ALL: &'static [Self] = &[Self::OpenGl, Self::OpenGlWgpu, Self::Software];

    #[inline(always)]
    pub fn choice_index(self) -> usize {
        Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or(0)
    }

    #[inline(always)]
    pub fn from_choice(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or(Self::OpenGl)
    }
}

impl Default for RendererChoice {
    fn default() -> Self {
        Self::OpenGl
    }
}

/// Fullscreen policy selected by a concrete theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FullscreenChoice {
    Exclusive,
    Borderless,
}

impl FullscreenChoice {
    #[inline(always)]
    pub const fn choice_index(self) -> usize {
        match self {
            Self::Exclusive => 0,
            Self::Borderless => 1,
        }
    }

    #[inline(always)]
    pub const fn from_choice(index: usize) -> Self {
        if index == 1 {
            Self::Borderless
        } else {
            Self::Exclusive
        }
    }
}

impl Default for FullscreenChoice {
    fn default() -> Self {
        Self::Exclusive
    }
}

/// Window mode selected by a concrete theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayModeChoice {
    Windowed,
    Fullscreen(FullscreenChoice),
}

impl Default for DisplayModeChoice {
    fn default() -> Self {
        Self::Windowed
    }
}

/// Presentation policy selected by a concrete theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentPolicyChoice {
    Mailbox,
    Immediate,
}

impl PresentPolicyChoice {
    #[inline(always)]
    pub const fn choice_index(self) -> usize {
        match self {
            Self::Mailbox => 0,
            Self::Immediate => 1,
        }
    }

    #[inline(always)]
    pub const fn from_choice(index: usize) -> Self {
        if index == 1 {
            Self::Immediate
        } else {
            Self::Mailbox
        }
    }
}

impl Default for PresentPolicyChoice {
    fn default() -> Self {
        Self::Mailbox
    }
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
    fn music_request_is_backend_neutral() {
        let path = PathBuf::from("Pack/Song/music.ogg");
        let cut = AudioCut {
            start_sec: 12.0,
            length_sec: 15.0,
            fade_in_sec: 0.5,
            fade_out_sec: 1.0,
        };
        assert_eq!(
            AudioRequest::PlayMusic {
                path: path.clone(),
                cut,
                looping: true,
                rate: 1.25,
            },
            AudioRequest::PlayMusic {
                path,
                cut,
                looping: true,
                rate: 1.25,
            }
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
