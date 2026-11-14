//! Read the retained Current owners without executing authored getters or
//! advancing their approach clocks a second time.
use super::*;
use deadsync_gameplay::{AppearanceEffects, SongLuaPlayerOptions};

impl SongRuntime {
    pub(crate) fn gameplay_player_options(
        &mut self,
        appearance: [AppearanceEffects; LUA_PLAYERS],
    ) -> Result<[SongLuaPlayerOptions; LUA_PLAYERS], Error> {
        let catalog = self.vm.rooted(self.catalog)?;
        self.vm.with_native_stack(|stack| {
            let base = stack.top();
            stack.push(catalog)?;
            read(stack, stack.top(), "player_options", true)?;
            let players = stack.top();
            let mut output = appearance.map(|appearance| SongLuaPlayerOptions {
                appearance,
                ..Default::default()
            });
            for (player, options) in output.iter_mut().enumerate() {
                index(stack, players, player + 1)?;
                // ModsGroup's fourth owner is Current; the global compiler
                // aliases refer to Song and would bypass native approach.
                index(stack, stack.top(), 4)?;
                let current = stack.top();
                require_table(stack, current)?;
                read(stack, current, "__songlua_player_option_state", true)?;
                if stack.value(-1) != Some(Value::Nil) {
                    let state = stack.top();
                    require_table(stack, state)?;
                    stack.push(Value::Nil)?;
                    while stack.next_table(state as isize)? {
                        let value = match stack.value(-1) {
                            Some(Value::Boolean(value)) => Some(f32::from(value)),
                            Some(Value::Number(value)) => Some(value as f32),
                            Some(Value::String(_)) => stack.to_number(-1).map(|n| n as f32),
                            _ => None,
                        };
                        if let Some(value) = value {
                            // Coerce a copy, preserving the original traversal key.
                            stack.push(stack.value(-2).unwrap())?;
                            if let Some(name) = stack.to_bytes(-1)?
                                && let Ok(name) = std::str::from_utf8(name)
                            {
                                options.set_current(name, value);
                            }
                            stack.set_top(stack.top() - 1)?;
                        }
                        stack.set_top(stack.top() - 1)?;
                    }
                }
                stack.set_top(current)?;
                // Speed fields coexist on the owner. Native mode selection uses
                // Current's approached spacing and maximum BPM, not their presence.
                let spacing = current_number(stack, current, "__songlua_time_spacing", 0.0)?;
                let max = current_number(stack, current, "__songlua_speedmod_mmod", 0.0)?;
                let (key, value) = if spacing != 0.0 {
                    (
                        "cmod",
                        current_number(stack, current, "__songlua_speedmod_cmod", 200.0)?,
                    )
                } else if max != 0.0 {
                    ("mmod", max)
                } else {
                    (
                        "xmod",
                        current_number(stack, current, "__songlua_speedmod_xmod", 1.0)?,
                    )
                };
                options.set_current(key, value);
                stack.set_top(players)?;
            }
            stack.set_top(base)?;
            Ok(output)
        })
    }
}

fn current_number(
    stack: &mut NativeStack<'_, '_>,
    owner: usize,
    key: &'static str,
    default: f32,
) -> Result<f32, Error> {
    read(stack, owner, key, true)?;
    let value = if stack.value(-1) == Some(Value::Nil) {
        default
    } else {
        stack.number(-1)? as f32
    };
    stack.set_top(stack.top() - 1)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::scroll::ScrollSpeedSetting;

    fn runtime() -> SongRuntime {
        let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "current options");
        context.live = true;
        context.players[0].speedmod = SongLuaSpeedMod::X(1.0);
        SongRuntime::new(&context).unwrap()
    }

    fn execute(runtime: &mut SongRuntime, source: &str) {
        let code =
            deadlib_lua::compile_named(source.as_bytes(), b"=current option publication").unwrap();
        runtime.vm.execute(&code).unwrap();
    }

    #[test]
    fn gameplay_player_options_reads_current_without_running_getters_or_approaching() {
        let mut runtime = runtime();
        execute(
            &mut runtime,
            r#"
            local ps = GAMESTATE:GetPlayerState(PLAYER_1)
            local current = ps:GetPlayerOptions('ModsLevel_Current')
            current:Flip(-0.1); current:Brake(0.2); current:Stealth(0.4)
            current:Dark2(0.75); current:DizzyHolds(true); current:CMod(450)
            ps:GetPlayerOptions('ModsLevel_Song'):Flip(0.9, 1)
            current.Flip = function() error('publication called an authored getter') end
            ps.GetPlayerOptions = function() error('publication called an authored selector') end
            collectgarbage('collect')
        "#,
        );
        let appearance = [AppearanceEffects {
            hidden: 0.6,
            ..Default::default()
        }; LUA_PLAYERS];
        let options = runtime.gameplay_player_options(appearance).unwrap();
        assert_eq!(options[0].visual.flip, Some(-0.1));
        assert_eq!(options[0].accel.brake, Some(0.2));
        assert_eq!(options[0].appearance.stealth, 0.4);
        assert_eq!(options[0].appearance.hidden, 0.6);
        assert_eq!(options[0].visibility.dark_cols[1], Some(0.75));
        assert_eq!(options[0].visual.dizzy_holds, Some(true));
        assert_eq!(
            options[0].scroll_speed,
            Some(ScrollSpeedSetting::CMod(450.0))
        );
        assert_eq!(options[1].visual.flip, None);
        assert_eq!(
            runtime.gameplay_player_options(appearance).unwrap(),
            options
        );
    }

    #[test]
    fn gameplay_player_options_keeps_speed_mode_transitions_and_zero_resets() {
        let mut runtime = runtime();
        execute(
            &mut runtime,
            r#"
            local ps = GAMESTATE:GetPlayerState(PLAYER_1)
            ps:GetPlayerOptions('ModsLevel_Current'):XMod(2)
            local song = ps:GetPlayerOptions('ModsLevel_Song')
            song:CMod(300, 1); song:Stealth(0.5, 1)
        "#,
        );
        runtime.advance_options(0.1, None).unwrap();
        let read = |runtime: &mut SongRuntime| {
            runtime
                .gameplay_player_options([AppearanceEffects::default(); LUA_PLAYERS])
                .unwrap()[0]
        };
        let first = read(&mut runtime);
        assert_eq!(first.scroll_speed, Some(ScrollSpeedSetting::CMod(215.0)));
        assert!((first.appearance.stealth - 0.1).abs() < 0.00001);
        execute(
            &mut runtime,
            r#"
            local song = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
            song:XMod(1.5, 10000); song:Stealth(0, 10000)
        "#,
        );
        runtime.advance_options(0.2, None).unwrap();
        let reset = read(&mut runtime);
        assert_eq!(reset.scroll_speed, Some(ScrollSpeedSetting::XMod(1.5)));
        assert_eq!(reset.appearance.stealth, 0.0);
        execute(
            &mut runtime,
            r#"
            GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Current'):MMod(500)
        "#,
        );
        assert_eq!(
            read(&mut runtime).scroll_speed,
            Some(ScrollSpeedSetting::MMod(500.0))
        );
    }
}
