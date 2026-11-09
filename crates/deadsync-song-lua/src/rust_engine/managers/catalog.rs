//! Song-owned catalog and chart objects. Lua references live in traced captures.
use super::fields::{Field, array, object, set};
use super::*;
use crate::{SongLuaCompileContext, SongLuaDifficulty, SongLuaStep};
use std::{path::Path, sync::Arc};
mod charts;
mod timing;

#[derive(Clone)]
struct Setup {
    context: SongLuaCompileContext,
    #[cfg(feature = "test-support")]
    observations: crate::rust_engine::observations::Observations,
}

pub(super) fn install(
    vm: &mut Vm,
    context: &crate::SongLuaCompileContext,
    #[cfg(feature = "test-support")] observations: crate::rust_engine::observations::Observations,
) -> Result<deadlib_lua::Root, Error> {
    let captures = [vm.get_global(b"_G")?, vm.get_global(b"tonumber")?];
    let initialize = vm.new_host_function_with_data(
        initialize,
        &captures,
        Setup {
            context: context.clone(),
            #[cfg(feature = "test-support")]
            observations,
        },
    )?;
    let catalog = vm.call(initialize, &[])?[0];
    vm.pin(catalog)
}

fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let setup = call
        .function_data::<Setup>()
        .expect("catalog context")
        .clone();
    let context = setup.context;
    let catalog = object(call, |call, catalog| {
        // Chart density is cached before the native current song option level exists.
        let rate = object(call, |call, rate| {
            set(rate, "__songlua_music_rate", 1.0_f32, call)
        })?;
        set(catalog, "current_song_options", rate, call)?;
        let song = song(call, &context, rate)?;
        set(catalog, "song", song, call)?;
        let steps = object(call, |call, steps| {
            let all = get(call, Value::Table(song), "__songlua_steps")?;
            let all = table(call, all)?;
            for index in 0..crate::LUA_PLAYERS {
                let value = if let Some(selected) = context.player_step_indices[index] {
                    let value = call.raw_table_get(all, Value::Number((selected + 1) as f64))?;
                    table(call, value)?
                } else {
                    charts::steps(
                        call,
                        context.players[index].difficulty,
                        context.players[index].display_bpms,
                        &context.song_dir,
                        crate::song_lua_style_info(&context.style_name).steps_type,
                        context.player_timing[index].as_ref(),
                        rate,
                    )?
                };
                set(steps, index + 1, value, call)?;
            }
            Ok(())
        })?;
        set(catalog, "steps", steps, call)?;
        super::song_options::install(call, catalog, rate, context.song_music_rate)?;
        super::player_options::install(
            call,
            catalog,
            &context,
            #[cfg(feature = "test-support")]
            setup.observations.clone(),
        )?;
        let trails = object(call, |call, trails| {
            for index in 1..=crate::LUA_PLAYERS {
                let steps = call.raw_table_get(steps, Value::Number(index as f64))?;
                let steps = table(call, steps)?;
                let trail = trail(call, song, steps, context.song_display_bpms)?;
                set(trails, index, trail, call)?;
            }
            Ok(())
        })?;
        set(catalog, "trails", trails, call)?;
        let primary = call.raw_table_get(trails, Value::Number(1.0))?;
        let primary = table(call, primary)?;
        let course = course(call, &context, song, primary)?;
        set(catalog, "course", course, call)?;
        let first_steps = call.raw_table_get(steps, Value::Number(1.0))?;
        let manager = songman(call, &context, song, first_steps, course)?;
        let globals = table(call, call.captures()[0])?;
        set(globals, "SONGMAN", manager, call)?;
        let util = object(call, |call, util| {
            for (name, filtered) in [
                ("GetPlayableSteps", false),
                ("GetPlayableStepsByStepsType", true),
            ] {
                let method = call.new_host_function_with_data(song_util, &[], filtered)?;
                set(util, name, method, call)?;
            }
            Ok(())
        })?;
        set(globals, "SongUtil", util, call)?;
        Ok(())
    })?;
    call.push(Value::Table(catalog))
}

pub(super) fn arg(call: &HostCall<'_>, index: usize) -> Value {
    let offset = usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
    call.arguments()
        .get(index + offset)
        .copied()
        .unwrap_or(Value::Nil)
}
pub(super) fn text(call: &HostCall<'_>, value: Value) -> Option<String> {
    let Value::String(id) = value else {
        return None;
    };
    std::str::from_utf8(call.string_bytes(id)?)
        .ok()
        .map(str::to_owned)
}
pub(super) fn float(call: &HostCall<'_>, value: Value) -> Option<f32> {
    match value {
        Value::Number(value) => {
            let value = value as f32;
            value.is_finite().then_some(value)
        }
        Value::String(_) => text(call, value)?.trim().parse().ok(),
        _ => None,
    }
}
pub(super) fn integer(call: &HostCall<'_>, value: Value) -> Option<i32> {
    match value {
        Value::Number(value) if value.is_finite() => {
            Some(value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32)
        }
        Value::String(_) => {
            let value = text(call, value)?;
            let value = value.trim();
            value
                .parse()
                .ok()
                .or_else(|| value.parse::<f32>().ok().map(|n| n.round() as i32))
        }
        _ => None,
    }
}
fn function(value: Value) -> bool {
    matches!(value, Value::Function(_) | Value::Native(_))
}
fn get(call: &mut HostCall<'_>, owner: Value, name: &str) -> Result<Value, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.table_get(owner, key)
}
pub(super) fn raw(call: &mut HostCall<'_>, owner: TableId, name: &str) -> Result<Value, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.raw_table_get(owner, key)
}
fn method(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    value: impl Field,
) -> Result<(), Error> {
    let value = value.value(call)?;
    let callback = call.new_host_function(super::constant, &[value])?;
    set(owner, name, callback, call)
}
fn path_methods(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    has: &str,
    path: Option<&Path>,
) -> Result<(), Error> {
    let path = path.map(crate::file_path_string).unwrap_or_default();
    method(call, owner, has, !path.is_empty())?;
    method(call, owner, name, path)
}
fn radar(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    object(call, |call, table| method(call, table, "GetValue", 0.0_f32))
}

#[cfg(feature = "test-support")]
pub fn fixture_single_value_array(call: &mut HostCall<'_>, value: Value) -> Result<Value, Error> {
    array(call, [value]).map(Value::Table)
}

#[cfg(feature = "test-support")]
pub fn fixture_string_method(
    call: &mut HostCall<'_>, owner: TableId, name: &str, value: &str,
) -> Result<(), Error> {
    method(call, owner, name, value)
}

#[cfg(feature = "test-support")]
pub fn fixture_path_methods(
    call: &mut HostCall<'_>, owner: TableId, name: &str, has: &str, path: Option<&Path>,
) -> Result<(), Error> {
    path_methods(call, owner, name, has, path)
}

pub(super) fn song(
    call: &mut HostCall<'_>,
    context: &SongLuaCompileContext,
    rate: TableId,
) -> Result<TableId, Error> {
    object(call, |call, song| {
        let steps = charts::all_steps(call, context, rate)?;
        set(song, "__songlua_steps", steps, call)?;
        let native = (1..=context.song_steps.len())
            .map(|i| call.raw_table_get(steps, Value::Number(i as f64)))
            .collect::<Result<Vec<_>, _>>()?;
        let metadata: Arc<[SongLuaStep]> = context.song_steps.clone().into();
        for name in [
            "GetAllSteps",
            "GetStepsByStepsType",
            "GetOneSteps",
            "HasStepsType",
            "HasStepsTypeAndDifficulty",
            "HasEdits",
        ] {
            let callback =
                call.new_host_function_with_data(select_steps, &native, (name, metadata.clone()))?;
            set(song, name, callback, call)?;
        }
        method(
            call,
            song,
            "GetSongDir",
            crate::song_dir_string(&context.song_dir),
        )?;
        for name in [
            "GetMainTitle",
            "GetDisplayMainTitle",
            "GetTranslitMainTitle",
            "GetDisplayFullTitle",
            "GetTranslitFullTitle",
        ] {
            method(call, song, name, context.main_title.as_str())?;
        }
        for name in [
            "GetDisplaySubTitle",
            "GetTranslitSubTitle",
            "GetDisplayArtist",
            "GetTranslitArtist",
            "GetOrTryAtLeastToGetSimfileAuthor",
        ] {
            method(call, song, name, "")?;
        }
        method(
            call,
            song,
            "GetGroupName",
            crate::song_group_name(&context.song_dir),
        )?;
        path_methods(
            call,
            song,
            "GetMusicPath",
            "HasMusic",
            crate::song_music_path(&context.song_dir).as_deref(),
        )?;
        let banner = context
            .song_banner_path
            .clone()
            .or_else(|| crate::song_named_image_path(&context.song_dir, &["banner", "bn"]));
        path_methods(call, song, "GetBannerPath", "HasBanner", banner.as_deref())?;
        let callback = call.new_host_function_with_data(has_banner, &[], banner)?;
        set(song, "HasBanner", callback, call)?;
        for (name, has, names) in [
            (
                "GetBackgroundPath",
                "HasBackground",
                &["background", "bg"][..],
            ),
            ("GetJacketPath", "HasJacket", &["jacket", "cover"][..]),
            (
                "GetCDImagePath",
                "HasCDImage",
                &["cdtitle", "cdimage", "disc"][..],
            ),
        ] {
            path_methods(
                call,
                song,
                name,
                has,
                crate::song_named_image_path(&context.song_dir, names).as_deref(),
            )?;
        }
        let length = context.music_length_seconds.max(0.0);
        let [first, last] = context.song_note_bounds.unwrap_or([0.0, length]);
        for (name, value) in [
            ("MusicLengthSeconds", length),
            ("GetFirstSecond", first),
            ("GetLastSecond", last),
            ("GetFirstBeat", 0.0),
            (
                "GetLastBeat",
                length * context.song_display_bpms[1].max(0.0) / 60.0,
            ),
            ("GetStageCost", 1.0),
        ] {
            method(call, song, name, value)?;
        }
        let bpms = array(call, context.song_display_bpms)?;
        method(call, song, "GetDisplayBpms", bpms)?;
        let model = context.song_timing.clone().unwrap_or_else(|| {
            deadsync_rules::timing::TimingData::from_segments(
                0.0,
                0.0,
                &deadsync_rules::timing::TimingSegments {
                    bpms: context.song_timing_bpms.clone(),
                    ..Default::default()
                },
                &[],
            )
        });
        let timing = timing::create(call, context.song_display_bpms, Some(&model), rate)?;
        method(call, song, "GetTimingData", timing)
    })
}

pub(super) fn statistics_steps(
    call: &mut HostCall<'_>,
    context: &SongLuaCompileContext,
    player: usize,
    rate: TableId,
) -> Result<TableId, Error> {
    charts::steps(
        call,
        context.players[player].difficulty,
        context.players[player].display_bpms,
        &context.song_dir,
        crate::song_lua_style_info(&context.style_name).steps_type,
        None,
        rate,
    )
}
fn has_banner(call: &mut HostCall<'_>) -> Result<(), Error> {
    let present = call
        .function_data::<Option<std::path::PathBuf>>()
        .expect("banner")
        .as_ref()
        .is_some_and(|p| p.is_file());
    call.push(Value::Boolean(present))
}

const STEPS_TYPES: [&str; 38] = [
    "dance-single",
    "dance-double",
    "dance-couple",
    "dance-solo",
    "dance-threepanel",
    "dance-routine",
    "pump-single",
    "pump-halfdouble",
    "pump-double",
    "pump-couple",
    "pump-routine",
    "kb7-single",
    "ez2-single",
    "ez2-double",
    "ez2-real",
    "para-single",
    "ds3ddx-single",
    "beat-single5",
    "beat-versus5",
    "beat-double5",
    "beat-single7",
    "beat-versus7",
    "beat-double7",
    "maniax-single",
    "maniax-double",
    "techno-single4",
    "techno-single5",
    "techno-single8",
    "techno-double4",
    "techno-double5",
    "techno-double8",
    "popn-five",
    "popn-nine",
    "lights-cabinet",
    "kickbox-human",
    "kickbox-quadarm",
    "kickbox-insect",
    "kickbox-arachnid",
];
fn difficulty(call: &HostCall<'_>, value: Value) -> Option<SongLuaDifficulty> {
    use SongLuaDifficulty::*;
    if let Value::Number(n) = value {
        return ((0.0..6.0).contains(&n) && n.fract() == 0.0)
            .then(|| [Beginner, Easy, Medium, Hard, Challenge, Edit][n as usize]);
    }
    crate::with_compact_ascii_lowercase_song_key(
        &text(call, value)?,
        crate::song_lua_difficulty_from_normalized,
    )
}
fn select_steps(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (name, steps) = call
        .function_data::<(&'static str, Arc<[SongLuaStep]>)>()
        .expect("chart selector")
        .clone();
    let type_value = arg(call, 0);
    let type_name = if name == "GetAllSteps" {
        None
    } else if let Value::Number(n) = type_value {
        if n < 0.0 || n >= STEPS_TYPES.len() as f64 || n.fract() != 0.0 {
            return Err(call.error(b"Expected StepsType"));
        }
        Some(STEPS_TYPES[n as usize].to_owned())
    } else {
        text(call, type_value)
    };
    let diff_value = arg(call, 1);
    let diff = if name == "HasEdits" {
        Some(SongLuaDifficulty::Edit)
    } else {
        difficulty(call, diff_value)
    };
    if matches!(name, "GetOneSteps" | "HasStepsTypeAndDifficulty")
        && matches!(diff_value, Value::Number(_))
        && diff.is_none()
    {
        return Err(call.error(b"Expected Difficulty"));
    }
    let result = object(call, |call, result| {
        let mut count = 0;
        for (index, step) in steps.iter().enumerate() {
            let type_matches = type_name.as_ref().is_some_and(|name| {
                crate::with_compact_ascii_lowercase_song_key(name, |name| {
                    crate::with_compact_ascii_lowercase_song_key(&step.steps_type, |kind| {
                        name.trim_start_matches("stepstype") == kind.trim_start_matches("stepstype")
                    })
                })
            });
            let diff_matches = !matches!(
                name,
                "GetOneSteps" | "HasStepsTypeAndDifficulty" | "HasEdits"
            ) || diff == Some(step.difficulty);
            if name == "GetAllSteps" || type_matches && diff_matches {
                count += 1;
                set(result, count, call.captures()[index], call)?;
            }
        }
        Ok(())
    })?;
    let value = match name {
        "HasStepsType" | "HasStepsTypeAndDifficulty" | "HasEdits" => {
            Value::Boolean(call.raw_table_len(result)? != 0)
        }
        "GetOneSteps" => call.raw_table_get(result, Value::Number(1.0))?,
        _ => Value::Table(result),
    };
    call.push(value)
}

fn song_util(call: &mut HostCall<'_>) -> Result<(), Error> {
    let filtered = *call.function_data::<bool>().expect("SongUtil method");
    for index in 0..call.arguments().len() {
        let song = call.arguments()[index];
        if !matches!(song, Value::Table(_)) || !function(get(call, song, "GetAllSteps")?) {
            continue;
        }
        let name = if filtered {
            "GetStepsByStepsType"
        } else {
            "GetAllSteps"
        };
        let argument = if filtered {
            match call.arguments().get(index + 1).copied() {
                Some(v) => v,
                None => call.try_intern(b"StepsType_Dance_Single")?,
            }
        } else {
            Value::Nil
        };
        return call.with_roots(&[argument], |call| {
            let method = get(call, song, name)?;
            let result = if function(method) {
                let args = [song, argument];
                call.call(method, &args[..if argument == Value::Nil { 1 } else { 2 }])?
                    .first()
                    .copied()
                    .unwrap_or(Value::Nil)
            } else {
                Value::Table(call.try_new_table()?)
            };
            call.push(result)
        });
    }
    let result = call.try_new_table()?;
    call.push(Value::Table(result))
}

fn entry(call: &mut HostCall<'_>, song: TableId, steps: TableId) -> Result<TableId, Error> {
    object(call, |call, entry| {
        method(call, entry, "GetSong", song)?;
        method(call, entry, "GetSteps", steps)?;
        method(call, entry, "GetCourseEntryType", "CourseEntryType_Fixed")?;
        method(call, entry, "GetNormalModifiers", "")?;
        method(call, entry, "GetAttackModifiers", "")?;
        method(call, entry, "IsSecret", false)
    })
}
fn trail(
    call: &mut HostCall<'_>,
    song: TableId,
    steps: TableId,
    bpms: [f32; 2],
) -> Result<TableId, Error> {
    object(call, |call, trail| {
        set(trail, "__songlua_steps", steps, call)?;
        let get_type = get(call, Value::Table(steps), "GetStepsType")?;
        if !function(get_type) {
            return Err(call.error(b"Steps getter must be a function"));
        }
        let kind = call
            .call(get_type, &[Value::Table(steps)])?
            .first()
            .copied()
            .unwrap_or(Value::Nil);
        let kind = text(call, kind).ok_or_else(|| call.error(b"StepsType string expected"))?;
        method(call, trail, "GetStepsType", kind)?;
        let entry = entry(call, song, steps)?;
        let entries = array(call, [entry])?;
        method(call, trail, "GetTrailEntries", entries)?;
        let callback =
            call.new_host_function(trail_entry, &[Value::Table(entries), Value::Table(entry)])?;
        set(trail, "GetTrailEntry", callback, call)?;
        method(call, trail, "GetDifficulty", "Difficulty_Medium")?;
        method(call, trail, "GetMeter", 1_i32)?;
        let bpms = array(call, bpms)?;
        method(call, trail, "GetDisplayBpms", bpms)?;
        let callback = call.new_host_function(fresh_radar, &[])?;
        set(trail, "GetRadarValues", callback, call)
    })
}
fn trail_entry(call: &mut HostCall<'_>) -> Result<(), Error> {
    let index = integer(call, arg(call, 0)).unwrap_or(0).max(0) as usize + 1;
    let entries = table(call, call.captures()[0])?;
    let value = call.raw_table_get(entries, Value::Number(index as f64))?;
    if value != Value::Nil {
        table(call, value)?;
    }
    call.push(if value == Value::Nil {
        call.captures()[1]
    } else {
        value
    })
}
fn fresh_radar(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = radar(call)?;
    call.push(Value::Table(value))
}
fn course(
    call: &mut HostCall<'_>,
    context: &SongLuaCompileContext,
    song: TableId,
    trail: TableId,
) -> Result<TableId, Error> {
    object(call, |call, course| {
        for name in [
            "GetDisplayMainTitle",
            "GetDisplayFullTitle",
            "GetTranslitMainTitle",
            "GetTranslitFullTitle",
        ] {
            method(call, course, name, context.main_title.as_str())?;
        }
        for name in ["GetDescription", "GetScripter"] {
            method(call, course, name, "")?;
        }
        method(
            call,
            course,
            "GetCourseDir",
            crate::file_path_string(&context.song_dir.join("compat-course.crs")),
        )?;
        method(call, course, "GetCourseType", "CourseType_Nonstop")?;
        method(call, course, "GetDifficulty", "Difficulty_Medium")?;
        for (name, has, names) in [
            ("GetBannerPath", "HasBanner", &["banner", "bn"][..]),
            (
                "GetBackgroundPath",
                "HasBackground",
                &["background", "bg"][..],
            ),
        ] {
            path_methods(
                call,
                course,
                name,
                has,
                crate::song_named_image_path(&context.song_dir, names).as_deref(),
            )?;
        }
        let steps = raw(call, trail, "__songlua_steps")?;
        let steps = table(call, steps)?;
        let entry = entry(call, song, steps)?;
        let entries = array(call, [entry])?;
        method(call, course, "GetCourseEntries", entries)?;
        let trails = array(call, [trail])?;
        method(call, course, "GetAllTrails", trails)?;
        method(call, course, "GetTrail", trail)?;
        method(call, course, "GetEstimatedNumStages", 1_i32)?;
        for (name, value) in [
            ("AllSongsAreFixed", true),
            ("IsAutogen", false),
            ("IsEndless", false),
            ("IsPlayable", true),
        ] {
            method(call, course, name, value)?;
        }
        Ok(())
    })
}

#[derive(Clone)]
struct Lookup {
    name: &'static str,
    dir: String,
    course_dir: String,
    group: String,
    title: String,
    banner: String,
}
fn songman(
    call: &mut HostCall<'_>,
    context: &SongLuaCompileContext,
    song: TableId,
    steps: Value,
    course: TableId,
) -> Result<TableId, Error> {
    object(call, |call, manager| {
        let group_name = crate::song_group_name(&context.song_dir);
        for (name, value) in [
            ("GetSongFromSteps", song),
            ("GetRandomSong", song),
            ("GetRandomCourse", course),
        ] {
            method(call, manager, name, value)?;
        }
        let group_songs = array(call, [song])?;
        method(call, manager, "GetPreferredSortSongs", group_songs)?;
        method(call, manager, "GetPopularSongs", group_songs)?;
        let group_courses = array(call, [course])?;
        method(call, manager, "GetPreferredSortCourses", group_courses)?;
        method(call, manager, "GetPopularCourses", group_courses)?;
        for (name, value) in [("GetAllSongs", song), ("GetAllCourses", course)] {
            let list = array(call, [value])?;
            method(call, manager, name, list)?;
        }
        for name in ["GetSongGroupNames", "GetCourseGroupNames"] {
            let list = array(call, [group_name.as_str()])?;
            method(call, manager, name, list)?;
        }
        for name in [
            "FindSong",
            "FindCourse",
            "GetSongsInGroup",
            "GetCoursesInGroup",
            "DoesSongGroupExist",
            "DoesCourseGroupExist",
            "GetSongGroupBannerPath",
            "GetCourseGroupBannerPath",
            "SongToPreferredSortSectionName",
            "GetPreferredSortSongsBySectionName",
        ] {
            let retained = match name {
                "FindSong" => song,
                "FindCourse" => course,
                "GetCoursesInGroup" => group_courses,
                _ => group_songs,
            };
            let data = Lookup {
                name,
                dir: crate::song_dir_string(&context.song_dir),
                course_dir: crate::file_path_string(&context.song_dir.join("compat-course.crs")),
                group: group_name.clone(),
                title: context.main_title.clone(),
                banner: crate::song_named_image_path(&context.song_dir, &["banner", "bn"])
                    .as_deref()
                    .map(crate::file_path_string)
                    .unwrap_or_default(),
            };
            let callback =
                call.new_host_function_with_data(lookup, &[Value::Table(retained)], data)?;
            set(manager, name, callback, call)?;
        }
        let callback = call.new_host_function(extra_stage, &[Value::Table(song), steps])?;
        set(manager, "GetExtraStageInfo", callback, call)?;
        for name in [
            "GetSongColor",
            "GetSongGroupColor",
            "GetCourseColor",
            "GetSongRank",
            "ShortenGroupName",
        ] {
            let callback = call.new_host_function_with_data(simple_manager, &[], name)?;
            set(manager, name, callback, call)?;
        }
        let group = object(call, |call, group| {
            method(call, group, "GetSyncOffset", 0.0_f32)
        })?;
        method(call, manager, "GetGroup", group)?;
        for (name, value) in [
            ("GetNumSongs", 1_i32),
            ("GetNumLockedSongs", 0),
            ("GetNumUnlockedSongs", 1),
            ("GetNumSelectableAndUnlockedSongs", 1),
            ("GetNumAdditionalSongs", 0),
            ("GetNumSongGroups", 1),
            ("GetNumCourses", 1),
            ("GetNumAdditionalCourses", 0),
            ("GetNumCourseGroups", 1),
        ] {
            method(call, manager, name, value)?;
        }
        for name in [
            "WasLoadedFromAdditionalSongs",
            "WasLoadedFromAdditionalCourses",
        ] {
            method(call, manager, name, false)?;
        }
        for name in ["SetPreferredSongs", "SetPreferredCourses"] {
            let callback = call.new_host_function(
                set_preferred,
                &[
                    call.captures()[0],
                    call.captures()[1],
                    Value::Table(manager),
                ],
            )?;
            set(manager, name, callback, call)?;
        }
        Ok(())
    })
}
fn lookup(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call
        .function_data::<Lookup>()
        .expect("catalog lookup")
        .clone();
    let query = text(call, arg(call, 0));
    let matches_group = query.as_deref().unwrap_or_default() == data.group;
    let value = match data.name {
        "FindSong" | "FindCourse" => {
            let found = query.is_some_and(|query| {
                crate::song_lookup_matches(&query, &data.dir, &data.group, &data.title)
                    || data.name == "FindCourse"
                        && crate::song_lookup_matches(
                            &query,
                            &data.course_dir,
                            &data.group,
                            &data.title,
                        )
            });
            if found {
                call.captures()[0]
            } else {
                Value::Nil
            }
        }
        "DoesSongGroupExist" | "DoesCourseGroupExist" => {
            Value::Boolean(query.is_some_and(|q| q == data.group))
        }
        "GetSongGroupBannerPath" | "GetCourseGroupBannerPath" => {
            call.try_intern(if matches_group {
                data.banner.as_bytes()
            } else {
                b""
            })?
        }
        "SongToPreferredSortSectionName" => {
            call.try_intern(if matches!(arg(call, 0), Value::Table(_)) {
                data.group.as_bytes()
            } else {
                b""
            })?
        }
        _ => {
            if matches_group {
                call.captures()[0]
            } else {
                Value::Table(call.try_new_table()?)
            }
        }
    };
    call.push(value)
}
fn simple_manager(call: &mut HostCall<'_>) -> Result<(), Error> {
    let name = *call
        .function_data::<&'static str>()
        .expect("catalog method");
    let value = match name {
        "GetSongRank" => {
            if matches!(arg(call, 0), Value::Table(_)) {
                Value::Number(1.0)
            } else {
                Value::Nil
            }
        }
        "ShortenGroupName" => {
            call.try_intern(text(call, arg(call, 0)).unwrap_or_default().as_bytes())?
        }
        _ => Value::Table(array(call, [1.0_f32; 4])?),
    };
    call.push(value)
}
fn extra_stage(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.push(call.captures()[0])?;
    call.push(call.captures()[1])
}
fn set_preferred(call: &mut HostCall<'_>) -> Result<(), Error> {
    super::side_effect(call, call.captures()[0], call.captures()[1])?;
    call.push(call.captures()[2])
}
