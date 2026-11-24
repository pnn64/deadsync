//! Resource, metric, and animation loading for local 3.95-format theme packages.

mod animation;
mod backgrounds;
mod bitmap_text;
mod intro;
mod metrics;
mod options;
mod package;
mod resources;
mod scene;
mod select_music;
mod stage_card;
mod title;

pub use animation::{ActorAnimation, AnimationError};
pub use backgrounds::{BackgroundDefinition, BackgroundPlayback, BackgroundTexture};
pub use intro::PackageIntro;
pub use metrics::MetricLookupError;
pub use options::{PackageOptions, PackagePlayerOptions, PreparedOptions, PreparedPlayerOptions};
pub use package::ThemePackage;
pub use resources::{ResourceCatalog, ResourceCategory, ResourceLookupError};
pub use select_music::{PackageMusic, PreparedMusic};
pub use stage_card::PackageStageCard;
pub use title::{PackageTitle, PackageTitleState, TITLE_FONT};
