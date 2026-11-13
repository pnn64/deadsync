//! ScreenGameplay transition owners and their existing loaded actors.
use super::*;
use generated_players::dummy;
use screen_parts::{append, invoke, text};
use sprites::field;

fn set(call: &mut HostCall<'_>, actor: TableId, name: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.raw_table_set(actor, key, value)
}

const IN_COMMANDS: &[u8] = br#"return function()

return {
    { InitCommand=function(self) self:diffuse(Color.Black):Center():FullScreen() end,
      OnCommand=function(self) self:sleep(1.4):accelerate(0.6):diffusealpha(0) end },
    { InitCommand=function(self) self:diffuse(GetCurrentColor(true)):Center():rotationz(10):zoom(0):diffusealpha(0.9) end,
      OnCommand=function(self) self:sleep(0.4):linear(0.6):rotationz(0):zoom(1.1):diffusealpha(0) end },
    { InitCommand=function(self) self:diffuse(GetCurrentColor(true)):Center():rotationy(180):rotationz(-10):zoom(0):diffusealpha(0.8) end,
      OnCommand=function(self) self:sleep(0.4):decelerate(0.6):rotationz(0):zoom(1.3):diffusealpha(0) end },
    { InitCommand=function(self) self:diffuse(GetCurrentColor(true)):Center():rotationz(10):zoom(0) end,
      OnCommand=function(self) self:sleep(0.4):decelerate(0.8):rotationz(0):zoom(0.9):diffusealpha(0) end },
}

end"#;
const STAGE_TEXT: &[u8] = br#"return function()

if GAMESTATE:IsCourseMode() then
    local entries = GAMESTATE:GetCurrentCourse():GetCourseEntries()
    return ("%s 1 / %d"):format(THEME:GetString("Stage", "Stage"), #entries)
elseif GAMESTATE:IsEventMode() then
    return THEME:GetString("Stage", "Event")
else
    return THEME:GetString("Stage", "Stage") .. " " .. tostring(GAMESTATE:GetCurrentStageIndex() + 1)
end

end"#;
const OUT_COMMANDS: &[u8] = br#"return function()

return {
	InitCommand=function(self) self:FullScreen():diffuse(0,0,0,0) end,
	OnCommand=function(self) self:sleep(0.5):linear(1):diffusealpha(1) end,
	OffCommand=function(self)
		if SL.Global.GameMode == "ITG" then
			local song = GAMESTATE:GetCurrentSong()
			local totalWhites = 0
			for player in ivalues( GAMESTATE:GetHumanPlayers() ) do
				local pn = ToEnumShortString(player)
				local pss = STATSMAN:GetCurStageStats():GetPlayerStageStats(player)
				local number = pss:GetTapNoteScores("TapNoteScore_W1")
				local faPlus = SL[pn].Stages.Stats[SL.Global.Stages.PlayedThisGame + 1].ex_counts.W0_total
				-- Subtract FA+ count from the overall fantastic window count.
				local whites = number - faPlus
				totalWhites = totalWhites + whites
				-- This will save the white count to Stats.xml, so we can later recover
				-- it when we deprecate FA+ mode and introduce W0.
				--
				-- The Score field is completely unused in Simply Love, and the ability
				-- to set the field is exposed to lua so we can hijack it for our own\
				-- purposes.
				local bestWhites = whites
				if PROFILEMAN:IsPersistentProfile(pn) then
					local steps = GAMESTATE:GetCurrentSteps(pn)
					local scores = PROFILEMAN:GetProfile(pn):GetHighScoreList(song, steps):GetHighScores()
					for hs in ivalues(scores) do
						-- If the player previously quadded the song, retain the better white count.
						-- Technically this is a workaround because the date would be wrong, but
						-- it's still worth to keep the score around
						if (pss:GetPercentDancePoints() == hs:GetPercentDP() and hs:GetPercentDP() == 1.0) then
							bestWhites = math.min(bestWhites, hs:GetScore())
						end
					end
				end
				pss:SetScore(bestWhites)
			end
			if IsRoutine() then
				local routineStats = STATSMAN:GetCurStageStats():GetRoutineStageStats()
				local bestWhites = totalWhites
				if PROFILEMAN:IsPersistentProfile(GAMESTATE:GetMasterPlayerNumber()) then
					local steps = GAMESTATE:GetCurrentSteps(GAMESTATE:GetMasterPlayerNumber())
					local scores = PROFILEMAN:GetProfile(GAMESTATE:GetMasterPlayerNumber()):GetHighScoreList(song, steps):GetHighScores()
					for hs in ivalues(scores) do
						-- If the player previously quadded the song, retain the better white count.
						-- Technically this is a workaround because the date would be wrong, but
						-- it's still worth to keep the score around
						if (routineStats:GetPercentDancePoints() == hs:GetPercentDP() and hs:GetPercentDP() == 1.0) then
							bestWhites = math.min(bestWhites, hs:GetScore())
						end
					end
				end
				routineStats:SetScore(bestWhites)
			end
		end
	end
}
end"#;

impl Actors {
    #[cfg(feature = "test-support")]
    pub(crate) fn start_transition(&self, vm: &mut Vm, actor: Value) -> Result<(), Error> {
        let function = vm.new_host_function(
            start_callback,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
        )?;
        vm.call(function, &[actor]).map(|_| ())
    }
}

pub(super) fn ensure_helpers(vm: &mut Vm, helpers: TableId) -> Result<(), Error> {
    for (key, source, name) in [
        (
            "transition_in_commands",
            IN_COMMANDS,
            "@theme:/BGAnimations/ScreenGameplay in/default.lua",
        ),
        (
            "transition_stage_text",
            STAGE_TEXT,
            "@theme:/BGAnimations/ScreenGameplay in/default.lua",
        ),
        (
            "transition_out_commands",
            OUT_COMMANDS,
            "@theme:/BGAnimations/ScreenGameplay out.lua",
        ),
    ] {
        let field = vm.try_intern(key.as_bytes())?;
        if vm.raw_table_get(helpers, field)? == Value::Nil {
            let code = compile_named(source, name.as_bytes())?;
            let function = vm.execute(&code)?[0];
            super::super::put(vm, helpers, key, function)?;
        }
    }
    Ok(())
}

pub(super) fn state(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<u8>, Error> {
    let value = get(call, actor, b"__songlua_transition_state")?;
    if value == Value::Nil {
        return Ok(None);
    }
    let number = sprites::number(call, value)?.trunc();
    if !(0.0..256.0).contains(&number) {
        return Err(call.error(b"transition state out of range"));
    }
    Ok(Some(number as u8))
}

pub(super) fn child(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<TableId>, Error> {
    let value = get(call, actor, b"__songlua_transition_child")?;
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}

pub(super) fn start(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    if state(call, actor)? != Some(0) {
        return Ok(());
    }
    if let Some(child) = child(call, actor)? {
        // The AutoActor survives replacement/removal of the raw child field by
        // its own callback. Start's state write follows the complete traversal.
        call.with_roots(&[Value::Table(child)], |call| start_commands(call, child))?;
        set(call, actor, "__songlua_transition_state", Value::Number(1.))?;
    }
    Ok(())
}

#[cfg(feature = "test-support")]
fn start_callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    start(call, table(call, arg(call, 0))?)
}

fn global_table(call: &mut HostCall<'_>, name: &str) -> Result<TableId, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let value = field(call, globals, name)?;
    table(call, value)
}
fn global_call(call: &mut HostCall<'_>, name: &str, args: &[Value]) -> Result<Value, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let function = field(call, globals, name)?;
    function_call(call, function, args)
}
fn function_call(call: &mut HostCall<'_>, function: Value, args: &[Value]) -> Result<Value, Error> {
    if !matches!(function, Value::Function(_) | Value::Native(_)) {
        return Err(call.error(b"function expected"));
    }
    Ok(call
        .call(function, args)?
        .first()
        .copied()
        .unwrap_or(Value::Nil))
}
fn reload(call: &mut HostCall<'_>, value: bool) -> Result<(), Error> {
    let sl = global_table(call, "SL")?;
    let global = field(call, sl, "Global")?;
    set(
        call,
        table(call, global)?,
        "GameplayReloadCheck",
        Value::Boolean(value),
    )
}
fn hide(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, arg(call, 0))?;
    invoke(call, actor, "visible", &[Value::Boolean(false)])?;
    reload(call, true)
}
fn off(call: &mut HostCall<'_>) -> Result<(), Error> {
    table(call, arg(call, 0))?;
    reload(call, false)
}
fn cancel(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, arg(call, 0))?;
    invoke(call, actor, "sleep", &[Value::Number(0.1)])?;
    invoke(call, actor, "linear", &[Value::Number(0.4)])?;
    invoke(call, actor, "diffusealpha", &[Value::Number(1.)])
}

fn attach(call: &mut HostCall<'_>, owner: TableId, child: TableId) -> Result<(), Error> {
    set(
        call,
        owner,
        "__songlua_transition_child",
        Value::Table(child),
    )?;
    append(call, owner, child)
}
fn commands(
    call: &mut HostCall<'_>,
    actor: TableId,
    handlers: TableId,
    names: &[&str],
) -> Result<(), Error> {
    for name in names {
        let function = get(call, handlers, name.as_bytes())?;
        if !matches!(function, Value::Function(_) | Value::Native(_)) {
            return Err(call.error(b"function expected"));
        }
        set(call, actor, name, function)?;
    }
    Ok(())
}

pub(super) fn create(call: &mut HostCall<'_>, scene: u64, name: &str) -> Result<TableId, Error> {
    let actor = dummy(call, scene, "ActorFrame", name, None)?;
    set(call, actor, "__songlua_transition_state", Value::Number(0.))?;
    if name.eq_ignore_ascii_case("In") {
        create_in(call, scene, actor)?;
    } else {
        let quad = dummy(call, scene, "Quad", "", Some(actor))?;
        if name.eq_ignore_ascii_case("Out") {
            let factory = native_children::helper_function(call, b"transition_out_commands")?;
            let handlers = call.call(factory, &[])?[0];
            call.with_roots(&[handlers], |call| {
                commands(
                    call,
                    quad,
                    table(call, handlers)?,
                    &["InitCommand", "OnCommand", "OffCommand"],
                )
            })?;
        }
        invoke(call, quad, "FullScreen", &[])?;
        invoke(call, quad, "diffuse", &[Value::Number(0.); 4])?;
        if name.eq_ignore_ascii_case("Out") {
            invoke(call, quad, "sleep", &[Value::Number(0.5)])?;
            invoke(call, quad, "linear", &[Value::Number(1.)])?;
            invoke(call, quad, "diffusealpha", &[Value::Number(1.)])?;
        } else {
            let function = call.new_host_function(cancel, &[])?;
            set(call, quad, "StartTransitioningCommand", function)?;
        }
        attach(call, actor, quad)?;
    }
    Ok(actor)
}

fn create_in(call: &mut HostCall<'_>, scene: u64, actor: TableId) -> Result<(), Error> {
    let child = dummy(call, scene, "ActorFrame", "", Some(actor))?;
    attach(call, actor, child)?;
    let visuals = dummy(call, scene, "ActorFrame", "", Some(child))?;
    append(call, child, visuals)?;
    let sl = global_table(call, "SL")?;
    let global = field(call, sl, "Global")?;
    let reload = field(call, table(call, global)?, "GameplayReloadCheck")?;
    let reload = !matches!(reload, Value::Nil | Value::Boolean(false));
    for (name, body) in [
        (
            "HideCommand",
            hide as fn(&mut HostCall<'_>) -> Result<(), Error>,
        ),
        ("OffCommand", off),
    ] {
        let function = call.new_host_function(body, &[call.captures()[0], call.captures()[1]])?;
        set(call, visuals, name, function)?;
    }
    if reload {
        let name = call.try_intern(b"Hide")?;
        return invoke(call, visuals, "playcommand", &[name]);
    }
    invoke(call, visuals, "sleep", &[Value::Number(2.)])?;
    let name = call.try_intern(b"Hide")?;
    invoke(call, visuals, "queuecommand", &[name])?;
    let factory = native_children::helper_function(call, b"transition_in_commands")?;
    let handlers = call.call(factory, &[])?[0];
    call.with_roots(&[handlers], |call| {
        let handlers = table(call, handlers)?;
        let prefs = global_table(call, "ThemePrefs")?;
        let getter = field(call, prefs, "Get")?;
        let key = call.try_intern(b"VisualStyle")?;
        let style = function_call(call, getter, &[key])?;
        let style = assets::text(call, style)?;
        let spooky = global_call(call, "IsSpooky", &[])?;
        let spooky = !matches!(spooky, Value::Nil | Value::Boolean(false));
        let color = global_call(call, "GetCurrentColor", &[Value::Boolean(true)])?;
        table(call, color)?;
        call.with_roots(&[color], |call| {
            for index in 0..4 {
                let sprite = dummy(
                    call,
                    scene,
                    if index == 0 { "Quad" } else { "Sprite" },
                    "",
                    Some(visuals),
                )?;
                let handler = call.raw_table_get(handlers, Value::Number((index + 1) as f64))?;
                commands(
                    call,
                    sprite,
                    table(call, handler)?,
                    &["InitCommand", "OnCommand"],
                )?;
                invoke(call, sprite, "Center", &[])?;
                if index == 0 {
                    invoke(call, sprite, "FullScreen", &[])?;
                    invoke(
                        call,
                        sprite,
                        "diffuse",
                        &[
                            Value::Number(0.),
                            Value::Number(0.),
                            Value::Number(0.),
                            Value::Number(1.),
                        ],
                    )?;
                    invoke(call, sprite, "sleep", &[Value::Number(1.4)])?;
                    invoke(call, sprite, "accelerate", &[Value::Number(0.6)])?;
                } else {
                    let asset = if spooky {
                        "_VisualStyles/Spooky/ExtraSpooky/Bats".into()
                    } else {
                        format!(
                            "_VisualStyles/{style}/GameplayIn {}",
                            if index == 3 { "minisplode" } else { "splode" }
                        )
                    };
                    let path =
                        gameplay_bitmaps::load(&asset).map_err(|e| call.error(e.as_bytes()))?;
                    let path = call.try_intern(path.as_bytes())?;
                    sprites::set_texture(call, sprite, path, false)?;
                    invoke(call, sprite, "diffuse", &[color])?;
                    invoke(
                        call,
                        sprite,
                        "rotationz",
                        &[Value::Number(if index == 2 { -10. } else { 10. })],
                    )?;
                    if index == 2 {
                        invoke(call, sprite, "rotationy", &[Value::Number(180.)])?;
                    }
                    invoke(call, sprite, "zoom", &[Value::Number(0.)])?;
                    if index != 3 {
                        invoke(
                            call,
                            sprite,
                            "diffusealpha",
                            &[Value::Number(if index == 1 { 0.9 } else { 0.8 })],
                        )?;
                    }
                    invoke(call, sprite, "sleep", &[Value::Number(0.4)])?;
                    invoke(
                        call,
                        sprite,
                        if index == 1 { "linear" } else { "decelerate" },
                        &[Value::Number(if index == 3 { 0.8 } else { 0.6 })],
                    )?;
                    invoke(call, sprite, "rotationz", &[Value::Number(0.)])?;
                    invoke(
                        call,
                        sprite,
                        "zoom",
                        &[Value::Number([0., 1.1, 1.3, 0.9][index])],
                    )?;
                }
                invoke(call, sprite, "diffusealpha", &[Value::Number(0.)])?;
                append(call, visuals, sprite)?;
            }
            Ok(())
        })
    })?;
    let factory = native_children::helper_function(call, b"transition_stage_text")?;
    let stage = call.call(factory, &[])?[0];
    let stage = assets::text(call, stage)?;
    let label = text(call, scene, "", &stage)?;
    native_children::adopt(call, label, child)?;
    invoke(call, label, "accelerate", &[Value::Number(0.5)])?;
    invoke(call, label, "diffusealpha", &[Value::Number(1.)])?;
    invoke(call, label, "sleep", &[Value::Number(0.66)])?;
    invoke(call, label, "accelerate", &[Value::Number(0.33)])?;
    append(call, child, label)
}

fn start_commands(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    native_children::run_command(call, actor, b"StartTransitioning", None)?;
    native_children::with_children(call, actor, |call, child| start_commands(call, child))
}
