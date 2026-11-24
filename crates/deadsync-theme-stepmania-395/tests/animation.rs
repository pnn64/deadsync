use std::fs;

use deadlib_present::anim::TweenState;
use deadsync_theme_stepmania_395::{ActorAnimation, AnimationError, ThemePackage};

#[test]
fn inherited_metric_uses_initial_properties_and_delayed_transforms() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("metrics.ini"),
        concat!(
            "[ScreenPanel]\nFallback=ScreenShared\n",
            "[ScreenShared]\nOnCommand=",
            "X,(SCREEN_CENTER_X - 20);zoomtowidth,100;diffusealpha,0;",
            "sleep,0.25;addx,20;linear,0.5;diffusealpha,1;addx,40;",
            "horizalign,RIGHT;vertalign,top;hidden,1\n",
        ),
    )
    .unwrap();
    let package = ThemePackage::open([root.path()], None).unwrap();
    let animation = package
        .animation_metric("ScreenPanel", "OnCommand", [640.0, 480.0])
        .unwrap()
        .unwrap();
    let mut timeline = animation.start(TweenState {
        w: 50.0,
        h: 25.0,
        ..Default::default()
    });

    assert_eq!(timeline.state().x, 300.0);
    assert_eq!(timeline.state().scale, [2.0, 1.0]);
    assert_eq!(timeline.state().tint[3], 0.0);
    assert_eq!([timeline.state().hx, timeline.state().vy], [1.0, 0.0]);
    assert!(!timeline.state().visible);
    timeline.update(0.125);
    assert_eq!(timeline.state().x, 300.0);
    timeline.update(0.375);
    assert_eq!(timeline.state().x, 340.0);
    assert_eq!(timeline.state().tint[3], 0.5);
    timeline.update(0.25);
    assert_eq!(timeline.state().x, 360.0);
    assert_eq!(timeline.state().tint[3], 1.0);
    assert!(timeline.is_empty());
    assert!(
        package
            .animation_metric("ScreenPanel", "OffCommand", [640.0, 480.0])
            .unwrap()
            .is_none()
    );
}

#[test]
fn height_changes_leave_horizontal_scale_unchanged() {
    let animation =
        ActorAnimation::parse("diffuse,1,0.5,0.25,0.75;zoomtoheight,20", [640.0, 480.0]).unwrap();
    let timeline = animation.start(TweenState {
        w: 25.0,
        h: 40.0,
        scale: [3.0, 1.0],
        ..Default::default()
    });

    assert_eq!(timeline.state().scale, [3.0, 0.5]);
    assert_eq!(timeline.state().tint, [1.0, 0.5, 0.25, 0.75]);
    assert!(timeline.is_empty());
}

#[test]
fn unsupported_and_malformed_commands_identify_the_failed_statement() {
    assert!(matches!(
        ActorAnimation::parse("sleep,0.25;queuecommand,Refresh", [640.0, 480.0]),
        Err(AnimationError::UnsupportedCommand { statement: 2, command })
            if command == "queuecommand"
    ));
    for commands in ["linear,-1", "x,SURFACE_WIDTH", "diffuse,1,0,1", "zoom,1,2"] {
        assert!(matches!(
            ActorAnimation::parse(commands, [640.0, 480.0]),
            Err(AnimationError::InvalidArguments { statement: 1, .. })
        ));
    }
}
