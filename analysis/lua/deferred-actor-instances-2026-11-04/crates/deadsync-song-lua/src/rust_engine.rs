//! Opt-in song host integration with the shared Rust Lua engine.
//!
//! This portion owns song clock, RNG, file loading and actor declarations. Actor
//! instances, managers, baking and live-scene bindings still need migration before this can
//! replace the legacy song compiler. It does not install a partial GAMESTATE.
use crate::{
    LUA_PLAYERS, SONG_LUA_RUNTIME_BEAT_KEY as BEAT, SONG_LUA_RUNTIME_BPS_KEY as BPS,
    SONG_LUA_RUNTIME_DELTA_BEAT_KEY as DELTA_BEAT,
    SONG_LUA_RUNTIME_DELTA_SECONDS_KEY as DELTA_SECONDS, SONG_LUA_RUNTIME_KEY,
    SONG_LUA_RUNTIME_RATE_KEY as RATE, SONG_LUA_RUNTIME_SECONDS_KEY as SECONDS,
    SongLuaCompileContext, song_display_bps, song_elapsed_seconds_at, song_music_rate,
};
use deadlib_lua::{Error, HostCall, Root, TableId, Value, Vm};
mod actors;
mod definitions;
pub use actors::{ActorScene, ActorSnapshot};
mod loader;
mod random;
pub use random::RandomSnapshot;
#[cfg(feature = "test-support")]
pub mod test_support;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const FREEZE: &str = "__songlua_freeze";
const DELAY: &str = "__songlua_delay";
const BPMS: &str = "__songlua_timing_bpms";
const PLAYERS: &str = crate::runtime::PLAYER_SONG_POSITIONS_KEY;

struct Clock {
    timing: Option<deadsync_rules::timing::TimingData>,
    live: AtomicBool,
}

#[derive(Clone, Copy)]
enum Operation {
    Float(&'static str),
    Boolean(&'static str),
    MusicSeconds,
    Values(bool),
    SetValues(bool),
    SetBeat,
    Reset,
}

#[derive(Clone)]
struct Binding {
    clock: Arc<Clock>,
    operation: Operation,
}

/// An independent Rust VM with the song clock's Rust host bindings installed.
/// All retained Lua objects are explicit roots or traced callback captures.
/// Separate instances have separate heaps and clock state.
pub struct RustSongRuntime {
    vm: Vm,
    random: random::RandomState,
    loader: Root,
    actor_loader: Root,
    actors: actors::Actors,
    clock: Arc<Clock>,
    runtime: Root,
    position: Root,
    players: Vec<Root>,
    operations: [Root; 6],
}

impl RustSongRuntime {
    pub fn new(context: &SongLuaCompileContext) -> Result<Self, Error> {
        let mut vm = Vm::new();
        let random = random::install(&mut vm, context.random_seed)?;
        let loader = loader::install(&mut vm, &context.song_dir)?;
        let actor_loader = definitions::install(&mut vm)?;
        let actors = actors::Actors::install(&mut vm)?;
        let clock = Arc::new(Clock {
            timing: context.song_timing.clone(),
            live: AtomicBool::new(context.live),
        });
        let runtime = vm.try_new_table()?;
        let runtime_root = vm.pin(Value::Table(runtime))?;
        for key in [BEAT, SECONDS, DELTA_BEAT, DELTA_SECONDS] {
            put(&mut vm, runtime, key, Value::Number(0.0))?;
        }
        put(
            &mut vm,
            runtime,
            BPS,
            number(if context.live {
                10.0
            } else {
                song_display_bps(context)
            }),
        )?;
        put(&mut vm, runtime, RATE, number(song_music_rate(context)))?;
        put(&mut vm, runtime, FREEZE, Value::Boolean(false))?;
        put(&mut vm, runtime, DELAY, Value::Boolean(false))?;
        let tonumber = vm.get_global(b"tonumber")?;
        // Keep the original primitive conversion even if a script replaces it.
        let tonumber_root = vm.pin(tonumber)?;
        let song_position = position(&mut vm, &clock, runtime, tonumber)?;
        let mut players = Vec::new();
        if context.live {
            let positions = vm.try_new_table()?;
            put(&mut vm, runtime, PLAYERS, Value::Table(positions))?;
            for index in 1..=LUA_PLAYERS {
                let player = vm.try_new_table()?;
                vm.raw_table_set(positions, Value::Number(index as f64), Value::Table(player))?;
                for key in [BEAT, SECONDS, BPS, RATE, FREEZE, DELAY] {
                    let key = vm.try_intern(key.as_bytes())?;
                    let value = vm.raw_table_get(runtime, key)?;
                    vm.raw_table_set(player, key, value)?;
                }
                players.push(position(&mut vm, &clock, player, tonumber)?);
            }
        }
        let bpms = vm.try_new_table()?;
        put(&mut vm, runtime, BPMS, Value::Table(bpms))?;
        for (index, &(beat, bpm)) in context.song_timing_bpms.iter().enumerate() {
            let segment = vm.try_new_table()?;
            vm.raw_table_set(
                bpms,
                Value::Number((index + 1) as f64),
                Value::Table(segment),
            )?;
            for (slot, value) in [
                beat,
                bpm,
                song_elapsed_seconds_at(beat, context) * song_music_rate(context),
            ]
            .into_iter()
            .enumerate()
            {
                vm.raw_table_set(segment, Value::Number((slot + 1) as f64), number(value))?;
            }
        }
        let mut roots = Vec::new();
        for operation in [
            Operation::Values(false),
            Operation::Values(true),
            Operation::SetValues(false),
            Operation::SetValues(true),
            Operation::SetBeat,
            Operation::Reset,
        ] {
            let function = vm.new_host_function_with_data(
                callback,
                &[tonumber],
                Binding {
                    clock: clock.clone(),
                    operation,
                },
            )?;
            roots.push(vm.pin(function)?);
        }
        vm.set_global(SONG_LUA_RUNTIME_KEY.as_bytes(), Value::Table(runtime))?;
        vm.unpin(tonumber_root)?;
        Ok(Self {
            vm,
            random,
            loader,
            actor_loader,
            actors,
            clock,
            runtime: runtime_root,
            position: song_position,
            players,
            operations: roots.try_into().expect("six clock operations"),
        })
    }

    /// Register additional song bindings or execute scripts in this same VM.
    /// Values obtained here follow `deadlib-lua`'s ordinary rooting contract.
    pub fn vm_mut(&mut self) -> &mut Vm {
        &mut self.vm
    }

    /// Construct a scene from a declaration snapshot and run child-first Init.
    /// The runtime retains instances and command closures until explicit release.
    pub fn instantiate_actor(&mut self, declaration: Value) -> Result<ActorScene, Error> {
        self.actors.instantiate(&mut self.vm, declaration)
    }

    /// The stable root instance, nil for a false Condition, or None after release.
    pub fn actor_scene_root(&self, scene: ActorScene) -> Option<Value> {
        self.actors.root(scene)
    }

    pub fn actor_scene_snapshot(&self, scene: ActorScene) -> Option<Vec<ActorSnapshot>> {
        self.actors.snapshot(scene)
    }

    /// Retire native actors and release their Lua roots. Retained Lua instance
    /// tables remain ordinary tables; native methods reject the retired objects.
    pub fn release_actor_scene(&mut self, scene: ActorScene) -> Result<bool, Error> {
        self.actors.release(&mut self.vm, scene)
    }

    /// Script command errors are reported and execution continues, as in native
    /// Actor::RunCommands. Fatal engine limits still abort the Rust entry.
    pub fn take_actor_diagnostics(&self) -> Vec<String> {
        self.actors.take_diagnostics()
    }

    /// Load a song script with cmd preprocessing and its file environment.
    /// Store or pin the returned function before another VM entry or collection.
    pub fn load_file(&mut self, path: &std::path::Path) -> Result<Value, Error> {
        let path = self.vm.try_intern(path.to_string_lossy().as_bytes())?;
        let loader = self.vm.rooted(self.loader)?;
        Ok(self.vm.call(loader, &[path])?[0])
    }

    /// Load an actor declaration or Lua-returned template with native LoadActor
    /// semantics. This preserves live callbacks but does not instantiate actors.
    /// Relative entry paths are resolved against the process working directory;
    /// Lua callers resolve relative paths against their own source file.
    pub fn load_actor(
        &mut self,
        path: &std::path::Path,
        arguments: &[Value],
    ) -> Result<&[Value], Error> {
        let path = self.vm.try_intern(path.to_string_lossy().as_bytes())?;
        let mut inputs = Vec::with_capacity(arguments.len() + 1);
        inputs.push(path);
        inputs.extend_from_slice(arguments);
        let loader = self.vm.rooted(self.actor_loader)?;
        self.vm.call(loader, &inputs)
    }

    /// Execute a song file with arguments, preserving every returned value.
    /// This loads code into the existing session; it does not construct a scene.
    pub fn execute_file(
        &mut self,
        path: &std::path::Path,
        arguments: &[Value],
    ) -> Result<&[Value], Error> {
        let mut roots = Vec::with_capacity(arguments.len());
        let function = (|| {
            for &value in arguments {
                roots.push(self.vm.pin(value)?);
            }
            self.load_file(path)
        })();
        for root in roots {
            self.vm.unpin(root)?;
        }
        self.vm.call(function?, arguments)
    }

    /// Preserve this song's random stream across a baking/analysis probe.
    pub fn preserve_random(&self) -> RandomSnapshot {
        self.random.preserve()
    }

    pub fn runtime_table(&self) -> Result<Value, Error> {
        self.vm.rooted(self.runtime)
    }
    pub fn song_position(&self) -> Result<Value, Error> {
        self.vm.rooted(self.position)
    }

    /// Zero-based player index. Player positions exist for live contexts only.
    pub fn player_position(&self, index: usize) -> Result<Option<Value>, Error> {
        self.players
            .get(index)
            .map(|&root| self.vm.rooted(root))
            .transpose()
    }

    /// Live frames contain audio music time; preparation uses elapsed wall time.
    /// This changes clock interpretation without replacing captured Lua tables.
    pub fn set_live(&mut self, live: bool) {
        self.clock.live.store(live, Ordering::Relaxed);
    }

    pub fn values(&mut self) -> Result<(f32, f32), Error> {
        self.read_values(0)
    }
    pub fn delta_values(&mut self) -> Result<(f32, f32), Error> {
        self.read_values(1)
    }
    pub fn set_values(&mut self, beat: f32, seconds: f32) -> Result<(), Error> {
        self.run(2, &[number(beat), number(seconds)]).map(|_| ())
    }
    pub fn set_delta_values(&mut self, beat: f32, seconds: f32) -> Result<(), Error> {
        self.run(3, &[number(beat), number(seconds)]).map(|_| ())
    }
    pub fn set_beat(&mut self, beat: f32) -> Result<(), Error> {
        self.run(4, &[number(beat)]).map(|_| ())
    }
    pub fn reset(&mut self) -> Result<(), Error> {
        self.run(5, &[]).map(|_| ())
    }

    fn read_values(&mut self, operation: usize) -> Result<(f32, f32), Error> {
        let [Value::Number(a), Value::Number(b)] = self.run(operation, &[])? else {
            unreachable!("clock getters return two numbers")
        };
        Ok((*a as f32, *b as f32))
    }

    fn run(&mut self, operation: usize, args: &[Value]) -> Result<&[Value], Error> {
        let function = self.vm.rooted(self.operations[operation])?;
        // Match legacy helpers: look up the current runtime, while SongPosition
        // closures retain the table they were originally constructed with.
        let runtime = self.vm.get_global(SONG_LUA_RUNTIME_KEY.as_bytes())?;
        let mut arguments = vec![runtime];
        arguments.extend_from_slice(args);
        self.vm.call(function, &arguments)
    }
}

fn number(value: f32) -> Value {
    Value::Number(f64::from(value))
}

fn put(vm: &mut Vm, table: TableId, key: &str, value: Value) -> Result<(), Error> {
    let key = vm.try_intern(key.as_bytes())?;
    vm.raw_table_set(table, key, value)
}

fn position(
    vm: &mut Vm,
    clock: &Arc<Clock>,
    runtime: TableId,
    tonumber: Value,
) -> Result<Root, Error> {
    let table = vm.try_new_table()?;
    let root = vm.pin(Value::Table(table))?;
    for (name, operation) in [
        ("GetSongBeat", Operation::Float(BEAT)),
        ("GetSongBeatVisible", Operation::Float(BEAT)),
        ("GetMusicSeconds", Operation::MusicSeconds),
        ("GetMusicSecondsVisible", Operation::MusicSeconds),
        ("GetCurBPS", Operation::Float(BPS)),
        ("GetFreeze", Operation::Boolean(FREEZE)),
        ("GetDelay", Operation::Boolean(DELAY)),
    ] {
        let function = vm.new_host_function_with_data(
            callback,
            &[tonumber, Value::Table(runtime)],
            Binding {
                clock: clock.clone(),
                operation,
            },
        )?;
        put(vm, table, name, function)?;
    }
    Ok(root)
}

fn field(call: &mut HostCall<'_>, table: Value, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_get(table, key)
}

fn set(call: &mut HostCall<'_>, table: Value, key: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_set(table, key, value)
}

fn numeric(call: &mut HostCall<'_>, value: Value) -> Result<f64, Error> {
    if let Value::Number(number) = value {
        return Ok(number);
    }
    let tonumber = call.captures()[0];
    match call.call(tonumber, &[value])?.first().copied() {
        Some(Value::Number(number)) => Ok(number),
        _ => Err(call.error(b"expected number or string coercible to number")),
    }
}

fn float(call: &mut HostCall<'_>, table: Value, key: &str) -> Result<f32, Error> {
    let value = field(call, table, key)?;
    Ok(numeric(call, value)? as f32)
}

fn optional_float(
    call: &mut HostCall<'_>,
    table: Value,
    key: &str,
    default: f32,
) -> Result<f32, Error> {
    let value = field(call, table, key)?;
    if value == Value::Nil {
        Ok(default)
    } else {
        Ok(numeric(call, value)? as f32)
    }
}

fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Binding { clock, operation } = call
        .function_data::<Binding>()
        .expect("clock binding")
        .clone();
    let table = call
        .captures()
        .get(1)
        .copied()
        .unwrap_or_else(|| call.arguments()[0]);
    if !matches!(table, Value::Table(_)) {
        return Err(call.error(b"expected song runtime table"));
    }
    match operation {
        Operation::Float(key) => {
            let value = float(call, table, key)?;
            call.push(number(value))
        }
        Operation::Boolean(key) => {
            let value = field(call, table, key)?;
            call.push(Value::Boolean(value.is_truthy()))
        }
        Operation::MusicSeconds => {
            let elapsed = field(call, table, SECONDS)?;
            let elapsed = numeric(call, elapsed)?;
            let rate = float(call, table, RATE)?;
            let seconds = if clock.live.load(Ordering::Relaxed) {
                elapsed as f32
            } else {
                (elapsed * f64::from(rate)) as f32
                    + clock
                        .timing
                        .as_ref()
                        .map_or(0.0, |clock| clock.get_time_for_beat_exact(0.0))
            };
            // Match the legacy whole-float integer path, including -0 -> +0.
            let seconds = if seconds.fract() == 0.0 && seconds.abs() < 1e14 {
                (seconds as i64) as f64
            } else {
                f64::from(seconds)
            };
            call.push(Value::Number(seconds))
        }
        Operation::Values(delta) => {
            let (a, b) = if delta {
                (DELTA_BEAT, DELTA_SECONDS)
            } else {
                (BEAT, SECONDS)
            };
            let a = float(call, table, a)?;
            let b = float(call, table, b)?;
            call.push(number(a))?;
            call.push(number(b))
        }
        Operation::SetValues(delta) => {
            let (a, b) = if delta {
                (DELTA_BEAT, DELTA_SECONDS)
            } else {
                (BEAT, SECONDS)
            };
            let first = call.arguments()[1];
            let second = call.arguments()[2];
            set(call, table, a, first)?;
            set(call, table, b, second)
        }
        Operation::SetBeat => {
            let beat = call.number(1)? as f32;
            let mut bps = optional_float(call, table, BPS, 1.0)?;
            let rate = optional_float(call, table, RATE, 1.0)?;
            if let Some(clock) = &clock.timing {
                let seconds = clock.get_time_for_beat_exact(beat);
                let position = clock.get_song_position(seconds);
                set(call, table, BEAT, number(beat))?;
                set(
                    call,
                    table,
                    SECONDS,
                    number((seconds - clock.get_time_for_beat_exact(0.0)) / rate.max(f32::EPSILON)),
                )?;
                set(call, table, FREEZE, Value::Boolean(position.is_in_freeze))?;
                set(call, table, DELAY, Value::Boolean(position.is_in_delay))?;
                return set(call, table, BPS, number(position.bpm / 60.0));
            }
            let mut segment_beat = 0.0;
            let mut segment_seconds = 0.0;
            let bpms = field(call, table, BPMS)?;
            if bpms != Value::Nil {
                // The nested argument window roots even a table synthesized by
                // __index while numeric conversion can reenter the collector.
                let tonumber = call.captures()[0];
                let reader = call.new_host_function(read_segments, &[tonumber])?;
                let values = call.call(reader, &[bpms, number(beat), number(bps)])?;
                let [Value::Number(a), Value::Number(b), Value::Number(c)] = values else {
                    unreachable!("segment tuple")
                };
                segment_beat = *a as f32;
                bps = *b as f32;
                segment_seconds = *c as f32;
            }
            set(call, table, BEAT, number(beat))?;
            set(
                call,
                table,
                SECONDS,
                number(
                    (segment_seconds + (beat - segment_beat) / bps.max(f32::EPSILON))
                        / rate.max(f32::EPSILON),
                ),
            )?;
            set(call, table, BPS, number(bps))
        }
        Operation::Reset => {
            let reset = call.new_host_function(reset_position, &[])?;
            call.call(reset, &[table])?;
            set(call, table, DELTA_BEAT, number(0.0))?;
            set(call, table, DELTA_SECONDS, number(0.0))?;
            let key = call.try_intern(PLAYERS.as_bytes())?;
            let Value::Table(table) = table else {
                return Err(call.error(b"expected song runtime table"));
            };
            let players = call.raw_table_get(table, key)?;
            if players != Value::Nil {
                let reset = call.new_host_function(reset_players, &[])?;
                call.call(reset, &[players])?;
            }
            Ok(())
        }
    }
}

fn read_segments(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(bpms) = call.arguments()[0] else {
        return Err(call.error(b"expected timing BPM table"));
    };
    let beat = call.number(1)? as f32;
    let mut bps = call.number(2)? as f32;
    let mut segment_beat = 0.0;
    let mut seconds = 0.0;
    for index in 1.. {
        let segment = call.raw_table_get(bpms, Value::Number(index as f64))?;
        if segment == Value::Nil {
            break;
        }
        let Value::Table(segment) = segment else {
            return Err(call.error(b"expected BPM segment table"));
        };
        let value = call.raw_table_get(segment, Value::Number(1.0))?;
        let next = numeric(call, value)? as f32;
        if next > beat {
            break;
        }
        segment_beat = next;
        let value = call.raw_table_get(segment, Value::Number(2.0))?;
        bps = numeric(call, value)? as f32 / 60.0;
        let value = call.raw_table_get(segment, Value::Number(3.0))?;
        seconds = numeric(call, value)? as f32;
    }
    for value in [segment_beat, bps, seconds] {
        call.push(number(value))?;
    }
    Ok(())
}

fn reset_players(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(players) = call.arguments()[0] else {
        return Err(call.error(b"expected player positions table"));
    };
    for index in 1.. {
        let player = call.raw_table_get(players, Value::Number(index as f64))?;
        if player == Value::Nil {
            break;
        }
        // Each player's table stays alive if its setters detach it from the
        // runtime and collect before the rest of its fields have been reset.
        let reset = call.new_host_function(reset_position, &[])?;
        call.call(reset, &[player])?;
    }
    Ok(())
}

fn reset_position(call: &mut HostCall<'_>) -> Result<(), Error> {
    let table = call.arguments()[0];
    if !matches!(table, Value::Table(_)) {
        return Err(call.error(b"expected song position table"));
    }
    for (key, value) in [
        (BEAT, number(0.0)),
        (SECONDS, number(0.0)),
        (BPS, number(10.0)),
        (FREEZE, Value::Boolean(false)),
        (DELAY, Value::Boolean(false)),
    ] {
        set(call, table, key, value)?;
    }
    Ok(())
}
