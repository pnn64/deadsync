use std::{fs, path::Path, time::Duration};

use deadlib_assets::TextureDecodeJob;
use deadlib_present::actors::Actor;
use deadsync_theme::stage::{StageCardMode, StageCardPresentation, StageCardView};
use deadsync_theme_stepmania_395::PackageStageCard;

#[test]
fn stage_card_draws_authored_layers_for_the_active_stage_and_phase() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("metrics.ini"),
        "[ScreenStage]\nMinimumDelay=0.5\n",
    )
    .unwrap();
    scene(
        root.path(),
        "background",
        "[Layer1]\nFile=matte.png\n",
        &["matte.png"],
    );
    scene(
        root.path(),
        "overlay",
        concat!(
            "[Layer1]\nFile=second.png\nCondition=GetStageText() == \"2\"\n",
            "[Layer2]\nFile=event.png\nCondition=GetStageText() == \"event\"\n",
            "[Layer3]\nFile=rule.png\nCommand=y,160;linear,0.5;y,210\n",
        ),
        &["second.png", "event.png", "rule.png"],
    );
    scene(
        root.path(),
        "in",
        "[Layer1]\nFile=entry.png\nCommand=sleep,0.25\n",
        &["entry.png"],
    );
    scene(
        root.path(),
        "out",
        "[Layer1]\nFile=exit.png\nCommand=diffusealpha,0;linear,0.5;diffusealpha,1\n",
        &["exit.png"],
    );

    let (card, textures) = PackageStageCard::open(root.path(), None).unwrap();
    let timing = card.timing();
    assert_eq!(timing.hold, Duration::from_millis(750));
    assert_eq!(timing.exit, Duration::from_millis(500));
    let render = |mode, elapsed, exit_elapsed| {
        let mut actors = Vec::new();
        card.push_actors(
            &mut actors,
            StageCardView {
                stage_number: 2,
                mode,
                elapsed,
                exit_elapsed,
            },
        );
        match actors.pop() {
            Some(Actor::Frame { children, .. }) => children,
            _ => panic!("expected a stage card frame"),
        }
    };
    let entry = render(StageCardMode::Regular, 0.125, None);
    assert_eq!(
        names(&entry, &textures),
        ["matte.png", "second.png", "rule.png", "entry.png"]
    );
    let hold = render(StageCardMode::Event, 0.5, None);
    assert_eq!(
        names(&hold, &textures),
        ["matte.png", "event.png", "rule.png"]
    );
    assert!(matches!(
        hold[2],
        Actor::Sprite {
            offset: [320.0, 210.0],
            ..
        }
    ));
    let exit = render(StageCardMode::Final, 1.0, Some(0.25));
    assert_eq!(
        names(&exit, &textures),
        ["matte.png", "second.png", "rule.png", "exit.png"]
    );
    assert!(matches!(
        exit[3],
        Actor::Sprite {
            tint: [1.0, 1.0, 1.0, 0.5],
            ..
        }
    ));
}

fn scene(root: &Path, name: &str, ini: &str, sprites: &[&str]) {
    let dir = root
        .join("BGAnimations")
        .join(format!("ScreenStage {name}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("BGAnimation.ini"), ini).unwrap();
    for sprite in sprites {
        image::RgbaImage::from_pixel(24, 12, image::Rgba([72, 48, 128, 255]))
            .save(dir.join(sprite))
            .unwrap();
    }
}

fn names<'a>(actors: &[Actor], textures: &'a [TextureDecodeJob]) -> Vec<&'a str> {
    actors
        .iter()
        .map(|actor| {
            let Actor::Sprite { source, .. } = actor else {
                panic!("expected a stage sprite");
            };
            let job = textures
                .iter()
                .find(|job| Some(job.key.as_str()) == source.texture_key())
                .unwrap();
            job.path.file_name().unwrap().to_str().unwrap()
        })
        .collect()
}
