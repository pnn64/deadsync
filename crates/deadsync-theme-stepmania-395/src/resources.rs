use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::metrics::MetricCatalog;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceCategory {
    Backgrounds,
    Fonts,
    Graphics,
    Numbers,
    Sounds,
    Other,
}

impl ResourceCategory {
    const ALL: [Self; 6] = [
        Self::Backgrounds,
        Self::Fonts,
        Self::Graphics,
        Self::Numbers,
        Self::Sounds,
        Self::Other,
    ];

    pub const fn directory(self) -> &'static str {
        match self {
            Self::Backgrounds => "BGAnimations",
            Self::Fonts => "Fonts",
            Self::Graphics => "Graphics",
            Self::Numbers => "Numbers",
            Self::Sounds => "Sounds",
            Self::Other => "Other",
        }
    }

    fn accepts_implicit(self, entry: &Resource) -> bool {
        let extension = Path::new(&entry.name)
            .extension()
            .and_then(|ext| ext.to_str());
        match (self, extension) {
            (_, Some("redir")) => !entry.directory,
            (Self::Backgrounds | Self::Graphics, None) => entry.directory,
            (Self::Backgrounds, Some("actor" | "xml")) => !entry.directory,
            (
                Self::Graphics,
                Some(
                    "actor" | "xml" | "sprite" | "png" | "jpg" | "bmp" | "gif" | "avi" | "mpg"
                    | "mpeg" | "txt",
                ),
            ) => !entry.directory,
            (Self::Fonts, Some("ini")) | (Self::Numbers, Some("png")) => !entry.directory,
            (Self::Sounds, Some("mp3" | "ogg" | "wav")) => !entry.directory,
            (Self::Other, _) => true,
            _ => false,
        }
    }
}

/// An index of local resource folders, ordered from overrides to fallbacks.
pub struct ResourceCatalog {
    packages: Vec<Package>,
}

struct Package {
    categories: [Vec<Resource>; ResourceCategory::ALL.len()],
}

struct Resource {
    path: PathBuf,
    name: String,
    directory: bool,
}

#[derive(Debug)]
pub enum ResourceLookupError {
    InvalidName(String),
    Ambiguous { first: PathBuf, second: PathBuf },
    ReadRedirect { path: PathBuf, error: io::Error },
    InvalidRedirect { path: PathBuf, target: String },
    MissingRedirectTarget { path: PathBuf, target: String },
    CircularRedirect(PathBuf),
    CircularSectionFallback(String),
}

impl fmt::Display for ResourceLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "invalid theme resource name {name:?}"),
            Self::Ambiguous { first, second } => {
                write!(
                    f,
                    "theme resource matches both {} and {}",
                    first.display(),
                    second.display()
                )
            }
            Self::ReadRedirect { path, error } => write!(f, "reading {}: {error}", path.display()),
            Self::InvalidRedirect { path, target } => {
                write!(
                    f,
                    "invalid redirect target {target:?} in {}",
                    path.display()
                )
            }
            Self::MissingRedirectTarget { path, target } => {
                write!(
                    f,
                    "redirect target {target:?} in {} is missing",
                    path.display()
                )
            }
            Self::CircularRedirect(path) => write!(f, "circular redirect at {}", path.display()),
            Self::CircularSectionFallback(section) => {
                write!(f, "circular resource fallback at [{section}]")
            }
        }
    }
}

impl std::error::Error for ResourceLookupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadRedirect { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl ResourceCatalog {
    /// Scans each package once. Earlier folders take precedence during lookup.
    pub fn open(roots: impl IntoIterator<Item = impl AsRef<Path>>) -> io::Result<Self> {
        let packages = roots
            .into_iter()
            .map(|root| Package::open(root.as_ref()))
            .collect::<io::Result<_>>()?;
        Ok(Self { packages })
    }

    /// Resolves a filename or logical resource name, following aliases across
    /// the same package stack.
    pub fn resolve(
        &self,
        category: ResourceCategory,
        name: &str,
    ) -> Result<Option<&Path>, ResourceLookupError> {
        let name = name.trim();
        if !valid_name(name) {
            return Err(ResourceLookupError::InvalidName(name.to_owned()));
        }
        let name_lower = name.to_ascii_lowercase();
        let explicit = Path::new(name).extension().is_some();
        let resource = self.find(category, &name_lower, explicit)?;
        Self::follow_redirects(resource, |target| {
            let explicit = Path::new(target).extension().is_some();
            self.find(category, &target.to_ascii_lowercase(), explicit)
        })
    }

    pub(crate) fn resolve_scoped(
        &self,
        category: ResourceCategory,
        section: &str,
        name: &str,
        metrics: &MetricCatalog,
    ) -> Result<Option<&Path>, ResourceLookupError> {
        let section = section.trim();
        let name = name.trim();
        if !section.is_empty() && !valid_name(section) {
            return Err(ResourceLookupError::InvalidName(section.to_owned()));
        }
        if !valid_name(name) {
            return Err(ResourceLookupError::InvalidName(name.to_owned()));
        }
        let resource = self.find_scoped(category, section, name, metrics)?;
        Self::follow_redirects(resource, |target| {
            let (section, name) = target.split_once(' ').unwrap_or(("", target));
            self.find_scoped(category, section, name, metrics)
        })
    }

    fn follow_redirects<'a>(
        resource: Option<&'a Resource>,
        lookup: impl Fn(&str) -> Result<Option<&'a Resource>, ResourceLookupError>,
    ) -> Result<Option<&'a Path>, ResourceLookupError> {
        let Some(mut resource) = resource else {
            return Ok(None);
        };
        let mut redirects = Vec::new();
        while resource.name.ends_with(".redir") && !resource.directory {
            let path = &resource.path;
            if redirects.contains(&path) {
                return Err(ResourceLookupError::CircularRedirect(path.clone()));
            }
            redirects.push(path);
            let text =
                fs::read_to_string(path).map_err(|error| ResourceLookupError::ReadRedirect {
                    path: path.clone(),
                    error,
                })?;
            let target = text
                .trim_start_matches('\u{feff}')
                .lines()
                .next()
                .unwrap_or_default()
                .trim();
            if !valid_name(target) {
                return Err(ResourceLookupError::InvalidRedirect {
                    path: path.clone(),
                    target: target.to_owned(),
                });
            }
            resource =
                lookup(target)?.ok_or_else(|| ResourceLookupError::MissingRedirectTarget {
                    path: path.clone(),
                    target: target.to_owned(),
                })?;
        }
        Ok(Some(&resource.path))
    }

    fn find(
        &self,
        category: ResourceCategory,
        name: &str,
        explicit: bool,
    ) -> Result<Option<&Resource>, ResourceLookupError> {
        for package in &self.packages {
            if let Some(resource) = package.find(category, name, explicit)? {
                return Ok(Some(resource));
            }
        }
        Ok(None)
    }

    fn find_scoped(
        &self,
        category: ResourceCategory,
        section: &str,
        name: &str,
        metrics: &MetricCatalog,
    ) -> Result<Option<&Resource>, ResourceLookupError> {
        let explicit = Path::new(name).extension().is_some();
        let mut resource_name = String::new();
        let mut visited = Vec::new();
        for package in &self.packages {
            let mut current = section;
            visited.clear();
            loop {
                resource_name.clear();
                if !current.is_empty() {
                    resource_name.push_str(current);
                    resource_name.push(' ');
                }
                resource_name.push_str(name);
                resource_name.make_ascii_lowercase();
                if let Some(resource) = package.find(category, &resource_name, explicit)? {
                    return Ok(Some(resource));
                }
                let Some(parent) = metrics.fallback(current) else {
                    break;
                };
                if !valid_name(parent) {
                    return Err(ResourceLookupError::InvalidName(parent.to_owned()));
                }
                visited.push(current);
                if visited.contains(&parent) {
                    return Err(ResourceLookupError::CircularSectionFallback(
                        parent.to_owned(),
                    ));
                }
                current = parent;
            }
        }
        Ok(None)
    }
}

impl Package {
    fn open(root: &Path) -> io::Result<Self> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("theme package {} is not a directory", root.display()),
            ));
        }
        let mut categories = std::array::from_fn(|_| Vec::new());
        for category in ResourceCategory::ALL {
            let directory = root.join(category.directory());
            match fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        format!(
                            "theme resource folder {} is not a directory",
                            directory.display()
                        ),
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            }
            let entries = &mut categories[category as usize];
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if !(kind.is_file() || kind.is_dir()) {
                    continue;
                }
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                entries.push(Resource {
                    path: entry.path(),
                    name: name.to_ascii_lowercase(),
                    directory: kind.is_dir(),
                });
            }
            entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        }
        Ok(Self { categories })
    }

    fn find(
        &self,
        category: ResourceCategory,
        name: &str,
        explicit: bool,
    ) -> Result<Option<&Resource>, ResourceLookupError> {
        let mut entries = self.categories[category as usize].iter().filter(|entry| {
            if explicit {
                entry.name == name
            } else {
                entry.name.starts_with(name) && category.accepts_implicit(entry)
            }
        });
        let first = entries.next();
        if let (Some(first), Some(second)) = (first, entries.next()) {
            return Err(ResourceLookupError::Ambiguous {
                first: first.path.clone(),
                second: second.path.clone(),
            });
        }
        if first.is_none() && category == ResourceCategory::Fonts {
            return self.find(ResourceCategory::Numbers, name, explicit);
        }
        Ok(first)
    }
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty() && !matches!(name, "." | "..") && !name.contains(['/', '\\', ':', '\0'])
}
