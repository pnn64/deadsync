use crate::ini::SimpleIni;
use deadlib_platform::dirs::app_dirs;
use deadlib_present::color::Color;
use deadsync_theme::color::{JudgmentColorRole, JudgmentPalette, JudgmentPalettePreset};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentPaletteDefinition {
    pub id: String,
    pub name: String,
    pub palette: JudgmentPalette,
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentPaletteCatalog {
    built_in: JudgmentPalettePreset,
    pub default_palette_id: String,
    pub palettes: Vec<JudgmentPaletteDefinition>,
}

impl JudgmentPaletteCatalog {
    /// Start a catalog with the theme's immutable built-in palette.
    #[must_use]
    pub fn new(built_in: JudgmentPalettePreset) -> Self {
        Self {
            built_in,
            default_palette_id: built_in.id.to_owned(),
            palettes: vec![JudgmentPaletteDefinition {
                id: built_in.id.to_owned(),
                name: built_in.name.to_owned(),
                palette: built_in.palette,
                built_in: true,
            }],
        }
    }

    pub fn from_ini(content: &str, built_in: JudgmentPalettePreset) -> Self {
        let mut ini = SimpleIni::new();
        ini.load_str(content);

        let mut custom = Vec::new();
        for (section, properties) in ini.sections() {
            let Some(id) = section.strip_prefix("Palette ") else {
                continue;
            };
            let id = id.trim();
            if id.is_empty() || id.eq_ignore_ascii_case(built_in.id) {
                continue;
            }
            let name = properties
                .get("Name")
                .map(String::as_str)
                .and_then(valid_name)
                .unwrap_or("Custom Palette")
                .to_owned();
            let mut colors = built_in.palette.colors;
            for role in JudgmentColorRole::ALL {
                if let Some(color) = properties
                    .get(role.config_key())
                    .and_then(|raw| Color::from_hex(raw))
                {
                    colors[role.index()] = color.to_rgba();
                }
            }
            let order = properties
                .get("Order")
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
            custom.push((
                order,
                JudgmentPaletteDefinition {
                    id: id.to_owned(),
                    name,
                    palette: JudgmentPalette::from_base_colors(colors, built_in.dim_peaks),
                    built_in: false,
                },
            ));
        }
        custom.sort_by(|(left_order, left), (right_order, right)| {
            left_order
                .cmp(right_order)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut catalog = Self::new(built_in);
        for (_, definition) in custom {
            if catalog.palette(&definition.id).is_none() {
                catalog.palettes.push(definition);
            }
        }
        if let Some(default_id) = ini.get("General", "DefaultPalette")
            && catalog.palette(default_id).is_some()
        {
            default_id.clone_into(&mut catalog.default_palette_id);
        }
        catalog
    }

    #[must_use]
    pub fn to_ini(&self) -> String {
        let mut output = String::from("[General]\n");
        writeln!(output, "DefaultPalette={}", self.resolved_default_id())
            .expect("writing into String cannot fail");

        for (order, definition) in self
            .palettes
            .iter()
            .filter(|definition| !definition.built_in)
            .enumerate()
        {
            writeln!(output, "\n[Palette {}]", definition.id)
                .expect("writing into String cannot fail");
            writeln!(output, "Name={}", definition.name).expect("writing into String cannot fail");
            writeln!(output, "Order={order}").expect("writing into String cannot fail");
            for role in JudgmentColorRole::ALL {
                writeln!(
                    output,
                    "{}={}",
                    role.config_key(),
                    Color::from_rgba(definition.palette.color(role)).to_hex()
                )
                .expect("writing into String cannot fail");
            }
        }
        output
    }

    #[must_use]
    pub fn load(path: &Path, built_in: JudgmentPalettePreset) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => Self::from_ini(&content, built_in),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::new(built_in),
            Err(error) => {
                log::warn!(
                    "failed to load judgment palettes from '{}': {error}",
                    path.display()
                );
                Self::new(built_in)
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create judgment palette directory '{}': {error}",
                    parent.display()
                )
            })?;
        }
        deadlib_platform::atomic_write::write_atomic(path, self.to_ini().as_bytes()).map_err(
            |error| {
                format!(
                    "failed to save judgment palettes to '{}': {error}",
                    path.display()
                )
            },
        )
    }

    #[must_use]
    pub fn palette(&self, id: &str) -> Option<&JudgmentPaletteDefinition> {
        self.palettes.iter().find(|palette| palette.id == id)
    }

    #[must_use]
    pub fn resolved_default_id(&self) -> &str {
        if self.palette(&self.default_palette_id).is_some() {
            &self.default_palette_id
        } else {
            self.built_in.id
        }
    }

    #[must_use]
    pub fn resolve(&self, selection: Option<&str>) -> JudgmentPalette {
        selection
            .and_then(|id| self.palette(id))
            .or_else(|| self.palette(self.resolved_default_id()))
            .map_or(self.built_in.palette, |entry| entry.palette)
    }

    pub fn create_palette(&mut self, name: &str, source_id: &str) -> Result<String, String> {
        let name = self.checked_unique_name(name, None)?;
        let palette = self
            .palette(source_id)
            .map_or_else(|| self.resolve(None), |definition| definition.palette);
        let id = Uuid::new_v4().to_string();
        self.palettes.push(JudgmentPaletteDefinition {
            id: id.clone(),
            name,
            palette,
            built_in: false,
        });
        Ok(id)
    }

    pub fn rename_palette(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = self.checked_unique_name(name, Some(id))?;
        let Some(definition) = self.palettes.iter_mut().find(|entry| entry.id == id) else {
            return Err("The judgment palette no longer exists.".to_owned());
        };
        if definition.built_in {
            return Err("Built-in judgment palettes cannot be renamed.".to_owned());
        }
        definition.name = name;
        Ok(())
    }

    pub fn set_color(
        &mut self,
        id: &str,
        role: JudgmentColorRole,
        color: Color,
    ) -> Result<(), String> {
        let Some(definition) = self.palettes.iter_mut().find(|entry| entry.id == id) else {
            return Err("The judgment palette no longer exists.".to_owned());
        };
        if definition.built_in {
            return Err("Built-in judgment palettes cannot be edited.".to_owned());
        }
        definition.palette =
            definition
                .palette
                .with_color(role, color.to_rgba(), self.built_in.dim_peaks);
        Ok(())
    }

    pub fn delete_palette(&mut self, id: &str) -> Result<(), String> {
        let Some(index) = self.palettes.iter().position(|entry| entry.id == id) else {
            return Err("The judgment palette no longer exists.".to_owned());
        };
        if self.palettes[index].built_in {
            return Err("Built-in judgment palettes cannot be deleted.".to_owned());
        }
        self.palettes.remove(index);
        if self.default_palette_id == id {
            self.built_in.id.clone_into(&mut self.default_palette_id);
        }
        Ok(())
    }

    pub fn set_default_palette(&mut self, id: &str) -> Result<(), String> {
        if self.palette(id).is_none() {
            return Err("The judgment palette no longer exists.".to_owned());
        }
        id.clone_into(&mut self.default_palette_id);
        Ok(())
    }

    fn checked_unique_name(&self, raw: &str, existing_id: Option<&str>) -> Result<String, String> {
        let Some(name) = valid_name(raw) else {
            return Err("Palette names must contain between 1 and 32 characters.".to_owned());
        };
        if self.palettes.iter().any(|entry| {
            Some(entry.id.as_str()) != existing_id && entry.name.eq_ignore_ascii_case(name)
        }) {
            return Err("A judgment palette with that name already exists.".to_owned());
        }
        Ok(name.to_owned())
    }
}

fn valid_name(raw: &str) -> Option<&str> {
    let name = raw.trim();
    (!name.is_empty() && name.chars().count() <= 32 && !name.contains(['\n', '\r', '[', ']', '=']))
        .then_some(name)
}

// Initialized for the session's theme; catalogs are loaded/updated on menu transitions.
static RUNTIME_CATALOG: OnceLock<RwLock<Arc<JudgmentPaletteCatalog>>> = OnceLock::new();

/// Load the session catalog on first use with the supplied theme defaults.
pub fn runtime_catalog(built_in: JudgmentPalettePreset) -> Arc<JudgmentPaletteCatalog> {
    RUNTIME_CATALOG
        .get_or_init(|| {
            RwLock::new(Arc::new(JudgmentPaletteCatalog::load(
                &app_dirs().judgment_palettes_path(),
                built_in,
            )))
        })
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn update_runtime_catalog(
    built_in: JudgmentPalettePreset,
    update: impl FnOnce(&mut JudgmentPaletteCatalog) -> Result<(), String>,
) -> Result<Arc<JudgmentPaletteCatalog>, String> {
    let current = runtime_catalog(built_in);
    let mut next = (*current).clone();
    update(&mut next)?;
    next.save(&app_dirs().judgment_palettes_path())?;
    let next = Arc::new(next);
    *RUNTIME_CATALOG
        .get()
        .expect("runtime_catalog initialized the session catalog")
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::clone(&next);
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRESET: JudgmentPalettePreset = JudgmentPalettePreset {
        id: "test-theme",
        name: "Test Theme",
        palette: JudgmentPalette::new([[0.5, 0.25, 1.0, 1.0]; 7], [[0.1; 4]; 7], [[0.2; 4]; 7]),
        dim_peaks: [96, 64],
    };

    #[test]
    fn empty_catalog_resolves_to_supplied_theme_palette() {
        let catalog = JudgmentPaletteCatalog::new(PRESET);
        assert_eq!(catalog.resolve(None), PRESET.palette);
        assert_eq!(catalog.resolve(Some("missing")), PRESET.palette);
    }

    #[test]
    fn ini_uses_supplied_defaults_and_protects_the_builtin() {
        let catalog = JudgmentPaletteCatalog::from_ini(
            "[General]\nDefaultPalette=missing\n\n[Palette TEST-THEME]\nMiss=#FF0000\n\n[Palette custom]\nGreat=#80000000\nExcellent=invalid\n",
            PRESET,
        );
        assert_eq!(catalog.palettes.len(), 2);
        assert_eq!(catalog.resolve(None), PRESET.palette);
        let custom = catalog.resolve(Some("custom"));
        assert_eq!(
            custom.color(JudgmentColorRole::Excellent),
            PRESET.palette.color(JudgmentColorRole::Excellent)
        );
        assert_eq!(
            custom.gameplay_dim_color(JudgmentColorRole::Great),
            [0.0, 0.0, 0.0, 128.0 / 255.0]
        );
        assert_eq!(
            custom.gameplay_dim_color(JudgmentColorRole::Excellent)[2],
            96.0 / 255.0
        );
        assert_eq!(
            custom.evaluation_dim_color(JudgmentColorRole::Excellent)[2],
            64.0 / 255.0
        );
    }

    #[test]
    fn catalog_round_trip_preserves_custom_colors_and_order() {
        let mut catalog = JudgmentPaletteCatalog::new(PRESET);
        let first = catalog.create_palette("Warm", PRESET.id).unwrap();
        catalog
            .set_color(
                &first,
                JudgmentColorRole::Excellent,
                Color::from_hex("#123456").unwrap(),
            )
            .unwrap();
        let second = catalog.create_palette("Cool", PRESET.id).unwrap();
        catalog.set_default_palette(&second).unwrap();

        let loaded = JudgmentPaletteCatalog::from_ini(&catalog.to_ini(), PRESET);
        assert_eq!(loaded.default_palette_id, second);
        assert_eq!(loaded.palettes[1].name, "Warm");
        assert_eq!(loaded.palettes[2].name, "Cool");
        assert_eq!(
            Color::from_rgba(
                loaded
                    .palette(&first)
                    .unwrap()
                    .palette
                    .color(JudgmentColorRole::Excellent)
            )
            .to_hex(),
            "#123456"
        );
    }

    #[test]
    fn built_in_palette_cannot_be_changed_or_removed() {
        let mut catalog = JudgmentPaletteCatalog::new(PRESET);
        assert!(catalog.rename_palette(PRESET.id, "Other").is_err());
        assert!(catalog.delete_palette(PRESET.id).is_err());
        assert!(
            catalog
                .set_color(PRESET.id, JudgmentColorRole::Miss, Color::BLACK)
                .is_err()
        );
    }

    #[test]
    fn deleting_machine_default_falls_back_to_theme_palette() {
        let mut catalog = JudgmentPaletteCatalog::new(PRESET);
        let id = catalog.create_palette("Temporary", PRESET.id).unwrap();
        catalog.set_default_palette(&id).unwrap();
        catalog.delete_palette(&id).unwrap();
        assert_eq!(catalog.resolved_default_id(), PRESET.id);
    }
}
