//! Existing host uptime reads; playback uptime is independent of music time.
use super::*;

#[derive(Clone)]
struct Binding {
    clock: Arc<Clock>,
    #[cfg(feature = "test-support")]
    inputs: Arc<Mutex<Inputs>>,
}

pub(super) fn install(
    vm: &mut Vm,
    clock: Arc<Clock>,
    _context: &SongLuaCompileContext,
) -> Result<(), Error> {
    let captures = [vm.get_global(b"tonumber")?, vm.get_global(b"_G")?];
    let binding = Binding {
        clock,
        #[cfg(feature = "test-support")]
        inputs: Arc::new(Mutex::new(Inputs::new(_context))),
    };
    let function = vm.new_host_function_with_data(since_start, &captures, binding)?;
    vm.set_global(b"GetTimeSinceStart", function)
}

fn since_start(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().unwrap().clone();
    #[cfg(feature = "test-support")]
    if let Some(value) = Inputs::next(&binding.inputs, call, "GetTimeSinceStart")? {
        return call.push(Value::Number(value));
    }
    let runtime = field(call, call.captures()[1], SONG_LUA_RUNTIME_KEY)?;
    let live = binding.clock.live.load(Ordering::Relaxed);
    if runtime == Value::Nil && !live {
        return call.push(Value::Number(0.));
    }
    if !matches!(runtime, Value::Table(_)) {
        return Err(call.error(b"expected song runtime table"));
    }
    let value = call.with_roots(&[runtime], |call| {
        if live {
            let value = field(call, runtime, "__songlua_uptime_seconds")?;
            if value == Value::Nil {
                Ok(0.)
            } else {
                numeric(call, value)
            }
        } else {
            let value = optional_float(call, runtime, SECONDS, 0.).unwrap_or(0.);
            // Match the native integer path, including negative zero.
            Ok(
                if value.is_finite()
                    && value.fract().abs() <= f32::EPSILON
                    && value >= i64::MIN as f32
                    && value < -(i64::MIN as f32)
                {
                    (value as i64) as f64
                } else {
                    value as f64
                },
            )
        }
    })?;
    call.push(Value::Number(value))
}

#[cfg(feature = "test-support")]
pub(super) struct Inputs {
    song_root: String,
    reads: std::collections::HashMap<(String, String, i32), std::collections::VecDeque<f64>>,
}

#[cfg(feature = "test-support")]
impl Inputs {
    fn canonical(source: &str, song_root: &str) -> String {
        let source = source
            .trim_start_matches('@')
            .replace('\\', "/")
            .replace("/./", "/");
        if let Some(relative) = source.strip_prefix(song_root) {
            return format!("song:/{}", relative.trim_start_matches('/'));
        }
        if let Some((_, relative)) = source.split_once("Themes/")
            && let Some((_, relative)) = relative.split_once('/')
        {
            return format!("theme:/{relative}");
        }
        source
    }

    pub(super) fn new(context: &SongLuaCompileContext) -> Self {
        let mut result = Self {
            song_root: context.song_dir.to_string_lossy().replace('\\', "/"),
            reads: Default::default(),
        };
        for input in &context.host_clock_inputs {
            let source = Self::canonical(&input.source, &result.song_root);
            result
                .reads
                .entry((input.clock.clone(), source, input.line))
                .or_default()
                .push_back(input.value);
        }
        result
    }

    pub(super) fn next(
        inputs: &Arc<Mutex<Self>>,
        call: &mut HostCall<'_>,
        domain: &str,
    ) -> Result<Option<f64>, Error> {
        if inputs.lock().unwrap().reads.is_empty() {
            return Ok(None);
        }
        let f = call.new_host_function_with_data(
            |child| {
                let (inputs, domain) = child
                    .function_data::<(Arc<Mutex<Inputs>>, String)>()
                    .unwrap()
                    .clone();
                let value = Inputs::next_stack(&inputs, child, &domain)?;
                child.push(value.map_or(Value::Nil, Value::Number))
            },
            &[],
            (inputs.clone(), domain.to_owned()),
        )?;
        Ok(match call.call(f, &[])?.first() {
            Some(Value::Number(n)) => Some(*n),
            _ => None,
        })
    }
    fn next_stack(
        inputs: &Arc<Mutex<Self>>,
        call: &mut HostCall<'_>,
        domain: &str,
    ) -> Result<Option<f64>, Error> {
        let song_root = {
            let inputs = inputs.lock().unwrap();
            if inputs.reads.is_empty() {
                return Ok(None);
            }
            inputs.song_root.clone()
        };
        let mut stack = call.native_stack()?;
        for level in 1..32 {
            let Some(frame) = stack.get_stack(stack.thread_id(), level)? else {
                continue;
            };
            let info = stack.get_info(frame, b"Sl")?;
            if info.what == Some(b"C\0") {
                continue;
            }
            let Some(source) = info.source.and_then(|source| stack.debug_bytes(source)) else {
                continue;
            };
            let source = source.split(|&byte| byte == 0).next().unwrap_or_default();
            let source = Self::canonical(&String::from_utf8_lossy(source), &song_root);
            let line = info.current_line.unwrap_or(0).max(0);
            return Ok(inputs
                .lock()
                .unwrap()
                .reads
                .get_mut(&(domain.to_owned(), source, line))
                .and_then(|values| values.pop_front()));
        }
        Ok(None)
    }
}
