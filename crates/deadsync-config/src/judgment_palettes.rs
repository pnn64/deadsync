use crate::ini::SimpleIni;
use deadlib_platform::dirs::app_dirs;
use deadlib_present::color::{
    Color, JudgmentColorRole, JudgmentPalette, SIMPLY_LOVE_JUDGMENT_PALETTE,
};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};
use uuid::Uuid;

pub const SIMPLY_LOVE_PALETTE_ID: &str = "simply-love";
pub const SIMPLY_LOVE_PALETTE_NAME: &str = "Simply Love";

#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentPaletteDefinition {
    pub id: String,
    pub name: String,
    pub palette: JudgmentPalette,
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentPaletteCatalog {
    pub default_palette_id: String,
    pub palettes: Vec<JudgmentPaletteDefinition>,
}

impl Default for JudgmentPaletteCatalog {
    fn default() -> Self {
        Self {
            default_palette_id: SIMPLY_LOVE_PALETTE_ID.to_owned(),
            palettes: vec![built_in_definition()],
        }
    }
}

impl JudgmentPaletteCatalog {
    pub fn from_ini(content: &str) -> Self {
        let mut ini = SimpleIni::new();
        ini.load_str(content);

        let mut custom = Vec::new();
        for (section, properties) in ini.sections() {
            let Some(id) = section.strip_prefix("Palette ") else {
                continue;
            };
            let id = id.trim();
            if id.is_empty() || id.eq_ignore_ascii_case(SIMPLY_LOVE_PALETTE_ID) {
                continue;
            }
            let name = properties
                .get("Name")
                .map(String::as_str)
                .and_then(valid_name)
                .unwrap_or("Custom Palette")
                .to_owned();
            let mut colors = SIMPLY_LOVE_JUDGMENT_PALETTE.colors;
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
                    palette: JudgmentPalette::from_base_colors(colors),
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

        let mut catalog = Self::default();
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
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => Self::from_ini(&content),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => {
                log::warn!(
                    "failed to load judgment palettes from '{}': {error}",
                    path.display()
                );
                Self::default()
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
        std::fs::write(path, self.to_ini()).map_err(|error| {
            format!(
                "failed to save judgment palettes to '{}': {error}",
                path.display()
            )
        })
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
            SIMPLY_LOVE_PALETTE_ID
        }
    }

    #[must_use]
    pub fn resolve(&self, selection: Option<&str>) -> JudgmentPalette {
        selection
            .and_then(|id| self.palette(id))
            .or_else(|| self.palette(self.resolved_default_id()))
            .map_or(SIMPLY_LOVE_JUDGMENT_PALETTE, |entry| entry.palette)
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
        definition.palette = definition.palette.with_color(role, color.to_rgba());
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
            SIMPLY_LOVE_PALETTE_ID.clone_into(&mut self.default_palette_id);
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

fn built_in_definition() -> JudgmentPaletteDefinition {
    JudgmentPaletteDefinition {
        id: SIMPLY_LOVE_PALETTE_ID.to_owned(),
        name: SIMPLY_LOVE_PALETTE_NAME.to_owned(),
        palette: SIMPLY_LOVE_JUDGMENT_PALETTE,
        built_in: true,
    }
}

fn valid_name(raw: &str) -> Option<&str> {
    let name = raw.trim();
    (!name.is_empty() && name.chars().count() <= 32 && !name.contains(['\n', '\r', '[', ']', '=']))
        .then_some(name)
}

static RUNTIME_CATALOG: LazyLock<RwLock<Arc<JudgmentPaletteCatalog>>> = LazyLock::new(|| {
    RwLock::new(Arc::new(JudgmentPaletteCatalog::load(
        &app_dirs().judgment_palettes_path(),
    )))
});

pub fn runtime_catalog() -> Arc<JudgmentPaletteCatalog> {
    RUNTIME_CATALOG
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn update_runtime_catalog(
    update: impl FnOnce(&mut JudgmentPaletteCatalog) -> Result<(), String>,
) -> Result<Arc<JudgmentPaletteCatalog>, String> {
    let current = runtime_catalog();
    let mut next = (*current).clone();
    update(&mut next)?;
    next.save(&app_dirs().judgment_palettes_path())?;
    let next = Arc::new(next);
    *RUNTIME_CATALOG
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::clone(&next);
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_catalog_resolves_to_exact_historical_palette() {
        let catalog = JudgmentPaletteCatalog::default();
        assert_eq!(catalog.resolve(None), SIMPLY_LOVE_JUDGMENT_PALETTE);
        assert_eq!(
            catalog.resolve(Some("missing")),
            SIMPLY_LOVE_JUDGMENT_PALETTE
        );
    }

    #[test]
    fn catalog_round_trip_preserves_custom_colors_and_order() {
        let mut catalog = JudgmentPaletteCatalog::default();
        let first = catalog
            .create_palette("Warm", SIMPLY_LOVE_PALETTE_ID)
            .unwrap();
        catalog
            .set_color(
                &first,
                JudgmentColorRole::Excellent,
                Color::from_hex("#123456").unwrap(),
            )
            .unwrap();
        let second = catalog
            .create_palette("Cool", SIMPLY_LOVE_PALETTE_ID)
            .unwrap();
        catalog.set_default_palette(&second).unwrap();

        let loaded = JudgmentPaletteCatalog::from_ini(&catalog.to_ini());
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
        let mut catalog = JudgmentPaletteCatalog::default();
        assert!(
            catalog
                .rename_palette(SIMPLY_LOVE_PALETTE_ID, "Other")
                .is_err()
        );
        assert!(catalog.delete_palette(SIMPLY_LOVE_PALETTE_ID).is_err());
        assert!(
            catalog
                .set_color(
                    SIMPLY_LOVE_PALETTE_ID,
                    JudgmentColorRole::Miss,
                    Color::BLACK
                )
                .is_err()
        );
    }

    #[test]
    fn deleting_machine_default_falls_back_to_simply_love() {
        let mut catalog = JudgmentPaletteCatalog::default();
        let id = catalog
            .create_palette("Temporary", SIMPLY_LOVE_PALETTE_ID)
            .unwrap();
        catalog.set_default_palette(&id).unwrap();
        catalog.delete_palette(&id).unwrap();
        assert_eq!(catalog.resolved_default_id(), SIMPLY_LOVE_PALETTE_ID);
    }
}
