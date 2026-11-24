//! Resource, metric, and animation loading for local 3.95-format theme packages.

mod animation;
mod backgrounds;
mod intro;
mod metrics;
mod package;
mod resources;

pub use animation::{ActorAnimation, AnimationError};
pub use backgrounds::{BackgroundDefinition, BackgroundPlayback, BackgroundTexture};
pub use intro::PackageIntro;
pub use metrics::MetricLookupError;
pub use package::ThemePackage;
pub use resources::{ResourceCatalog, ResourceCategory, ResourceLookupError};
