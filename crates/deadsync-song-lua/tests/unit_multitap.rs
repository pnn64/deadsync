//! Original multitap travel unit on the Rust VM.
use deadlib_lua::Value;
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaSpeedMod,
    rust_engine::{RustSongRuntime, test_support},
};
#[path = "support/baked_program.rs"]
mod api;
#[test]
fn multitap_travel_obeys_chart_timing_and_speedmod() {
    use deadsync_rules::timing::{
        ScrollSegment, SpeedSegment, SpeedUnit, TimingData, TimingSegments,
    };
    let mut context = SongLuaCompileContext::new(".", "Multitap timing");
    context.player_timing[0] = Some(TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            bpms: vec![(0.0, 120.0), (8.0, 240.0)],
            scrolls: vec![
                ScrollSegment {
                    beat: 0.0,
                    ratio: 0.5,
                },
                ScrollSegment {
                    beat: 8.0,
                    ratio: 0.25,
                },
            ],
            speeds: vec![
                SpeedSegment {
                    beat: 0.0,
                    ratio: 0.5,
                    delay: 0.0,
                    unit: SpeedUnit::Beats,
                },
                SpeedSegment {
                    beat: 4.0,
                    ratio: 1.0,
                    delay: 4.0,
                    unit: SpeedUnit::Beats,
                },
            ],
            ..Default::default()
        },
        &[],
    ));
    context.players[0].display_bpms = [120.0, 240.0];
    for (speed, rate, expected) in [
        (SongLuaSpeedMod::X(1.0), 1.0, 72.0),
        (SongLuaSpeedMod::X(2.0), 1.5, 144.0),
        (SongLuaSpeedMod::M(480.0), 2.0, 72.0),
        (SongLuaSpeedMod::C(120.0), 1.0, 192.0),
        (SongLuaSpeedMod::C(120.0), 2.0, 96.0),
    ] {
        context.players[0].speedmod = speed;
        context.song_music_rate = rate;
        assert!((test_support::multitap_y_offset(&context, 0, 6.0, 4.0) - expected).abs() < 0.001);
        let mut host = RustSongRuntime::new(&context).unwrap();
        host.set_values(6.0, 3.0 / rate).unwrap();
        let (method, value) = match speed {
            SongLuaSpeedMod::X(value) => ("XMod", value),
            SongLuaSpeedMod::M(value) => ("MMod", value),
            SongLuaSpeedMod::C(value) => ("CMod", value),
            _ => unreachable!(),
        };
        let value_method = api::string(host.vm_mut(), method);
        host.vm_mut()
            .set_global(b"speed_method", value_method)
            .unwrap();
        host.vm_mut()
            .set_global(b"speed_value", Value::Number(value as f64))
            .unwrap();
        let actual = api::as_number(
            api::eval(
                host.vm_mut(),
                r#"
                local options = {__songlua_reference_bpm=240}
                options[speed_method] = function() return speed_value end
                local ps = {GetPlayerNumber=function() return "PlayerNumber_P1" end,
                    GetCurrentPlayerOptions=function() return options end}
                return ArrowEffects.GetYOffset(ps, 1, 10) - ArrowEffects.GetYOffset(ps, 1, 6)
            "#,
            )[0],
        ) as f32;
        assert!(
            (actual - expected).abs() < 0.001,
            "Lua ArrowEffects {speed:?}: actual {actual}, expected {expected}"
        );
    }
}
