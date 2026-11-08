//! Observe native host poses/output that a VM-free scene owner must preserve.
use deadsync_song_lua::*;
use std::{path::Path, sync::Arc};
type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
fn models(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<SongLuaOverlayModelLayer<()>> {
    None
}
fn visual(_: &SongLuaCompileContext, _: &str) -> Option<(Kind, SongLuaOverlayState)> {
    None
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "native_scene_audit INPUT REPORT");
    let path = Path::new(&args[0]);
    let mut context = SongLuaCompileContext::new(path.parent().unwrap(), "native ownership audit");
    context.live = true;
    context.players[0].enabled = true;
    context.players[1].enabled = false;
    context.music_length_seconds = 4.0;
    let mut compiled = compile_song_lua_layers_with_default_host(
        &[path],
        0,
        &context,
        SongLuaNoteskinResolver::default(),
        models,
        layer,
        visual,
    )
    .unwrap();
    let mut scene = compiled[0].live_scene.take().unwrap();
    let startup = format!("{:?}", scene.take_startup_output());
    let mut previous = scene.tracked_states().collect::<Vec<_>>();
    let mut rows = Vec::new();
    for (index, combo) in [0, 1, 5, 100, 0, 10].into_iter().enumerate() {
        let mut frame = SongLuaLiveFrame::default();
        frame.beat = index as f64;
        frame.music_seconds = index as f64 / 2.0;
        frame.screen_seconds = frame.music_seconds;
        frame.delta_seconds = 0.5;
        frame.players[0].combo = combo;
        frame.players[0].life = if index % 2 == 0 { 0.5 } else { 0.1 };
        let output = scene.update(frame).unwrap();
        let kind = match index % 3 {
            0 => SongLuaInputType::FirstPress,
            1 => SongLuaInputType::Repeat,
            _ => SongLuaInputType::Release,
        };
        let (handled, input_output) = scene
            .input(&SongLuaInputEvent {
                player: Some(0),
                controller: Some(0),
                button: "Right",
                game_button: "Right",
                kind,
                device: "InputDevice_Key",
                device_button: "DeviceButton_right",
                level: 1.0,
                z: 0,
                down: kind != SongLuaInputType::Release,
                ago: 0.0,
                is_joystick: false,
                is_mouse: false,
            })
            .unwrap();
        let draw = scene.draw().unwrap();
        let now = scene.tracked_states().collect::<Vec<_>>();
        let mut changed = Vec::new();
        for ((target, before), (_, after)) in previous.iter().zip(&now) {
            let a = format!("{before:#?}");
            let b = format!("{after:#?}");
            let fields = a
                .lines()
                .zip(b.lines())
                .filter(|(a, b)| a != b)
                .map(|(a, b)| format!("{} -> {}", a.trim(), b.trim()))
                .collect::<Vec<_>>();
            if !fields.is_empty() {
                let name = match *target {
                    SongLuaTrackedActorTarget::Player(p) => format!("Player({p})"),
                    SongLuaTrackedActorTarget::PlayerJudgment(p) => format!("PlayerJudgment({p})"),
                    SongLuaTrackedActorTarget::PlayerCombo(p) => format!("PlayerCombo({p})"),
                    SongLuaTrackedActorTarget::SongForeground => "SongForeground".into(),
                    SongLuaTrackedActorTarget::ScreenLayer(p) => format!("ScreenLayer({p})"),
                };
                changed.push(serde_json::json!({"target":name,"fields":fields}));
            }
        }
        rows.push(serde_json::json!({"index":index,"combo":combo,"changed_tracked_poses":changed,"output":format!("{output:?}"),"input_output":format!("{input_output:?}"),"draw":format!("{draw:?}"),"handled":handled,"states":format!("{:?}",scene.states())}));
        previous = now;
    }
    std::fs::write(
        &args[1],
        serde_json::to_vec_pretty(&serde_json::json!({"startup":startup,"frames":rows})).unwrap(),
    )
    .unwrap();
}
