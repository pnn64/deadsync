//! Generated ScreenGameplay display children, before screen ownership/On commands.
use super::*;
use generated_players::dummy;
use sprites::{field, set};

#[cfg(feature = "test-support")]
struct Build {
    name: String,
}

const LYRIC_COMMANDS: &[u8] = br#"return function()
return {
    Back = function(self) self:LyricCommand('Back') end,
    Front = function(self) self:LyricCommand('Front') end,
    SetNoReverseCommand = function(self) self:x(SCREEN_CENTER_X):y(SCREEN_CENTER_Y + 160) end,
    SetReverseCommand = function(self) self:x(SCREEN_CENTER_X):y(SCREEN_CENTER_Y - 140) end,
    SetOneReverseCommand = function(self) self:x(SCREEN_CENTER_X):y(SCREEN_CENTER_Y - 205) end,
}
end"#;
const LYRIC_INITIAL: &[u8] = br#"return function()
local allReverse, anyReverse = true, false
for _, player in ipairs(GAMESTATE:GetEnabledPlayers()) do
    local reverse = GAMESTATE:GetPlayerState(player):GetPlayerOptions('ModsLevel_Current'):Reverse() == 1
    allReverse = allReverse and reverse
    anyReverse = anyReverse or reverse
end
return allReverse and 'SetReverse' or anyReverse and 'SetOneReverse' or 'SetNoReverse'
end"#;

impl Actors {
    #[cfg(feature = "test-support")]
    pub(crate) fn screen_part(&self, vm: &mut Vm, name: &str) -> Result<ActorScene, Error> {
        ensure_helpers(vm, self.helpers)?;
        let function = vm.new_host_function_with_data(
            construct,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            Build {
                name: name.to_owned(),
            },
        )?;
        let Value::Number(id) = vm.call(function, &[])?[0] else {
            unreachable!()
        };
        Ok(ActorScene {
            owner: self.instantiate,
            id: id as u64,
        })
    }
}

pub(super) fn ensure_helpers(vm: &mut Vm, helpers: TableId) -> Result<(), Error> {
    steps_displays::ensure_helpers(vm, helpers)?;
    transitions::ensure_helpers(vm, helpers)?;
    for (key, source, name) in [
        (
            "lyric_commands",
            LYRIC_COMMANDS,
            b"@theme:/metrics.ini:LyricDisplay".as_slice(),
        ),
        (
            "lyric_initial",
            LYRIC_INITIAL,
            b"@theme:/ScreenGameplay:LyricDisplayPosition".as_slice(),
        ),
    ] {
        let value = vm.try_intern(key.as_bytes())?;
        if vm.raw_table_get(helpers, value)? == Value::Nil {
            let code = compile_named(source, name)?;
            let function = vm.execute(&code)?[0];
            super::super::put(vm, helpers, key, function)?;
        }
    }
    Ok(())
}

#[cfg(feature = "test-support")]
fn construct(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Build>().unwrap();
    let name = data.name.clone();
    let id = {
        let guard = call.store();
        if guard.next >= (1u64 << 53) - 1 {
            return Err(call.error(b"actor scene identity exhausted"));
        }
        guard.next += 1;
        let id = guard.next;
        guard.scenes.insert(
            id,
            Scene {
                actors: Vec::new(),
                root: None,
                ended: false,
                sounds_retired: false,
                initial_states: None,
                song_session: None,
            },
        );
        id
    };
    let previous = {
        let guard = call.store();
        let capture = !guard.clock.live.load(std::sync::atomic::Ordering::Relaxed);
        std::mem::replace(&mut guard.constructing_capture, capture)
    };
    let result = create(call, id, &name);
    call.store().constructing_capture = previous;
    match result {
        Ok(actor) => call.store().scenes.get_mut(&id).unwrap().root = Some(actor),
        Err(error) => {
            release(call, id)?;
            return Err(error);
        }
    }
    call.push(Value::Number(id as f64))
}

pub(super) fn create(call: &mut HostCall<'_>, scene: u64, name: &str) -> Result<TableId, Error> {
    if ["In", "Out", "Cancel"]
        .iter()
        .any(|known| name.eq_ignore_ascii_case(known))
    {
        return transitions::create(call, scene, name);
    }
    if name.eq_ignore_ascii_case("Timer") {
        let actor = dummy(call, scene, "Timer", name, None)?;
        let function = call.new_host_function(timer_seconds, &[])?;
        let bound = get(call, actor, b"__songlua_bound_methods")?;
        set(call, table(call, bound)?, "GetSeconds", function)?;
        return Ok(actor);
    }
    if let Some(index) = crate::top_screen_score_index(name) {
        return score(call, scene, index);
    }
    if crate::top_screen_steps_display_index(name).is_some() {
        return steps_displays::create(call, scene, name);
    }
    if name.eq_ignore_ascii_case("LyricDisplay") {
        return lyrics(call, scene, name);
    }
    if name.eq_ignore_ascii_case("SongBackground") {
        return background(call, scene);
    }
    if name.eq_ignore_ascii_case("Debug") {
        let actor = text(call, scene, name, "")?;
        let globals = table(call, helper(call, 5)?)?;
        let center = field(call, globals, "SCREEN_CENTER_X")?;
        let center = number(call, center)?;
        let bottom = field(call, globals, "SCREEN_BOTTOM")?;
        let bottom = number(call, bottom)?;
        for (method, value) in [("x", center), ("y", bottom - 116.), ("zoom", 0.75)] {
            invoke(call, actor, method, &[Value::Number(value as f64)])?;
        }
        return Ok(actor);
    }
    if ["Overlay", "Header", "Footer", "SongForeground"]
        .iter()
        .any(|part| name.eq_ignore_ascii_case(part))
    {
        return dummy(call, scene, "ActorFrame", name, None);
    }
    if ["StepsDescription", "PlayerOptions", "ActiveAttackList"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || name == "SongOptions"
    {
        let actor = text(call, scene, name, "")?;
        invoke(call, actor, "visible", &[Value::Boolean(false)])?;
        return Ok(actor);
    }
    if name.starts_with("OniGameOver") {
        let actor = dummy(call, scene, "Sprite", name, None)?;
        blank_texture(call, actor)?;
        return Ok(actor);
    }
    Err(call.error(b"screen part is not implemented"))
}

fn timer_seconds(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.push(Value::Number(0.))
}

pub(super) fn text(
    call: &mut HostCall<'_>,
    scene: u64,
    name: &str,
    value: &str,
) -> Result<TableId, Error> {
    let actor = dummy(call, scene, "BitmapText", name, None)?;
    let value = call.try_intern(value.as_bytes())?;
    set(call, actor, "Text", value)?;
    Ok(actor)
}

pub(super) fn blank_texture(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let path =
        crate::blank_texture::load_native_blank_texture().map_err(|e| call.error(e.as_bytes()))?;
    let path = call.try_intern(path.as_bytes())?;
    sprites::set_texture(call, actor, path, false)
}

pub(super) fn append(
    call: &mut HostCall<'_>,
    parent: TableId,
    child: TableId,
) -> Result<(), Error> {
    native_children::adopt(call, child, parent)?;
    call.raw_table_set(
        parent,
        Value::Number((call.raw_table_len(parent)? + 1) as f64),
        Value::Table(child),
    )?;
    call.store()
        .actors
        .get_mut(&parent)
        .unwrap()
        .children
        .push(child);
    Ok(())
}

pub(super) fn named(
    call: &mut HostCall<'_>,
    parent: TableId,
    name: &str,
    child: TableId,
) -> Result<(), Error> {
    native_children::adopt(call, child, parent)?;
    native_children::insert(call, parent, name, Value::Table(child))
}

fn score(call: &mut HostCall<'_>, scene: u64, index: usize) -> Result<TableId, Error> {
    let actor = dummy(
        call,
        scene,
        "ActorFrame",
        crate::top_screen_score_name(index),
        None,
    )?;
    let frame = dummy(call, scene, "Sprite", "", None)?;
    blank_texture(call, frame)?;
    named(call, actor, "", frame)?;
    let percent = dummy(
        call,
        scene,
        "PercentageDisplay",
        "ScoreDisplayPercentage Percent",
        None,
    )?;
    let name = crate::top_screen_score_percent_name(index);
    let text = text(call, scene, name, "0.00%")?;
    named(call, percent, name, text)?;
    named(call, actor, "ScoreDisplayPercentage Percent", percent)?;
    Ok(actor)
}

pub(super) fn invoke(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &str,
    args: &[Value],
) -> Result<(), Error> {
    let function = field(call, actor, name)?;
    if !matches!(function, Value::Function(_) | Value::Native(_)) {
        return Err(call.error(b"actor function expected"));
    }
    let mut values = Vec::with_capacity(args.len() + 1);
    values.push(Value::Table(actor));
    values.extend_from_slice(args);
    call.call(function, &values)?;
    Ok(())
}

fn number(call: &mut HostCall<'_>, value: Value) -> Result<f32, Error> {
    sprites::number(call, value).map(|v| v as f32)
}

fn lyrics(call: &mut HostCall<'_>, scene: u64, name: &str) -> Result<TableId, Error> {
    let actor = dummy(call, scene, "ActorFrame", name, None)?;
    let factory = native_children::helper_function(call, b"lyric_commands")?;
    let commands = call.call(factory, &[])?[0];
    call.with_roots(&[commands], |call| {
        let commands = table(call, commands)?;
        for (name, side) in [("LyricBack", "Back"), ("LyricFront", "Front")] {
            let child = text(call, scene, name, "")?;
            let font = call.try_intern(b"Common Normal")?;
            set(call, child, "Font", font)?;
            let changed = get(call, commands, side.as_bytes())?;
            set(call, child, "ChangedCommand", changed)?;
            append(call, actor, child)?;
            named(call, actor, name, child)?;
        }
        for name in [
            "SetNoReverseCommand",
            "SetReverseCommand",
            "SetOneReverseCommand",
        ] {
            let function = get(call, commands, name.as_bytes())?;
            set(call, actor, name, function)?;
        }
        let initial = native_children::helper_function(call, b"lyric_initial")?;
        let initial = call.call(initial, &[])?[0];
        let initial = assets::text(call, initial)?;
        let function = get(call, commands, format!("{initial}Command").as_bytes())?;
        call.call(function, &[Value::Table(actor)])?;
        Ok(actor)
    })
}

fn background(call: &mut HostCall<'_>, scene: u64) -> Result<TableId, Error> {
    let actor = dummy(call, scene, "ActorFrame", "SongBackground", None)?;
    let implementation = dummy(call, scene, "ActorFrame", "", None)?;
    append(call, actor, implementation)?;
    let globals = table(call, helper(call, 5)?)?;
    let width = field(call, globals, "SCREEN_WIDTH")?;
    let width = number(call, width)?;
    let height = field(call, globals, "SCREEN_HEIGHT")?;
    let height = number(call, height)?;
    for (left, top, right, bottom) in [
        (0., 0., 0., height),
        (0., 0., width, 0.),
        (width, 0., width, height),
        (0., height, width, height),
    ] {
        let quad = dummy(call, scene, "Quad", "", Some(implementation))?;
        invoke(
            call,
            quad,
            "stretchto",
            &[left, top, right, bottom].map(|n| Value::Number(n as f64)),
        )?;
        invoke(call, quad, "diffuse", &[0., 0., 0., 1.].map(Value::Number))?;
        append(call, implementation, quad)?;
    }
    let brightness = dummy(call, scene, "ActorFrame", "", Some(implementation))?;
    append(call, implementation, brightness)?;
    let side = width / 2. - 20.;
    let prefs = get(call, globals, b"PREFSMAN")?;
    let level = call.with_roots(&[prefs], |call| {
        let function = field(call, table(call, prefs)?, "GetPreference")?;
        call.with_roots(&[function], |call| {
            if !matches!(function, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"preference function expected"));
            }
            let key = call.try_intern(b"BGBrightness")?;
            let value = call
                .call(function, &[prefs, key])?
                .first()
                .copied()
                .unwrap_or(Value::Nil);
            number(call, value)
        })
    })?;
    for (left, right) in [(0., side), (width - side, width), (side, width - side)] {
        let quad = dummy(call, scene, "Quad", "BrightnessOverlay", Some(brightness))?;
        invoke(
            call,
            quad,
            "stretchto",
            &[left, 0., right, height].map(|n| Value::Number(n as f64)),
        )?;
        invoke(
            call,
            quad,
            "diffuse",
            &[0., 0., 0., (1. - level) as f64].map(Value::Number),
        )?;
        append(call, brightness, quad)?;
    }
    // BackgroundImpl enables owned lookups recursively on its frame nodes.
    for frame in [actor, implementation, brightness] {
        set(
            call,
            frame,
            "__songlua_owned_child_lookup",
            Value::Boolean(true),
        )?;
    }
    Ok(actor)
}
