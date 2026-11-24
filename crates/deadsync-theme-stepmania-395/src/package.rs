use std::io;
use std::path::{Path, PathBuf};

use crate::metrics::MetricCatalog;
use crate::{
    ActorAnimation, AnimationError, BackgroundDefinition, MetricLookupError, ResourceCatalog,
    ResourceCategory, ResourceLookupError,
};

/// Presentation data and resource paths for the same ordered theme folders.
pub struct ThemePackage {
    metrics: MetricCatalog,
    resources: ResourceCatalog,
}

impl ThemePackage {
    /// Opens the active package followed by any fallback packages.
    pub fn open(
        roots: impl IntoIterator<Item = impl AsRef<Path>>,
        language: Option<&str>,
    ) -> io::Result<Self> {
        let roots = roots
            .into_iter()
            .map(|root| root.as_ref().canonicalize())
            .collect::<io::Result<Vec<PathBuf>>>()?;
        Ok(Self {
            resources: ResourceCatalog::open(&roots)?,
            metrics: MetricCatalog::open(&roots, language)?,
        })
    }

    pub fn metric(&self, section: &str, key: &str) -> Result<Option<&str>, MetricLookupError> {
        self.metrics.lookup(section, key)
    }

    /// Compiles an inherited command metric for the theme's logical canvas.
    pub fn animation_metric(
        &self,
        section: &str,
        key: &str,
        canvas: [f32; 2],
    ) -> Result<Option<ActorAnimation>, AnimationError> {
        self.metric(section, key)
            .map_err(AnimationError::Metric)?
            .map(|commands| ActorAnimation::parse(commands, canvas))
            .transpose()
    }

    /// Looks up a resource through the screen section's fallback chain.
    pub fn screen_resource(
        &self,
        category: ResourceCategory,
        section: &str,
        name: &str,
    ) -> Result<Option<&Path>, ResourceLookupError> {
        self.resources
            .resolve_scoped(category, section, name, &self.metrics)
    }

    /// Loads a numbered sprite-layer background resolved through the screen's fallback chain.
    pub fn background(
        &self,
        section: &str,
        name: &str,
        canvas: [f32; 2],
    ) -> io::Result<Option<BackgroundDefinition>> {
        self.screen_resource(ResourceCategory::Backgrounds, section, name)
            .map_err(io::Error::other)?
            .map(|path| BackgroundDefinition::read(path, canvas))
            .transpose()
    }
}
