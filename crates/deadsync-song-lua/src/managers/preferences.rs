//! Preference defaults and the production manager's mutable Lua value store.
use super::*;
use std::sync::Arc;

struct Defaults {
    offset: f32,
    aspect: f32,
    width: i32,
    height: i32,
    renderers: String,
    driver: String,
}

#[derive(Clone, Copy)]
enum Operation {
    Get,
    Set,
    Exists,
    Save,
    Reset,
}
struct Binding {
    defaults: Arc<Defaults>,
    operation: Operation,
}

pub(super) fn install(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    let manager = vm.try_new_table()?;
    let manager_root = vm.pin(Value::Table(manager))?;
    let store = vm.try_new_table()?;
    let store_root = vm.pin(Value::Table(store))?;
    let result = (|| {
        super::super::put(vm, store, "EventMode", Value::Boolean(context.event_mode))?;
        super::super::put(
            vm,
            manager,
            "__songlua_preference_values",
            Value::Table(store),
        )?;
        let defaults = Arc::new(Defaults {
            offset: context.global_offset_seconds,
            aspect: context.display_aspect_ratio,
            width: context.display_width.max(0.).round() as i32,
            height: context.display_height.max(0.).round() as i32,
            renderers: context.video_renderers.clone(),
            driver: context.last_seen_video_driver.clone(),
        });
        let globals = vm.get_global(b"_G")?;
        let tonumber = vm.get_global(b"tonumber")?;
        for (name, operation) in [
            ("GetPreference", Operation::Get),
            ("SetPreference", Operation::Set),
            ("PreferenceExists", Operation::Exists),
            ("SavePreferences", Operation::Save),
            ("SetPreferenceToDefault", Operation::Reset),
        ] {
            // A saved getter owns its store, while an existence query owns only
            // its defaults. Neither must retain the manager through a capture.
            let captures = [
                if matches!(
                    operation,
                    Operation::Set | Operation::Save | Operation::Reset
                ) {
                    Value::Table(manager)
                } else {
                    Value::Nil
                },
                if matches!(
                    operation,
                    Operation::Get | Operation::Set | Operation::Reset
                ) {
                    Value::Table(store)
                } else {
                    Value::Nil
                },
                globals,
                tonumber,
            ];
            let method = vm.new_host_function_with_data(
                callback,
                &captures,
                Binding {
                    defaults: defaults.clone(),
                    operation,
                },
            )?;
            let root = vm.pin(method)?;
            let result = super::super::put(vm, manager, name, method);
            vm.unpin(root)?;
            result?;
        }
        vm.set_global(b"PREFSMAN", Value::Table(manager))
    })();
    vm.unpin(store_root)?;
    vm.unpin(manager_root)?;
    result
}

fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().expect("preference binding");
    let operation = binding.operation;
    let defaults = binding.defaults.clone();
    let manager = call.captures()[0];
    let store = call.captures()[1];
    if matches!(
        operation,
        Operation::Set | Operation::Save | Operation::Reset
    ) {
        side_effect(call, call.captures()[2], call.captures()[3])?;
    }
    if matches!(operation, Operation::Save) {
        return call.push(manager);
    }
    let offset = usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
    let key = call.arguments().get(offset).copied().unwrap_or(Value::Nil);
    // These methods use read_string: numbers and invalid UTF-8 are ignored.
    let key = if let Value::String(id) = key {
        call.string_bytes(id)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::to_owned)
    } else {
        None
    };
    let Some(key) = key else {
        return call.push(match operation {
            Operation::Get => Value::Nil,
            Operation::Exists => Value::Boolean(false),
            _ => manager,
        });
    };
    let key_value = call.try_intern(key.as_bytes())?;
    match operation {
        Operation::Get => {
            let stored = call.table_get(store, key_value)?;
            let value = if stored == Value::Nil {
                default(call, &defaults, &key)?
            } else {
                stored
            };
            call.push(value)
        }
        Operation::Exists => {
            let exists = default(call, &defaults, &key)? != Value::Nil;
            call.push(Value::Boolean(exists))
        }
        Operation::Set => {
            if default(call, &defaults, &key)? != Value::Nil {
                let value = call
                    .arguments()
                    .get(offset + 1)
                    .copied()
                    .unwrap_or(Value::Nil);
                call.table_set(store, key_value, value)?;
            }
            call.push(manager)
        }
        Operation::Reset => {
            call.table_set(store, key_value, Value::Nil)?;
            call.push(manager)
        }
        Operation::Save => unreachable!(),
    }
}

fn default(call: &mut HostCall<'_>, defaults: &Defaults, key: &str) -> Result<Value, Error> {
    let lower = key.to_ascii_lowercase();
    let value = match lower.as_str() {
        "globaloffsetseconds" => Value::Number(defaults.offset as f64),
        "displayaspectratio" => Value::Number(defaults.aspect as f64),
        "displaywidth" => Value::Number(defaults.width as f64),
        "displayheight" => Value::Number(defaults.height as f64),
        "videorenderers" => return call.try_intern(defaults.renderers.as_bytes()),
        "lastseenvideodriver" => return call.try_intern(defaults.driver.as_bytes()),
        "eventmode" => Value::Boolean(true),
        "visualdelayseconds" => Value::Number(0.),
        "backgroundfitmode" => return call.try_intern(b"BackgroundFitMode_CoverPreserve"),
        "bgbrightness" => Value::Number(0.7),
        "timingwindowscale" => Value::Number(1.),
        "timingwindowadd" => Value::Number(0.0015),
        key if key.starts_with("timingwindowseconds") => Value::Number(0.),
        "autogengroupcourses"
        | "center1player"
        | "eastereggs"
        | "harshhotlifepenalty"
        | "memorycards"
        | "menutimer"
        | "onlydedicatedmenubuttons"
        | "ratemodsaffectfgchanges"
        | "showbanners"
        | "shownativelanguage"
        | "threekeynavigation" => Value::Boolean(false),
        "longversongseconds" => Value::Number(150.),
        "marathonversongseconds" => Value::Number(300.),
        "refreshrate" => Value::Number(60.),
        "maxhighscoresperlistfor" | "maxhighscoresperlistformachine" => Value::Number(10.),
        "songsperplay" | "soundvolume" => Value::Number(1.),
        "coinspercredit"
        | "customsongsloadtimeout"
        | "customsongsmaxmegabytes"
        | "customsongsmaxseconds"
        | "lifedifficultyscale"
        | "maxregencomboaftermiss"
        | "mintnstoscorenotes"
        | "musicwheelswitchspeed"
        | "regencomboaftermiss" => Value::Number(0.),
        "coinmode" => return call.try_intern(b"CoinMode_Free"),
        "displaymode" => return call.try_intern(b"Windowed"),
        "displayresolution" => return call.try_intern(b"1920x1080"),
        "editornoteskinp1" | "editornoteskinp2" => return call.try_intern(b"default"),
        "fullscreentype" => return call.try_intern(b"Borderless"),
        "premium" => return call.try_intern(b"Premium_Off"),
        "defaultmodifiers" | "httpallowhosts" => return call.try_intern(b""),
        "theme" => return call.try_intern(crate::SONG_LUA_THEME_NAME.as_bytes()),
        key if key.starts_with("defaultlocalprofileid") => return call.try_intern(b""),
        _ => Value::Nil,
    };
    Ok(value)
}
