use std::io;
use std::path::{Path, PathBuf};

use crate::metrics::MetricCatalog;
use crate::{MetricLookupError, ResourceCatalog, ResourceCategory, ResourceLookupError};

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
}
