//! Read the retained Current owners without executing authored getters or
//! advancing their approach clocks a second time.
use super::*;
use deadlib_lua::TableId;
use deadsync_gameplay::{AppearanceEffects, SongLuaPlayerOptions};

/// What a player's last snapshot read and produced. A later snapshot of the
/// same tables at the same versions, from the same profile appearance,
/// produces the same options. Debug builds walk anyway and compare.
#[derive(Clone, Copy)]
struct Snapshot {
    current: (TableId, u32),
    state: Option<(TableId, u32)>,
    appearance: AppearanceEffects,
    options: SongLuaPlayerOptions,
}
#[derive(Default)]
pub(crate) struct OptionSnapshots([Option<Snapshot>; LUA_PLAYERS]);

impl SongRuntime {
    /// Bring each player's snapshot up to date with Current, reading it again
    /// only where it may have changed. [`Self::gameplay_options`] lends the
    /// result.
    pub(crate) fn refresh_gameplay_options(
        &mut self,
        appearance: [AppearanceEffects; LUA_PLAYERS],
    ) -> Result<(), Error> {
        // Debug builds walk anyway and compare.
        let held = self.snapshots_hold(appearance)?;
        if held && !cfg!(debug_assertions) {
            return Ok(());
        }
        let catalog = self.vm.rooted(self.catalog)?;
        let snapshots = &mut self.option_snapshots;
        self.vm.with_native_stack(|stack| {
            let base = stack.top();
            stack.push(catalog)?;
            read(stack, stack.top(), "player_options", true)?;
            let players = stack.top();
            for player in 0..LUA_PLAYERS {
                index(stack, players, player + 1)?;
                // ModsGroup's fourth owner is Current; the global compiler
                // aliases refer to Song and would bypass native approach.
                index(stack, stack.top(), 4)?;
                let current = stack.top();
                require_table(stack, current)?;
                let Some(Value::Table(current_table)) = stack.value(-1) else {
                    unreachable!("Current owner is a table")
                };
                read(stack, current, "__songlua_player_option_state", true)?;
                let state = match stack.value(-1) {
                    Some(Value::Table(state)) => Some(state),
                    _ => None,
                };
                // Numeric keys name no modifier, so string-keyed versions
                // cover everything a walk reads.
                let current_key = (current_table, stack.raw_fields_version(current_table)?);
                let state_key = match state {
                    Some(state) => Some((state, stack.raw_fields_version(state)?)),
                    None => None,
                };
                let remembered = snapshots.0[player].as_ref().filter(|s| {
                    s.current == current_key
                        && s.state == state_key
                        && s.appearance == appearance[player]
                });
                match remembered {
                    Some(_) if !cfg!(debug_assertions) => {}
                    remembered => {
                        assert!(
                            remembered.is_some() || !held,
                            "raw reads trusted a snapshot that the stack path does not"
                        );
                        let walked = walk(stack, current, appearance[player])?;
                        if let Some(snapshot) = remembered {
                            assert_eq!(snapshot.options, walked, "stale Current option snapshot");
                        }
                        snapshots.0[player] = Some(Snapshot {
                            current: current_key,
                            state: state_key,
                            appearance: appearance[player],
                            options: walked,
                        });
                    }
                }
                stack.set_top(players)?;
            }
            stack.set_top(base)?;
            Ok(())
        })
    }

    /// Whether every player's snapshot still describes Current, by raw reads
    /// alone; `false` for anything unusual, which the stack path then reads.
    fn snapshots_hold(
        &mut self,
        appearance: [AppearanceEffects; LUA_PLAYERS],
    ) -> Result<bool, Error> {
        for (player, appearance) in appearance.into_iter().enumerate() {
            let Some(levels) = super::player_levels(&mut self.vm, self.catalog, player)? else {
                return Ok(false);
            };
            let Value::Table(current) = self.vm.raw_table_get(levels, Value::Number(4.))? else {
                return Ok(false);
            };
            let key = self.vm.intern_static(b"__songlua_player_option_state")?;
            let state = match self.vm.raw_table_get(current, key)? {
                Value::Table(state) => Some((state, self.vm.raw_fields_version(state)?)),
                _ => None,
            };
            let current = (current, self.vm.raw_fields_version(current)?);
            let held = self.option_snapshots.0[player]
                .as_ref()
                .is_some_and(|snapshot| {
                    snapshot.current == current
                        && snapshot.state == state
                        && snapshot.appearance == appearance
                });
            if !held {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Each player's options as of the last successful refresh.
    ///
    /// # Panics
    /// Before the first successful [`Self::refresh_gameplay_options`].
    pub(crate) fn gameplay_options(&self) -> [&SongLuaPlayerOptions; LUA_PLAYERS] {
        std::array::from_fn(|player| {
            &self.option_snapshots.0[player]
                .as_ref()
                .expect("options are refreshed before they are read")
                .options
        })
    }

    #[cfg(test)]
    fn gameplay_player_options(
        &mut self,
        appearance: [AppearanceEffects; LUA_PLAYERS],
    ) -> Result<[SongLuaPlayerOptions; LUA_PLAYERS], Error> {
        self.refresh_gameplay_options(appearance)?;
        Ok(self.gameplay_options().map(|options| *options))
    }
}

// Current's options from its state table, which `read` left above `current`
// on the stack, and its speed fields.
fn walk(
    stack: &mut NativeStack<'_, '_>,
    current: usize,
    appearance: AppearanceEffects,
) -> Result<SongLuaPlayerOptions, Error> {
    let mut options = SongLuaPlayerOptions {
        appearance,
        ..Default::default()
    };
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
    Ok(options)
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
    fn remembered_snapshots_follow_their_tables_and_appearance() {
        fn first(
            runtime: &mut SongRuntime,
            appearance: [AppearanceEffects; LUA_PLAYERS],
        ) -> SongLuaPlayerOptions {
            runtime.gameplay_player_options(appearance).unwrap()[0]
        }
        let mut runtime = runtime();
        let plain = [AppearanceEffects::default(); LUA_PLAYERS];
        let hidden = [AppearanceEffects {
            hidden: 0.6,
            ..Default::default()
        }; LUA_PLAYERS];
        let initial = first(&mut runtime, plain);
        assert_eq!(first(&mut runtime, plain), initial);
        execute(
            &mut runtime,
            "CURRENT = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Current')
             CURRENT:Flip(0.5)",
        );
        assert_eq!(first(&mut runtime, plain).visual.flip, Some(0.5));
        // A replaced state table is read, not the remembered one.
        execute(
            &mut runtime,
            "rawset(CURRENT, '__songlua_player_option_state', {flip = 0.25})",
        );
        assert_eq!(first(&mut runtime, plain).visual.flip, Some(0.25));
        // So is a speed field written straight to Current.
        execute(
            &mut runtime,
            "rawset(CURRENT, '__songlua_speedmod_xmod', 3)",
        );
        assert_eq!(
            first(&mut runtime, plain).scroll_speed,
            Some(ScrollSpeedSetting::XMod(3.0))
        );
        // The profile appearance is part of what a snapshot depends on.
        assert_eq!(first(&mut runtime, hidden).appearance.hidden, 0.6);
        assert_eq!(first(&mut runtime, plain).appearance.hidden, 0.0);
        assert_eq!(first(&mut runtime, plain).visual.flip, Some(0.25));
    }

    #[test]
    fn current_disabled_windows_follow_song_through_each_advance() {
        let mut runtime = runtime();
        execute(
            &mut runtime,
            r#"
            local ps = GAMESTATE:GetPlayerState(PLAYER_1)
            SONG = ps:GetPlayerOptions('ModsLevel_Song')
            CURRENT = ps:GetPlayerOptions('ModsLevel_Current')
            function DISABLED()
                return table.concat(CURRENT:GetDisabledTimingWindows(), ',')
            end
            SONG:DisableTimingWindow('TimingWindow_W5')
        "#,
        );
        let disabled = |runtime: &mut SongRuntime| {
            let code = deadlib_lua::compile_named(b"return DISABLED()", b"=disabled").unwrap();
            let value = runtime.vm.execute(&code).unwrap()[0];
            let Value::String(id) = value else {
                panic!("disabled window list")
            };
            String::from_utf8(runtime.vm.string_bytes(id).unwrap().to_vec()).unwrap()
        };
        for _ in 0..2 {
            runtime.advance_options(0.1, None).unwrap();
            assert_eq!(disabled(&mut runtime), "TimingWindow_W5");
        }
        // A change made to Current alone lasts until the next advance.
        execute(
            &mut runtime,
            "CURRENT:DisableTimingWindow('TimingWindow_W1')",
        );
        assert_eq!(disabled(&mut runtime), "TimingWindow_W1,TimingWindow_W5");
        runtime.advance_options(0.2, None).unwrap();
        assert_eq!(disabled(&mut runtime), "TimingWindow_W5");
        // So does a metatable on Current's copy, which a fresh copy lacks.
        execute(
            &mut runtime,
            r#"setmetatable(rawget(CURRENT, '__songlua_disabled_timing_windows'),
                {__index = function() return true end})"#,
        );
        assert!(disabled(&mut runtime).contains("TimingWindow_W1"));
        runtime.advance_options(0.3, None).unwrap();
        assert_eq!(disabled(&mut runtime), "TimingWindow_W5");
        execute(&mut runtime, "SONG:ResetDisabledTimingWindows()");
        runtime.advance_options(0.4, None).unwrap();
        assert_eq!(disabled(&mut runtime), "");
    }

    #[test]
    fn settled_options_resume_approaching_when_song_or_speed_changes() {
        let mut runtime = runtime();
        let flip = |runtime: &mut SongRuntime| {
            runtime
                .gameplay_player_options([AppearanceEffects::default(); LUA_PLAYERS])
                .unwrap()[0]
                .visual
                .flip
                .unwrap_or(0.0)
        };
        execute(
            &mut runtime,
            "SONG = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
             SONG:Flip(0.5, 1)",
        );
        let mut seconds = 0.0;
        let mut advance = |runtime: &mut SongRuntime| {
            seconds += 0.1;
            runtime.advance_options(seconds, None).unwrap();
        };
        for _ in 0..10 {
            advance(&mut runtime);
        }
        assert_eq!(flip(&mut runtime), 0.5);
        // Settled frames leave Current where it stands.
        advance(&mut runtime);
        assert_eq!(flip(&mut runtime), 0.5);
        // A new target moves it again, at the stored speed of 1 per second.
        execute(&mut runtime, "SONG:Flip(1, 1)");
        advance(&mut runtime);
        assert!((flip(&mut runtime) - 0.6).abs() < 1e-5);
        // A new speed alone takes effect on the next frame.
        execute(&mut runtime, "SONG:Flip(1, 2)");
        advance(&mut runtime);
        assert!((flip(&mut runtime) - 0.8).abs() < 1e-5);
        for _ in 0..5 {
            advance(&mut runtime);
        }
        assert_eq!(flip(&mut runtime), 1.0);
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
