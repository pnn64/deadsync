use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use deadsync_config::ini::SimpleIni;

use crate::resources::valid_name;

/// Presentation metrics layered in package priority order.
pub(crate) struct MetricCatalog {
    layers: Vec<SimpleIni>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MetricLookupError {
    pub section: String,
    pub key: String,
}

impl fmt::Display for MetricLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "circular fallback while reading [{}] {}",
            self.section, self.key
        )
    }
}

impl std::error::Error for MetricLookupError {}

impl MetricCatalog {
    /// Loads metrics and language overrides from folders in precedence order.
    pub(crate) fn open(
        roots: impl IntoIterator<Item = impl AsRef<Path>>,
        language: Option<&str>,
    ) -> io::Result<Self> {
        let language = language.filter(|language| !language.eq_ignore_ascii_case("english"));
        if let Some(language) = language
            && !valid_name(language)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid theme language {language:?}"),
            ));
        }
        let layers = roots
            .into_iter()
            .map(|root| read_layer(root.as_ref(), language))
            .collect::<io::Result<_>>()?;
        Ok(Self { layers })
    }

    /// Returns the raw metric, including any expressions and actor commands.
    /// A layer's section fallback is searched before moving to the next layer.
    pub(crate) fn lookup(
        &self,
        section: &str,
        key: &str,
    ) -> Result<Option<&str>, MetricLookupError> {
        let mut sections = vec![(section, 0)];
        while let Some((current, index)) = sections.last_mut() {
            let current = *current;
            let Some(layer) = self.layers.get(*index) else {
                sections.pop();
                continue;
            };
            *index += 1;
            if let Some(value) = layer.get(current, key) {
                return Ok(Some(value));
            }
            if let Some(parent) = layer
                .get(current, "Fallback")
                .filter(|name| !name.is_empty())
            {
                if sections.iter().any(|(active, _)| *active == parent) {
                    return Err(MetricLookupError {
                        section: parent.to_owned(),
                        key: key.to_owned(),
                    });
                }
                sections.push((parent, 0));
            }
        }
        Ok(None)
    }

    pub(crate) fn fallback(&self, section: &str) -> Option<&str> {
        self.layers
            .iter()
            .find_map(|layer| layer.get(section, "Fallback"))
            .filter(|name| !name.is_empty())
    }
}

fn read_layer(root: &Path, language: Option<&str>) -> io::Result<SimpleIni> {
    let mut contents = String::new();
    append_ini(&mut contents, &root.join("metrics.ini"))?;
    let languages = root.join("Languages");
    append_ini(&mut contents, &languages.join("english.ini"))?;
    if let Some(language) = language {
        append_ini(&mut contents, &languages.join(format!("{language}.ini")))?;
    }
    let mut layer = SimpleIni::new();
    layer.load_str(&contents);
    Ok(layer)
}

fn append_ini(contents: &mut String, path: &Path) -> io::Result<()> {
    match fs::read_to_string(path) {
        Ok(text) => {
            // Reset the active section before parsing the next file.
            contents.push_str("[]\n");
            contents.push_str(text.trim_start_matches('\u{feff}'));
            contents.push('\n');
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!("reading {}: {error}", path.display()),
        )),
    }
}
