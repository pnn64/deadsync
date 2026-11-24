use std::{fs, path::Path, sync::Arc};

use deadlib_assets::{AssetManager, parse_font_with_asset_context};
use deadlib_present::actors::Actor;
use deadlib_present::space::{Metrics, with_current_metrics};
use deadsync_theme::system_options::SystemOptionsPresentation;
use deadsync_theme::views::{
    SystemOptionsPage, SystemOptionsPageKind, SystemOptionsPhase, SystemOptionsRow,
    SystemOptionsTiming,
};
use deadsync_theme_stepmania_395::PackageOptions;

#[test]
fn options_use_authored_layout_and_live_rows_through_the_mounted_renderer() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path();
    fs::write(root.join("metrics.ini"), concat!(
        "[ScreenOptionsMenu]\nFallback=ScreenOptions\nTweenSeconds=0.4\n",
        "[ScreenOptionsSubmenu]\nFallback=ScreenOptions\nTweenSeconds=0.5\nRow2Y=160\n",
        "FrameOnCommand=x,-80;linear,0.5;x,0\n",
        "[ScreenOptions]\nNumRowsShown=2\nRow1Y=80\nRow2Y=120\n",
        "LabelsX=SCREEN_CENTER_X-120\nLabelsOnCommand=horizalign,right;zoom,0.75;shadowlength,2\n",
        "ItemsStartX=240\nItemsEndX=520\nItemsGapX=12\nItemsLongRowSharedX=360\nItemsOnCommand=zoom,0.5\n",
        "ColorSelected=1,1,1,1\nColorNotSelected=0.5,0.6,0.7,1\nColorDisabled=0.2,0.3,0.4,1\n",
        "ShowExplanations=1\nExplanationTogetherX=40\nExplanationTogetherY=420\nExplanationZoom=0.5\n",
        "ExplanationTogetherOnCommand=horizalign,left;wrapwidthpixels,400\n",
        "PageX=320\nPageY=240\nPageOnCommand=x,300;linear,0.5;x,320\n",
        "SeparateExitRow=1\nSeparateExitRowY=360\nHeaderX=320\nHeaderY=20\n",
    )).unwrap();
    let fonts = root.join("Fonts");
    fs::create_dir(&fonts).unwrap();
    for role in ["title", "item", "explanation"] {
        fs::write(
            fonts.join(format!("ScreenOptions {role}.redir")),
            "caption\n",
        )
        .unwrap();
    }
    fs::write(
        fonts.join("caption.ini"),
        "[Char Widths]\nBaseline=6\nTop=0\nLineSpacing=8\n",
    )
    .unwrap();
    raster(&fonts.join("caption 16x8.png"), [128, 64]);
    let graphics = root.join("Graphics");
    fs::create_dir(&graphics).unwrap();
    raster(&graphics.join("ScreenOptions page.png"), [64, 32]);
    raster(
        &graphics.join("ScreenOptions line highlight.png"),
        [400, 18],
    );
    let back = root.join("BGAnimations/ScreenOptionsMenu background");
    fs::create_dir_all(&back).unwrap();
    fs::write(
        back.join("BGAnimation.ini"),
        "[Layer1]\nFile=tile.png\nCommand=x,0;linear,1;x,100\n",
    )
    .unwrap();
    raster(&back.join("tile.png"), [16, 16]);

    let mut prepared = PackageOptions::open(root, None).unwrap();
    assert_eq!(prepared.textures.len(), 3);
    assert_eq!(
        prepared.presentation.layout().timing,
        Some(SystemOptionsTiming {
            service_seconds: 0.4,
            settings_seconds: 0.5
        })
    );
    let mut assets = AssetManager::new();
    for (name, path) in prepared.fonts {
        assets.register_font(
            name,
            parse_font_with_asset_context(&path, [root.to_owned()])
                .unwrap()
                .font,
        );
    }
    let row = |index| {
        assert!(index < 3);
        Some(SystemOptionsRow {
            label: Arc::from(["Renderer", "Scan Mode", "Volume"][index]),
            choices: Some(Arc::from([Arc::from("Alpha"), Arc::from("Beta")])),
            selected_choice: 1,
            inline: index == 1,
            disabled: index == 0,
            enabled_choices: None,
            choice_tint: None,
        })
    };
    let draw = |presentation: &mut PackageOptions, phase, now| {
        let mut actors = Vec::new();
        with_current_metrics(Metrics::centered(854.0, 480.0), || {
            presentation.with_renderer(phase, now, &assets, &mut |renderer| {
                (renderer.background)(&mut actors);
                (renderer.page)(
                    &mut actors,
                    SystemOptionsPage {
                        id: 2,
                        title: "DISPLAY",
                        kind: SystemOptionsPageKind::Settings,
                        selected: 1,
                        row_count: 3,
                        row: &row,
                        description: Some(Arc::from("Pick a scan mode.")),
                        detail: None,
                        phase,
                    },
                );
            });
        });
        actors
    };
    let actors = draw(
        &mut prepared.presentation,
        SystemOptionsPhase::Entering(0.25),
        10.0,
    );
    let [
        Actor::Frame { .. },
        Actor::Frame {
            offset, children, ..
        },
    ] = actors.as_slice()
    else {
        panic!("expected the background and moving options frame");
    };
    assert_eq!(*offset, [387.0, 240.0]);
    assert!(
        matches!(children.first(), Some(Actor::Sprite { offset, z: -10, .. }) if *offset == [310.0, 240.0])
    );
    assert!(
        children.iter().any(
            |actor| matches!(actor, Actor::Sprite { z: -1, offset, .. } if offset[1] == 160.0)
        )
    );
    assert!(
        matches!(label(children, "Renderer"), Actor::Text { offset, color, scale, shadow_len, .. }
        if *offset == [200.0, 80.0] && *color == [0.2,0.3,0.4,1.0] && *scale == [0.75;2] && *shadow_len == [2.0,-2.0])
    );
    assert!(
        matches!(label(children, "Alpha"), Actor::Text { offset, .. } if *offset == [240.0,160.0])
    );
    assert!(
        matches!(label(children, "Beta"), Actor::Text { offset, align, .. } if *offset == [360.0,80.0] && align[0] == 0.5)
    );
    assert!(
        matches!(label(children, "Scan Mode"), Actor::Text { offset, color, .. } if offset[1] == 160.0 && *color == [1.0;4])
    );
    assert!(matches!(label(children, "Done"), Actor::Text { offset, .. } if offset[1] == 360.0));
    assert!(matches!(
        label(children, "Pick a scan mode."),
        Actor::Text {
            wrap_width_pixels: Some(400),
            ..
        }
    ));

    let leaving = draw(
        &mut prepared.presentation,
        SystemOptionsPhase::Leaving(0.0),
        10.25,
    );
    let Actor::Frame { children, .. } = &leaving[1] else {
        panic!("expected the departing options frame");
    };
    assert!(
        matches!(children.first(), Some(Actor::Sprite { offset, z: -10, .. }) if *offset == [320.0, 240.0])
    );

    assert_eq!(
        background_x(&mut prepared.presentation, &assets, 10.5),
        50.0
    );
    prepared.presentation.reset();
    assert_eq!(background_x(&mut prepared.presentation, &assets, 20.0), 0.0);
}

fn raster(path: &Path, [width, height]: [u32; 2]) {
    image::RgbaImage::from_pixel(width, height, image::Rgba([32, 48, 72, 255]))
        .save(path)
        .unwrap();
}

fn label<'a>(children: &'a [Actor], expected: &str) -> &'a Actor {
    children
        .iter()
        .find(|actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == expected))
        .unwrap()
}

fn background_x(
    options: &mut impl SystemOptionsPresentation<Actor>,
    assets: &AssetManager,
    now: f64,
) -> f32 {
    let mut actors = Vec::new();
    options.with_renderer(SystemOptionsPhase::Steady, now, assets, &mut |renderer| {
        (renderer.background)(&mut actors)
    });
    let [Actor::Frame { children, .. }] = actors.as_slice() else {
        panic!("expected the background frame");
    };
    let [Actor::Sprite { offset, .. }] = children.as_slice() else {
        panic!("expected the moving background");
    };
    offset[0]
}
