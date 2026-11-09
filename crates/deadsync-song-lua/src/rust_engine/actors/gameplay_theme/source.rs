//! Fresh chunks preserve the bundled source's filename, lines and environment.
use super::*;
use deadlib_lua::NativeLoadStatus;

#[derive(Clone, Copy)]
pub(super) struct Source {
    bytes: &'static [u8],
    name: &'static [u8],
}

macro_rules! bundled {
    ($path:literal) => {
        Source {
            bytes: include_bytes!(concat!("../../../../assets/simply-love/", $path)),
            name: concat!("@theme:/", $path).as_bytes(),
        }
    };
}

pub(super) const HUNDRED: Source = bundled!("Graphics/Combo 100milestone/default.lua");
pub(super) const THOUSAND: Source = bundled!("Graphics/Combo 1000milestone/default.lua");
pub(super) const TOASTY: Source = bundled!("BGAnimations/ScreenGameplay toasty.lua");
pub(super) const NEXT_SONG: Source =
    bundled!("BGAnimations/ScreenGameplay next course song/default.lua");
pub(super) const BLANK: Source = Source {
    bytes: b"return Def.Actor{};",
    name: b"@theme:/BGAnimations/_blank.lua",
};

pub(super) fn evaluate(
    call: &mut HostCall<'_>,
    source: Source,
    environment: Value,
    arguments: &[Value],
) -> Result<Value, Error> {
    let mut values = Vec::with_capacity(arguments.len() + 1);
    values.push(environment);
    values.extend_from_slice(arguments);
    call.with_roots(&values, |call| {
        let loader = call.new_host_function_with_data(evaluate_callback, &[], source)?;
        Ok(call.call(loader, &values)?[0])
    })
}

fn evaluate_callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let source = *call.function_data::<Source>().unwrap();
    let mut stack = call.native_stack()?;
    let count = stack.top();
    if stack.load_buffer(source.bytes, Some(source.name))? != NativeLoadStatus::Ok {
        return Err(stack.raise(stack.value(-1).unwrap()));
    }
    stack.push(stack.value(1).unwrap())?;
    stack.set_environment(-2)?;
    for index in 2..=count {
        stack.push(stack.value(index as isize).unwrap())?;
    }
    stack.call(count - 1, Some(1))?;
    stack.finish(1)
}

pub(super) fn environment(
    call: &mut HostCall<'_>,
    globals: Value,
    forward_writes: bool,
) -> Result<Value, Error> {
    let factory =
        call.new_host_function_with_data(environment_callback, &[globals], forward_writes)?;
    Ok(call.call(factory, &[])?[0])
}

fn environment_callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = call.captures()[0];
    let forward = *call.function_data::<bool>().unwrap();
    let mut stack = call.native_stack()?;
    stack.create_table(0, 1)?;
    stack.create_table(0, 2)?;
    stack.push(globals)?;
    stack.set_field(-2, b"__index")?;
    if forward {
        stack.push(globals)?;
        stack.set_field(-2, b"__newindex")?;
    }
    stack.set_metatable(-2)?;
    stack.finish(1)
}

pub(super) const HISTOGRAM: Source = bundled!("Scripts/SL-Histogram.lua");
pub(super) const UTF8: Source = bundled!("Scripts/utf8.lua");
pub(super) const TEXT_HELPERS: Source = bundled!("Scripts/SL-Helpers-Text.lua");
pub(super) const DANGER: Source =
    bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/Danger.lua");
pub(super) const SONG_INFO: Source =
    bundled!("BGAnimations/ScreenGameplay underlay/Shared/SongInfoBar.lua");

pub(super) fn statistics(name: &str) -> Option<Source> {
    Some(match name {
        "default.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/default.lua")
        }
        "Banner.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/Banner.lua")
        }
        "DarkBackground.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/DarkBackground.lua"
        ),
        "DensityGraph.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/DensityGraph.lua"
        ),
        "HoldsMinesRolls.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/HoldsMinesRolls.lua"
        ),
        "Scorebox.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/Scorebox.lua")
        }
        "TapNoteJudgments.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/TapNoteJudgments.lua"
        ),
        "Time.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/Time.lua")
        }
        _ => return None,
    })
}

pub(super) fn note_field(name: &str) -> Option<Source> {
    Some(match name {
        "ColumnCues.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ColumnCues.lua")
        }
        "ColumnFlashOnMiss.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ColumnFlashOnMiss.lua"
        ),
        "default.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/default.lua")
        }
        "DisplayMods.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/DisplayMods.lua")
        }
        "ErrorBar/Colorful.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ErrorBar/Colorful.lua"
        ),
        "ErrorBar/default.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ErrorBar/default.lua"
        ),
        "ErrorBar/Monochrome.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ErrorBar/Monochrome.lua"
        ),
        "ErrorBar/Text.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ErrorBar/Text.lua")
        }
        "MeasureCounter.lua" => {
            bundled!("BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/MeasureCounter.lua")
        }
        "SubtractiveScoring.lua" => bundled!(
            "BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/SubtractiveScoring.lua"
        ),
        _ => return None,
    })
}

pub(super) const HEADER: Source = bundled!("Graphics/_header.lua");
pub(super) const FOOTER: Source = bundled!("Graphics/_footer.lua");
pub(super) const DECORATION_ON: Source = Source {
    bytes: b"return function(self) self:hibernate(math.huge) end",
    name: b"@theme:/metrics/ScreenGameplay.lua",
};

pub(super) fn overlay(name: &str) -> Option<Source> {
    Some(match name {
        "default.lua" => bundled!("BGAnimations/ScreenGameplay overlay/default.lua"),
        "WhoIsCurrentlyWinning.lua" => {
            bundled!("BGAnimations/ScreenGameplay overlay/WhoIsCurrentlyWinning.lua")
        }
        "FailOnHoldStart.lua" => {
            bundled!("BGAnimations/ScreenGameplay overlay/FailOnHoldStart.lua")
        }
        "TrackTimeSpentInGameplay.lua" => {
            bundled!("BGAnimations/ScreenGameplay overlay/TrackTimeSpentInGameplay.lua")
        }
        "JudgmentOffsetTracking.lua" => {
            bundled!("BGAnimations/ScreenGameplay overlay/JudgmentOffsetTracking.lua")
        }
        "TrackExScoreJudgments.lua" => {
            bundled!("BGAnimations/ScreenGameplay overlay/TrackExScoreJudgments.lua")
        }
        "TrackFailTime.lua" => bundled!("BGAnimations/ScreenGameplay overlay/TrackFailTime.lua"),
        "PerColumnJudgmentTracking.lua" => {
            bundled!("BGAnimations/ScreenGameplay overlay/PerColumnJudgmentTracking.lua")
        }
        _ => return None,
    })
}
