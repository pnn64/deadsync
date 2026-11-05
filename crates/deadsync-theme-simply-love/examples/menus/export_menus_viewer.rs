use askama::Template;
use std::path::Path;

use super::{MENUS, Result, Size};

#[derive(Template)]
#[template(path = "../examples/menus/export_menus_viewer.html")]
struct GalleryTemplate {
    menus: Vec<GalleryMenu>,
}

struct GalleryMenu {
    label: &'static str,
    images: Vec<GalleryImage>,
}

struct GalleryImage {
    resolution: String,
    src: String,
    disabled: bool,
}

pub(super) fn write_index(output: &Path, sizes: &[Size]) -> Result<()> {
    let menus = MENUS
        .iter()
        .map(|&(menu, label)| GalleryMenu {
            label,
            images: sizes
                .iter()
                .map(|size| {
                    let disabled = menu == "content-browser" && size.aspect != (16, 9);
                    GalleryImage {
                        resolution: size.name(),
                        src: format!("{}/{menu}.png", size.name()),
                        disabled,
                    }
                })
                .collect(),
        })
        .collect();
    let html = GalleryTemplate { menus }.render()?;
    std::fs::write(output.join("index.html"), html)?;
    std::fs::write(output.join("menus.css"), include_str!("menus.css"))?;
    Ok(())
}
