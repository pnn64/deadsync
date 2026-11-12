use askama::Template;
use std::{collections::BTreeSet, path::Path};

use super::{MENUS, PLAYER_VARIANT_MENUS, PLAYER_VARIANTS, Result, Size};

#[derive(Template)]
#[template(path = "../examples/menus/export_menus_viewer.html")]
struct GalleryTemplate {
    menus: Vec<GalleryMenu>,
    resolutions: Vec<String>,
}

struct GalleryMenu {
    value: &'static str,
    label: &'static str,
    images: Vec<GalleryImage>,
}

struct GalleryImage {
    resolution: String,
    caption: String,
    src: String,
    disabled: bool,
    variant: Option<&'static str>,
}

pub(super) fn write_index(output: &Path, sizes: &[Size]) -> Result<()> {
    let mut seen_resolutions = BTreeSet::new();
    let resolutions = sizes
        .iter()
        .map(Size::name)
        .filter(|resolution| seen_resolutions.insert(resolution.clone()))
        .collect();
    let menus = MENUS
        .iter()
        .map(|&(menu, label)| GalleryMenu {
            value: menu,
            label,
            images: sizes
                .iter()
                .flat_map(|size| {
                    let disabled = menu == "content-browser" && size.aspect != (16, 9);
                    let mut images = Vec::new();
                    if menu != "song-select" {
                        images.push(GalleryImage {
                            resolution: size.name(),
                            caption: size.name(),
                            src: format!("{}/{menu}.png", size.name()),
                            disabled,
                            variant: None,
                        });
                    }
                    if PLAYER_VARIANT_MENUS.contains(&menu) {
                        images.extend(PLAYER_VARIANTS.iter().map(|variant| GalleryImage {
                            resolution: size.name(),
                            caption: format!("{} · {}", size.name(), variant.label),
                            src: format!(
                                "{}/{menu}{suffix}.png",
                                size.name(),
                                suffix = variant.suffix
                            ),
                            disabled: false,
                            variant: None,
                        }));
                        if menu == "song-select" {
                            for (player, label) in [("1p", "1P"), ("2p", "2P")] {
                                images.push(GalleryImage {
                                    resolution: size.name(),
                                    caption: format!(
                                        "{} · {} · Expanded Steps",
                                        size.name(),
                                        label
                                    ),
                                    src: format!(
                                        "{}/song-select-{player}-expanded.png",
                                        size.name()
                                    ),
                                    disabled: false,
                                    variant: Some("expanded-steps"),
                                });
                            }
                        }
                    }
                    images
                })
                .collect(),
        })
        .collect();
    let html = GalleryTemplate { menus, resolutions }.render()?;
    std::fs::write(output.join("index.html"), html)?;
    std::fs::write(output.join("menus.css"), include_str!("menus.css"))?;
    Ok(())
}
