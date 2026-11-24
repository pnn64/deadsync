use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
        let Some(mut resource) = self.find(category, name)? else {
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
            resource = self.find(category, target)?.ok_or_else(|| {
                ResourceLookupError::MissingRedirectTarget {
                    path: path.clone(),
                    target: target.to_owned(),
                }
            })?;
        }
        Ok(Some(&resource.path))
    }

    fn find(
        &self,
        category: ResourceCategory,
        name: &str,
    ) -> Result<Option<&Resource>, ResourceLookupError> {
        let name = name.to_ascii_lowercase();
        let explicit = Path::new(&name).extension().is_some();
        for package in &self.packages {
            if let Some(resource) = package.find(category, &name, explicit)? {
                return Ok(Some(resource));
            }
            if category == ResourceCategory::Fonts
                && let Some(resource) = package.find(ResourceCategory::Numbers, &name, explicit)?
            {
                return Ok(Some(resource));
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
        Ok(first)
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && !matches!(name, "." | "..") && !name.contains(['/', '\\', ':', '\0'])
}
