use askama::Template;
use std::{collections::BTreeSet, path::Path};

use super::{LAYOUTS, Result, Size};

const LAYOUT_OPTIONS: &[LayoutOption] = &[
    LayoutOption {
        value: "p1",
        label: "P1",
    },
    LayoutOption {
        value: "p2",
        label: "P2",
    },
    LayoutOption {
        value: "single",
        label: "Single",
    },
    LayoutOption {
        value: "versus",
        label: "Versus",
    },
    LayoutOption {
        value: "double",
        label: "Double",
    },
];

#[derive(Template)]
#[template(path = "../examples/gameplay/export_gameplay_viewer.html")]
struct GalleryTemplate {
    layouts: &'static [LayoutOption],
    resolutions: Vec<ResolutionOption>,
    sections: Vec<GallerySection>,
}

struct LayoutOption {
    value: &'static str,
    label: &'static str,
}

struct ResolutionOption {
    value: String,
}

struct GallerySection {
    layout: &'static str,
    resolution: String,
    centered: bool,
    images: [GalleryImage; 2],
}

struct GalleryImage {
    src: String,
    caption: String,
    variant: &'static str,
}

pub(super) fn write_index(output: &Path, sizes: &[Size], reverse: bool) -> Result<()> {
    let reverse_caption = if reverse { " (reverse)" } else { "" };
    let mut seen_resolutions = BTreeSet::new();
    let mut resolutions = Vec::new();
    let mut sections = Vec::new();

    for size in sizes {
        let resolution = size.name();
        if seen_resolutions.insert(resolution.clone()) {
            resolutions.push(ResolutionOption {
                value: resolution.clone(),
            });
        }

        for &layout in LAYOUTS {
            let mut variants = vec![(layout.to_owned(), false)];
            if matches!(layout, "p1" | "p2") {
                variants.push((format!("{layout}-centered"), true));
            }

            for (export_layout, centered) in variants {
                let images = [
                    ("gameplay", "Gameplay", "leaderboard"),
                    ("gameplay-hmr", "Gameplay HMR", "hmr"),
                ]
                .map(|(stem, mode, variant)| {
                    let src = format!(
                        "{resolution}/{export_layout}/{stem}{}.png",
                        if reverse { "-reverse" } else { "" }
                    );
                    let caption = format!(
                        "{resolution} - {}:{} - {export_layout} - {mode}{reverse_caption}",
                        size.aspect.0, size.aspect.1
                    );
                    GalleryImage {
                        src,
                        caption,
                        variant,
                    }
                });
                sections.push(GallerySection {
                    layout,
                    resolution: resolution.clone(),
                    centered,
                    images,
                });
            }
        }
    }

    let html = GalleryTemplate {
        layouts: LAYOUT_OPTIONS,
        resolutions,
        sections,
    }
    .render()?;
    std::fs::write(output.join("index.html"), html)?;
    Ok(())
}
