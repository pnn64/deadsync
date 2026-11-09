//! The production profile policy: session-owned mutable fields and default scores.
use super::catalog::{arg, float, text};
use super::fields::{object, set};
use super::scores::{empty, method};
use super::*;

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let captures = [vm.get_global(b"_G")?, vm.get_global(b"tonumber")?];
    let function = vm.new_host_function(initialize, &captures)?;
    vm.call(function, &[]).map(|_| ())
}

fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let manager = object(call, |call, owner| {
        set(owner, "__songlua_stats_prefix", "", call)?;
        // Publish each object to its eventual callback before making the next.
        let machine = profile(call, "Machine")?;
        method(call, owner, "GetMachineProfile", machine)?;
        call.with_roots(&[Value::Table(machine)], |call| {
            let profiles = object(call, |call, profiles| {
                for (index, name) in ["Player 1", "Player 2", "Player"].into_iter().enumerate() {
                    let value = profile(call, name)?;
                    set(profiles, index + 1, value, call)?;
                }
                Ok(())
            })?;
            let callback = call.new_host_function(
                get_profile,
                &[Value::Table(profiles), Value::Table(machine)],
            )?;
            set(owner, "GetProfile", callback, call)
        })?;
        let local = profile(call, "Local Profile")?;
        method(call, owner, "GetLocalProfileFromIndex", local)?;
        for (name, op) in [
            ("GetProfileDir", Lookup::Directory),
            ("GetPlayerName", Lookup::Name),
            ("LocalProfileIDToDir", Lookup::LocalDirectory),
        ] {
            let callback = call.new_host_function_with_data(lookup, &[], op)?;
            set(owner, name, callback, call)?;
        }
        for name in [
            "IsPersistentProfile",
            "IsSongNew",
            "ProfileWasLoadedFromMemoryCard",
            "LastLoadWasTamperedOrCorrupt",
            "ProfileFromMemoryCardIsNew",
        ] {
            method(call, owner, name, false)?;
        }
        for name in ["GetNumLocalProfiles", "GetSongNumTimesPlayed"] {
            method(call, owner, name, 0_i32)?;
        }
        method(call, owner, "GetLocalProfileIndexFromID", -1_i32)?;
        method(call, owner, "GetLocalProfileIDFromIndex", "")?;
        method(call, owner, "GetLocalProfile", Value::Nil)?;
        for name in ["GetLocalProfileIDs", "GetLocalProfileDisplayNames"] {
            let callback = call.new_host_function(empty, &[])?;
            set(owner, name, callback, call)?;
        }
        effect(call, owner, "SaveMachineProfile", None)?;
        for name in ["SaveProfile", "SaveLocalProfile"] {
            effect(call, owner, name, Some(Value::Boolean(false)))?;
        }
        bound(
            call,
            owner,
            "GetStatsPrefix",
            Op::Get("__songlua_stats_prefix", Kind::String),
        )?;
        bound(
            call,
            owner,
            "SetStatsPrefix",
            Op::Set("__songlua_stats_prefix", Kind::String),
        )
    })?;
    let globals = table(call, call.captures()[0])?;
    set(globals, "PROFILEMAN", manager, call)
}

fn slot(call: &HostCall<'_>, value: Value) -> Option<usize> {
    if let Some(player) = super::game_state::player(call, value) {
        return Some(player);
    }
    if value == Value::Number(2.) {
        return Some(2);
    }
    match text(call, value)?.as_str() {
        "ProfileSlot_Player1" => Some(0),
        "ProfileSlot_Player2" => Some(1),
        "ProfileSlot_Machine" => Some(2),
        _ => None,
    }
}
fn get_profile(call: &mut HostCall<'_>) -> Result<(), Error> {
    let index = slot(call, arg(call, 0));
    let value = if index == Some(2) {
        call.captures()[1]
    } else {
        let profiles = table(call, call.captures()[0])?;
        call.raw_table_get(profiles, Value::Number((index.unwrap_or(2) + 1) as f64))?
    };
    call.push(value)
}
#[derive(Clone, Copy)]
enum Lookup {
    Directory,
    Name,
    LocalDirectory,
}
fn lookup(call: &mut HostCall<'_>) -> Result<(), Error> {
    let result = match call.function_data::<Lookup>().unwrap() {
        Lookup::Directory => if slot(call, arg(call, 0)) == Some(2) {
            "/Save/MachineProfile/"
        } else {
            ""
        }
        .to_owned(),
        Lookup::Name => super::game_state::player(call, arg(call, 0))
            .map(|p| format!("Player {}", p + 1))
            .unwrap_or_default(),
        Lookup::LocalDirectory => format!(
            "/Save/LocalProfiles/{}/",
            text(call, arg(call, 0)).unwrap_or_default()
        ),
    };
    let result = call.try_intern(result.as_bytes())?;
    call.push(result)
}

#[derive(Clone, Copy)]
enum Kind {
    String,
    Number,
    Bool,
    Value,
    Table,
    Goal,
}
#[derive(Clone, Copy)]
enum Op {
    Get(&'static str, Kind),
    Set(&'static str, Kind),
    Names,
    CaloriesText,
    AddCalories,
}

fn bound(call: &mut HostCall<'_>, owner: TableId, name: &str, op: Op) -> Result<(), Error> {
    let captures = [Value::Table(owner), call.captures()[0], call.captures()[1]];
    let callback = call.new_host_function_with_data(state, &captures, op)?;
    set(owner, name, callback, call)
}
fn effect(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    result: Option<Value>,
) -> Result<(), Error> {
    let mut captures = vec![call.captures()[0], call.captures()[1]];
    captures.extend(result);
    let callback = call.new_host_function(side_effect, &captures)?;
    set(owner, name, callback, call)
}
fn side_effect(call: &mut HostCall<'_>) -> Result<(), Error> {
    super::side_effect(call, call.captures()[0], call.captures()[1])?;
    if let Some(&result) = call.captures().get(2) {
        call.push(result)?;
    }
    Ok(())
}

fn profile(call: &mut HostCall<'_>, name: &str) -> Result<TableId, Error> {
    object(call, |call, owner| {
        for (key, value, getter, setter) in [
            (
                "__songlua_display_name",
                name,
                "GetDisplayName",
                Some("SetDisplayName"),
            ),
            (
                "__songlua_last_score_name",
                name,
                "GetLastUsedHighScoreName",
                Some("SetLastUsedHighScoreName"),
            ),
            ("__songlua_guid", "", "GetGUID", None),
            ("__songlua_profile_dir", "", "GetProfileDir", None),
        ] {
            set(owner, key, value, call)?;
            bound(call, owner, getter, Op::Get(key, Kind::String))?;
            if let Some(setter) = setter {
                bound(call, owner, setter, Op::Set(key, Kind::String))?;
            }
        }
        for (key, getter, setter) in [
            (
                "__songlua_weight_pounds",
                "GetWeightPounds",
                Some("SetWeightPounds"),
            ),
            ("__songlua_voomax", "GetVoomax", Some("SetVoomax")),
            ("__songlua_birth_year", "GetBirthYear", Some("SetBirthYear")),
            (
                "__songlua_goal_calories",
                "GetGoalCalories",
                Some("SetGoalCalories"),
            ),
            (
                "__songlua_goal_seconds",
                "GetGoalSeconds",
                Some("SetGoalSeconds"),
            ),
            ("__songlua_calories_today", "GetCaloriesBurnedToday", None),
            ("__songlua_total_calories", "GetTotalCaloriesBurned", None),
        ] {
            set(owner, key, 0_f32, call)?;
            bound(call, owner, getter, Op::Get(key, Kind::Number))?;
            if let Some(setter) = setter {
                bound(call, owner, setter, Op::Set(key, Kind::Number))?;
            }
        }
        for (key, value, getter, setter) in [
            (
                "__songlua_ignore_step_count_calories",
                false,
                "GetIgnoreStepCountCalories",
                "SetIgnoreStepCountCalories",
            ),
            ("__songlua_is_male", true, "GetIsMale", "SetIsMale"),
        ] {
            set(owner, key, value, call)?;
            bound(call, owner, getter, Op::Get(key, Kind::Bool))?;
            bound(call, owner, setter, Op::Set(key, Kind::Bool))?;
        }
        set(owner, "__songlua_goal_type", 0_i32, call)?;
        bound(
            call,
            owner,
            "GetGoalType",
            Op::Get("__songlua_goal_type", Kind::Value),
        )?;
        bound(
            call,
            owner,
            "SetGoalType",
            Op::Set("__songlua_goal_type", Kind::Goal),
        )?;
        bound(
            call,
            owner,
            "GetCharacter",
            Op::Get("__songlua_character", Kind::Value),
        )?;
        bound(
            call,
            owner,
            "SetCharacter",
            Op::Set("__songlua_character", Kind::Value),
        )?;
        let users = call.try_new_table()?;
        set(owner, "__songlua_user_table", users, call)?;
        bound(
            call,
            owner,
            "GetUserTable",
            Op::Get("__songlua_user_table", Kind::Table),
        )?;
        bound(call, owner, "GetAllUsedHighScoreNames", Op::Names)?;
        bound(
            call,
            owner,
            "GetDisplayTotalCaloriesBurned",
            Op::CaloriesText,
        )?;
        bound(call, owner, "AddCaloriesToDailyTotal", Op::AddCalories)?;
        effect(call, owner, "AddScreenshot", Some(Value::Table(owner)))?;
        method(call, owner, "GetType", "ProfileType_Normal")?;
        for name in [
            "GetPriority",
            "GetAge",
            "GetTotalNumSongsPlayed",
            "GetNumTotalSongsPlayed",
            "GetTotalSessions",
            "GetTotalSessionSeconds",
            "GetTotalGameplaySeconds",
            "GetSongNumTimesPlayed",
            "GetNumToasties",
            "GetTotalTapsAndHolds",
            "GetTotalJumps",
            "GetTotalHolds",
            "GetTotalRolls",
            "GetTotalMines",
            "GetTotalHands",
            "GetTotalLifts",
            "GetTotalDancePoints",
            "GetTotalStepsWithTopGrade",
            "GetTotalTrailsWithTopGrade",
            "CalculateCaloriesFromHeartRate",
            "GetSongsActual",
            "GetCoursesActual",
            "GetSongsPossible",
            "GetCoursesPossible",
            "GetSongsPercentComplete",
            "GetCoursesPercentComplete",
            "GetSongsAndCoursesPercentCompleteAllDifficulties",
        ] {
            method(call, owner, name, 0_i32)?;
        }
        for name in ["IsCodeUnlocked", "HasPassedAnyStepsInSong"] {
            method(call, owner, name, false)?;
        }
        for name in [
            "GetMostPopularSong",
            "GetMostPopularCourse",
            "GetLastPlayedSong",
            "GetLastPlayedCourse",
        ] {
            method(call, owner, name, Value::Nil)?;
        }
        for name in [
            "GetHighScoreList",
            "GetHighScoreListIfExists",
            "GetCategoryHighScoreList",
        ] {
            let callback = call.new_host_function(super::scores::list, &[])?;
            set(owner, name, callback, call)?;
        }
        let callback = call.new_host_function(empty, &[])?;
        set(owner, "get_songs", callback, call)
    })
}

fn strict_text(call: &mut HostCall<'_>, value: Value) -> Result<Value, Error> {
    if value == Value::Nil {
        return call.try_intern(b"");
    }
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    std::str::from_utf8(call.string_bytes(id).unwrap())
        .map_err(|e| call.error(e.to_string().as_bytes()))?;
    Ok(Value::String(id))
}
fn get_float(call: &mut HostCall<'_>, owner: Value, key: &str) -> Result<f32, Error> {
    let value = super::super::field(call, owner, key)?;
    if value == Value::Nil {
        return Ok(0.);
    }
    Ok(super::song_options::number(call, value, call.captures()[2])? as f32)
}
fn boolish(call: &HostCall<'_>, value: Value) -> bool {
    match value {
        Value::Boolean(b) => b,
        Value::Number(n) => n != 0.,
        Value::String(_) => text(call, value).is_some_and(|s| {
            let s = s.trim();
            if ["true", "yes", "on"]
                .iter()
                .any(|v| s.eq_ignore_ascii_case(v))
            {
                true
            } else if ["false", "no", "off"]
                .iter()
                .any(|v| s.eq_ignore_ascii_case(v))
            {
                false
            } else {
                s.parse::<f32>().is_ok_and(|n| n != 0.)
            }
        }),
        _ => false,
    }
}
fn state(call: &mut HostCall<'_>) -> Result<(), Error> {
    let op = *call.function_data::<Op>().unwrap();
    let owner = call.captures()[0];
    let result = match op {
        Op::Get(key, kind) => {
            let value = super::super::field(call, owner, key)?;
            match kind {
                Kind::String => strict_text(call, value)?,
                Kind::Number => Value::Number(if value == Value::Nil {
                    0.
                } else {
                    super::song_options::number(call, value, call.captures()[2])? as f32 as f64
                }),
                Kind::Bool => Value::Boolean(!matches!(value, Value::Nil | Value::Boolean(false))),
                Kind::Table => {
                    table(call, value)?;
                    value
                }
                Kind::Value => value,
                Kind::Goal => unreachable!(),
            }
        }
        Op::Set(key, kind) => {
            let value = arg(call, 0);
            let value = match kind {
                Kind::String => {
                    if text(call, value).is_some() {
                        value
                    } else {
                        call.try_intern(b"")?
                    }
                }
                Kind::Number => Value::Number(float(call, value).unwrap_or(0.) as f64),
                Kind::Bool => Value::Boolean(boolish(call, value)),
                Kind::Goal => {
                    let offset =
                        usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
                    call.arguments()
                        .get(offset)
                        .copied()
                        .unwrap_or(Value::Number(0.))
                }
                Kind::Value => value,
                Kind::Table => unreachable!(),
            };
            super::super::set(call, owner, key, value)?;
            super::side_effect(call, call.captures()[1], call.captures()[2])?;
            owner
        }
        Op::Names => {
            let value = super::super::field(call, owner, "__songlua_last_score_name")?;
            let value = strict_text(call, value)?;
            let names = object(call, |call, names| {
                if text(call, value).is_some_and(|s| !s.is_empty()) {
                    set(names, 1_i32, value, call)?;
                }
                Ok(())
            })?;
            Value::Table(names)
        }
        Op::CaloriesText => {
            let calories = get_float(call, owner, "__songlua_total_calories")?.max(0.) as i64;
            call.try_intern(format!("{calories} Cal").as_bytes())?
        }
        Op::AddCalories => {
            let calories = float(call, arg(call, 0)).unwrap_or(0.);
            let today = get_float(call, owner, "__songlua_calories_today")? + calories;
            let total = get_float(call, owner, "__songlua_total_calories")? + calories;
            super::super::set(
                call,
                owner,
                "__songlua_calories_today",
                Value::Number(today as f64),
            )?;
            super::super::set(
                call,
                owner,
                "__songlua_total_calories",
                Value::Number(total as f64),
            )?;
            super::side_effect(call, call.captures()[1], call.captures()[2])?;
            owner
        }
    };
    call.push(result)
}
