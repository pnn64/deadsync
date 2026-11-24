use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use deadlib_assets::{AssetManager, parse_font_with_asset_context};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{Metrics, with_current_metrics};
use deadsync_theme::player_options::{
    PlayerOptionsInline, PlayerOptionsLayout, PlayerOptionsPage, PlayerOptionsRow,
};
use deadsync_theme::system_options::SystemOptionsPresentation;
use deadsync_theme::views::{
    SystemOptionsPage, SystemOptionsPageKind, SystemOptionsPhase, SystemOptionsRow,
    SystemOptionsTiming,
};
use deadsync_theme::{Screen, TransitionState};
use deadsync_theme_stepmania_395::{PackageOptions, PackagePlayerOptions};

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
    write_fonts(root);
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
    let assets = register_fonts(root, prepared.fonts);
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

#[test]
fn player_options_use_authored_slots_column_positions_and_transition_clock() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path();
    fs::write(root.join("metrics.ini"), concat!(
        "[ScreenPlayerOptions]\nFallback=ScreenOptions\n",
        "ItemsLongRowP1X=270\nItemsLongRowP2X=480\nIconsP1X=80\nIconsP2X=580\n",
        "FrameOnCommand=x,-80;linear,0.4;x,0\nFrameOffCommand=linear,0.5;x,100\n",
        "[ScreenOptions]\nNumRowsShown=4\nRow1Y=80\nRow2Y=130\nRow3Y=220\nRow4Y=280\n",
        "LabelsX=200\nLabelsOnCommand=horizalign,right\nItemsStartX=240\nItemsEndX=540\nItemsGapX=12\n",
        "ColorSelected=1,1,1,1\nColorNotSelected=0.5,0.5,0.5,1\nColorDisabled=0.2,0.2,0.2,1\n",
        "TweenSeconds=0.21\nSeparateExitRow=1\nSeparateExitRowY=360\n",
        "HeaderX=320\nHeaderY=20\n",
    )).unwrap();
    write_fonts(root);
    let graphics = root.join("Graphics");
    fs::create_dir(&graphics).unwrap();
    raster(&graphics.join("ScreenOptions cursor 3x2.png"), [18, 12]);
    let prepared = PackagePlayerOptions::open(root, None).unwrap();
    assert_eq!(prepared.textures.len(), 1);
    let assets = register_fonts(root, prepared.fonts);
    let mut layout = prepared.presentation;
    let viewport = layout.viewport();
    assert_eq!(viewport.slots, 4);
    assert!(viewport.separate_exit);
    assert_eq!(viewport.row_tween_seconds, 0.21);
    assert_eq!(layout.entry_transition().1, 0.4);
    assert_eq!(layout.exit_transition(Screen::Gameplay).1, 0.5);

    let names = [
        TextContent::Static("Perspective"),
        TextContent::Static("Skin"),
        TextContent::Static("Exit"),
    ];
    let choices = [TextContent::Static("Flat"), TextContent::Static("Angle")];
    let first = TextContent::Static("Lime");
    let second = TextContent::Static("Slate");
    let row = |index: usize| {
        Some(PlayerOptionsRow {
            label: &names[index],
            detail: None,
            slot: [0.5, 2.0, 4.0][index],
            slots: 1,
            alpha: 1.0,
            exit: index == 2,
            values: if index == 1 {
                [Some(&first), Some(&second)]
            } else {
                [None; 2]
            },
            next_row: [false; 2],
            inline: (index == 0).then_some(PlayerOptionsInline {
                choices: &choices,
                focused: [Some(1), None],
                selected: [0, 1],
                enabled: [None; 2],
            }),
            has_preview: index == 1,
        })
    };
    let preview = Cell::new(None);
    let record_preview = |_: &mut Vec<Actor>, index, x, y| preview.set(Some((index, x, y)));
    let page = || PlayerOptionsPage {
        title: "PLAYER OPTIONS",
        active: [true; 2],
        selected_rows: [0, 1],
        scroll_position: [0.0, 0.5],
        row_count: 3,
        visible_slots: 4,
        alpha: 1.0,
        row: &row,
        help: &|_, _| None,
        draw_preview: &record_preview,
    };
    let mut actors = Vec::new();
    with_current_metrics(Metrics::centered(854.0, 480.0), || {
        layout.push_page(
            &mut actors,
            page(),
            &assets,
            1.0,
            &TransitionState::ActorsFadeIn {
                elapsed: 0.2,
                duration: 0.4,
            },
        );
    });
    let [
        Actor::Frame {
            offset, children, ..
        },
    ] = actors.as_slice()
    else {
        panic!("expected a single options frame");
    };
    assert_eq!(*offset, [387.0, 240.0]);
    assert!(
        matches!(label(children, "Perspective"), Actor::Text { offset, .. } if *offset == [200.0, 105.0])
    );
    assert!(
        matches!(label(children, "Lime"), Actor::Text { offset, .. } if *offset == [270.0, 220.0])
    );
    assert!(
        matches!(label(children, "Slate"), Actor::Text { offset, .. } if *offset == [480.0, 220.0])
    );
    assert!(matches!(label(children, "Exit"), Actor::Text { offset, .. } if offset[1] == 360.0));
    assert_eq!(preview.get(), Some((1, [147.0, 647.0], 220.0)));
    assert_eq!(
        children
            .iter()
            .filter(|actor| matches!(
                actor,
                Actor::Sprite {
                    uv_rect: Some([_, 0.5, _, 1.0]),
                    ..
                }
            ))
            .count(),
        3
    );

    actors.clear();
    with_current_metrics(Metrics::centered(854.0, 480.0), || {
        layout.push_page(
            &mut actors,
            page(),
            &assets,
            1.3,
            &TransitionState::ActorsFadeOut {
                elapsed: 0.25,
                duration: 0.5,
                target: Screen::Gameplay,
            },
        );
    });
    assert!(
        matches!(actors.as_slice(), [Actor::Frame { offset, .. }] if *offset == [477.0, 240.0])
    );
}

fn write_fonts(root: &Path) {
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
}

fn register_fonts(root: &Path, fonts: Vec<(&'static str, PathBuf)>) -> AssetManager {
    let mut assets = AssetManager::new();
    for (name, path) in fonts {
        assets.register_font(
            name,
            parse_font_with_asset_context(&path, [root.to_owned()])
                .unwrap()
                .font,
        );
    }
    assets
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
