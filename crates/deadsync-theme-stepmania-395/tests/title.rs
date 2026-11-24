use std::{fs, time::Instant};

use deadlib_assets::{AssetManager, parse_font_with_asset_context};
use deadlib_present::actors::Actor;
use deadlib_present::anim::EffectMode;
use deadsync_config::coin::CoinMode;
use deadsync_core::input::InputSource;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_theme::main_menu::{MainMenuFrame, MainMenuPhase, MainMenuRuntimeView, ThemeMainMenu};
use deadsync_theme::{Screen, ThemeEffect, TransitionState, UiCue};
use deadsync_theme_stepmania_395::{PackageTitle, PackageTitleState, TITLE_FONT};

#[test]
fn authored_title_uses_its_layout_font_and_focus_commands_with_shared_navigation() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path();
    fs::write(
        root.join("metrics.ini"),
        concat!(
            "[ScreenTitleMenu]\nFallback=ScreenTitleDefaults\nChoiceNames=1,2\n",
            "Choice1=screen,ScreenSelectProfile\nChoice2=screen,ScreenOptionsMenu\n",
            "[ScreenTitleDefaults]\nScrollerX=SCREEN_CENTER_X\nScrollerY=160\n",
            "ScrollerTransform=function(self,offset,itemIndex,numItems) self:y(40*offset) end\n",
            "IdleTimeoutSeconds=10\n",
        ),
    )
    .unwrap();
    let graphics = root.join("Graphics");
    fs::create_dir(&graphics).unwrap();
    fs::write(
        graphics.join("ScreenTitleMenu scroll choice1.xml"),
        concat!(
            "<ActorFrame><children><BitmapText File=\"banner\" Text=\"Begin &amp; Dance\" ",
            "OnCommand=\"shadowlength,3\" LoseFocusCommand=\"stoptweening;linear,0.25;zoom,1\" ",
            "GainFocusCommand=\"stoptweening;linear,0.25;zoom,1.25\" /></children></ActorFrame>",
        ),
    )
    .unwrap();
    fs::write(graphics.join("ScreenTitleMenu scroll choice2.xml"), concat!(
        "<BitmapText File=\"banner\" Text=\"Preferences\" OnCommand=\"diffuse,1,1,1,0.8\" ",
        "GainFocusCommand=\"stoptweening;diffuseshift;effectperiod,0.5;effectcolor1,1,1,1,1;",
        "effectcolor2,0.25,0.5,0.75,1;linear,0.25;zoom,1.25\" LoseFocusCommand=\"stopeffect;zoom,1\"/>",
    )).unwrap();
    let fonts = root.join("Fonts");
    fs::create_dir(&fonts).unwrap();
    fs::write(fonts.join("banner.redir"), "heading\n").unwrap();
    fs::write(
        fonts.join("heading.ini"),
        "[Char Widths]\nBaseline=6\nTop=0\nLineSpacing=8\n",
    )
    .unwrap();
    image::RgbaImage::from_pixel(128, 64, image::Rgba([255, 255, 255, 255]))
        .save(fonts.join("heading 16x8.png"))
        .unwrap();
    let background = root.join("BGAnimations/ScreenTitleMenu background");
    fs::create_dir_all(&background).unwrap();
    fs::write(
        background.join("BGAnimation.ini"),
        "[Layer1]\nType=1\nFile=matte.png\n",
    )
    .unwrap();
    image::RgbaImage::from_pixel(32, 16, image::Rgba([24, 48, 80, 255]))
        .save(background.join("matte.png"))
        .unwrap();

    let (mut title, font_path, textures) = PackageTitle::open(root, None).unwrap();
    assert!(font_path.ends_with("heading.ini"));
    assert_eq!(textures.len(), 1);
    assert!(textures[0].path.ends_with("matte.png"));
    let font = parse_font_with_asset_context(&font_path, [root.to_path_buf()])
        .unwrap()
        .font;
    let mut assets = AssetManager::new();
    assets.register_font(TITLE_FONT, font);
    let font = &assets.fonts()[TITLE_FONT];
    assert!(font.ascii_glyphs[b'B' as usize].is_some());
    assert_eq!(font.line_spacing, 8);

    PackageTitle::update_menu::<()>(&mut title, 0.25, MainMenuPhase::Active);
    let first = choice_actors(&title);
    assert!(
        matches!(&first[0], Actor::Text { font: TITLE_FONT, content, offset, scale, shadow_len, .. }
        if content.as_str() == "Begin & Dance" && *offset == [320.0,160.0] && *scale == [1.25,1.25] && *shadow_len == [3.0,-3.0])
    );
    assert!(matches!(&first[1], Actor::Text { offset, .. } if *offset == [320.0,200.0]));

    let moved =
        PackageTitle::handle_menu_input::<()>(&mut title, &input(VirtualAction::p2_menu_down));
    assert!(matches!(moved, ThemeEffect::UiCue(UiCue::Change)));
    PackageTitle::update_menu::<()>(&mut title, 0.125, MainMenuPhase::Active);
    let focused = choice_actors(&title);
    assert!(
        matches!(&focused[1], Actor::Text { effect, offset, color, scale, .. }
        if effect.mode == EffectMode::DiffuseShift && effect.period == 0.5 && *offset == [320.0,160.0] && color[3] == 0.4 && *scale == [1.125,1.125])
    );
    let mut effects = Vec::new();
    PackageTitle::handle_menu_input::<()>(&mut title, &input(VirtualAction::p2_start))
        .append_to(&mut effects);
    assert!(matches!(
        effects.as_slice(),
        [
            ThemeEffect::UiCue(UiCue::Confirm),
            ThemeEffect::Navigate(Screen::Options)
        ]
    ));

    PackageTitle::sync_menu_runtime(
        &mut title,
        MainMenuRuntimeView {
            coin_mode: CoinMode::Pay,
            ..Default::default()
        },
    );
    assert_eq!(choice_actors(&title).len(), 1);
    assert!(matches!(
        PackageTitle::update_menu::<()>(&mut title, 10.0, MainMenuPhase::Active),
        ThemeEffect::Navigate(Screen::Init)
    ));
}

fn choice_actors(title: &PackageTitleState) -> Vec<Actor> {
    let mut actors = Vec::new();
    PackageTitle::push_menu_actors(
        &mut actors,
        title,
        MainMenuFrame {
            update_banner_tag: None,
            background_key: None,
            alpha_multiplier: 0.5,
            transition: &TransitionState::Idle,
            visual_policy: (),
        },
    );
    let [Actor::Frame { children, .. }] = actors.as_mut_slice() else {
        panic!("expected a title frame");
    };
    assert!(matches!(
        children.first(),
        Some(Actor::Sprite { z: i16::MIN, .. })
    ));
    children.drain(1..).collect()
}

fn input(action: VirtualAction) -> InputEvent {
    let now = Instant::now();
    InputEvent::new(action, 0, true, InputSource::Keyboard, now, 0, now, now)
}
