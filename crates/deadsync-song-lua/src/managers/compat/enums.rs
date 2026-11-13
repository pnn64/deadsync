//! Existing enum lists, shared Reverse tables and player-option capabilities.
use super::*;
struct Context {
    song_dir: String,
    confusion_offset: bool,
    confusion: bool,
    amod: bool,
}
pub(super) fn install(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    let initialize = vm.new_host_function_with_data(
        initialize,
        &[globals],
        Context {
            song_dir: crate::song_dir_string(&context.song_dir),
            confusion_offset: context.confusion_offset_available,
            confusion: context.confusion_available,
            amod: context.amod_available,
        },
    )?;
    vm.call(initialize, &[]).map(|_| ())
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, call.captures()[0])?;
    for (name, names) in [
        (
            "CullMode",
            &["CullMode_Back", "CullMode_Front", "CullMode_None"][..],
        ),
        (
            "EditState",
            &[
                "EditState_Edit",
                "EditState_Record",
                "EditState_RecordPaused",
                "EditState_Playing",
            ][..],
        ),
        (
            "ProfileSlot",
            &[
                "ProfileSlot_Player1",
                "ProfileSlot_Player2",
                "ProfileSlot_Machine",
            ][..],
        ),
        (
            "GameController",
            &["GameController_1", "GameController_2"][..],
        ),
        (
            "PlayerController",
            &[
                "PlayerController_Human",
                "PlayerController_Autoplay",
                "PlayerController_Cpu",
            ][..],
        ),
        (
            "HealthState",
            &[
                "HealthState_Hot",
                "HealthState_Alive",
                "HealthState_Danger",
                "HealthState_Dead",
            ][..],
        ),
        (
            "SortOrder",
            &[
                "SortOrder_Group",
                "SortOrder_Title",
                "SortOrder_Artist",
                "SortOrder_BPM",
                "SortOrder_Popularity",
                "SortOrder_Preferred",
                "SortOrder_Recent",
            ][..],
        ),
        (
            "TapNoteScore",
            &[
                "TapNoteScore_None",
                "TapNoteScore_HitMine",
                "TapNoteScore_AvoidMine",
                "TapNoteScore_CheckpointMiss",
                "TapNoteScore_Miss",
                "TapNoteScore_W5",
                "TapNoteScore_W4",
                "TapNoteScore_W3",
                "TapNoteScore_W2",
                "TapNoteScore_W1",
                "TapNoteScore_CheckpointHit",
            ][..],
        ),
        (
            "TapNoteType",
            &[
                "TapNoteType_Empty",
                "TapNoteType_Tap",
                "TapNoteType_HoldHead",
                "TapNoteType_HoldTail",
                "TapNoteType_Mine",
                "TapNoteType_Lift",
                "TapNoteType_Attack",
                "TapNoteType_AutoKeysound",
                "TapNoteType_Fake",
            ][..],
        ),
        (
            "TapNoteSource",
            &[
                "TapNoteSource_Original",
                "TapNoteSource_Addition",
                "TapNoteSource_Mine",
            ][..],
        ),
        (
            "TapNoteSubType",
            &[
                "TapNoteSubType_Invalid",
                "TapNoteSubType_Hold",
                "TapNoteSubType_Roll",
            ][..],
        ),
        (
            "HoldNoteScore",
            &[
                "HoldNoteScore_Held",
                "HoldNoteScore_LetGo",
                "HoldNoteScore_MissedHold",
                "HoldNoteScore_None",
            ][..],
        ),
        (
            "StepsType",
            &["StepsType_Dance_Single", "StepsType_Dance_Double"][..],
        ),
        (
            "StyleType",
            &[
                "StyleType_OnePlayerOneSide",
                "StyleType_OnePlayerTwoSides",
                "StyleType_TwoPlayersTwoSides",
            ][..],
        ),
    ] {
        install_enum(call, globals, name, names, false)?;
    }
    install_enum(
        call,
        globals,
        "Difficulty",
        &[
            "Difficulty_Beginner",
            "Difficulty_Easy",
            "Difficulty_Medium",
            "Difficulty_Hard",
            "Difficulty_Challenge",
            "Difficulty_Edit",
        ],
        false,
    )?;
    install_enum(
        call,
        globals,
        "ModTimerType",
        &crate::player_options::MOD_TIMER_NAMES,
        true,
    )?;
    let options = object(call, |call, out| {
        for name in crate::SONG_LUA_PLAYER_OPTION_CAPABILITIES {
            set(out, *name, true, call)?;
        }
        for prefix in crate::SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES {
            for column in 1..=16 {
                set(out, format!("{prefix}{column}"), true, call)?;
            }
        }
        let context = call.function_data::<Context>().unwrap();
        let values = [
            ("ConfusionOffset", context.confusion_offset),
            ("Confusion", context.confusion),
            ("AMod", context.amod),
        ];
        for (name, value) in values {
            set(out, name, value, call)?;
        }
        Ok(())
    })?;
    call.with_roots(&[Value::Table(options)], |call| {
        set(globals, "PlayerOptions", options, call)
    })?;
    let song_dir = call.function_data::<Context>().unwrap().song_dir.clone();
    set(globals, "__songlua_song_dir", song_dir, call)
}
fn install_enum(
    call: &mut HostCall<'_>,
    globals: TableId,
    name: &str,
    names: &[&str],
    timer: bool,
) -> Result<(), Error> {
    let out = array(call, names.iter().copied())?;
    call.with_roots(&[Value::Table(out)], |call| {
        let reverse = object(call, |call, reverse| {
            for (i, name) in names.iter().enumerate() {
                set(reverse, *name, i, call)?;
            }
            if timer {
                for (i, label) in ["game", "beat", "song", "default"].iter().enumerate() {
                    set(reverse, *label, i, call)?;
                    set(reverse, i, i, call)?;
                }
            }
            Ok(())
        })?;
        call.with_roots(&[Value::Table(reverse)], |call| {
            let f = call.new_host_function(super::super::constant, &[Value::Table(reverse)])?;
            call.with_roots(&[f], |call| set(out, "Reverse", f, call))
        })?;
        if timer {
            let value = call.try_intern(b"ModTimerType")?;
            let f = call.new_host_function(super::super::constant, &[value])?;
            call.with_roots(&[f], |call| set(out, "GetName", f, call))?;
        }
        set(globals, name, out, call)
    })
}
