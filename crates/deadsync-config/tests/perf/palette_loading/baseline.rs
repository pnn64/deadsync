// Frozen from 8e6b38040 (0.5.1706).
use super::*;

use crate::ini::SimpleIni;

pub(super) fn from_ini(content: &str, built_in: JudgmentPalettePreset) -> JudgmentPaletteCatalog {
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

    let mut catalog = JudgmentPaletteCatalog::new(built_in);
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
