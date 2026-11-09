//! Song-local MT19937 bindings shared by math and MersenneTwister.
use crate::compat::SongLuaRandom;
use deadlib_lua::{Error, HostCall, Value, Vm};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Clone)]
pub(super) struct RandomState(Arc<Mutex<SongLuaRandom>>);

impl RandomState {
    fn lock(&self) -> MutexGuard<'_, SongLuaRandom> {
        // No lock is held while entering Lua. A Rust panic requires discarding
        // the VM; recovering its RNG lock only lets owned snapshots drop safely.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn preserve(&self) -> RandomSnapshot {
        RandomSnapshot {
            state: self.clone(),
            previous: self.lock().clone(),
        }
    }
}

/// Restore this song VM's random stream when the guard is dropped.
/// Baking probes can consume or reseed it without changing subsequent playback.
/// Snapshots may nest, and own no Lua handles or references into VM storage.
#[must_use = "dropping the snapshot restores the saved random stream"]
pub struct RandomSnapshot {
    state: RandomState,
    previous: SongLuaRandom,
}

impl Drop for RandomSnapshot {
    fn drop(&mut self) {
        *self.state.lock() = self.previous.clone();
    }
}

fn effective_seed(seed: i32) -> u32 {
    // Match the established song API: an explicit zero selects wall time.
    if seed == 0 {
        chrono::Utc::now().timestamp() as u32
    } else {
        seed as u32
    }
}

pub(super) fn install(vm: &mut Vm, seed: u32) -> Result<RandomState, Error> {
    let state = RandomState(Arc::new(Mutex::new(SongLuaRandom::seeded(effective_seed(
        seed as i32,
    )))));
    let namespace = vm.try_new_table()?;
    let root = vm.pin(Value::Table(namespace))?;
    let result = (|| {
        let seed = vm.new_host_function_with_data(seed_callback, &[], state.clone())?;
        super::put(vm, namespace, "Seed", seed)?;
        let random = vm.new_host_function_with_data(random_callback, &[], state.clone())?;
        super::put(vm, namespace, "Random", random)?;
        // The two names intentionally refer to the exact same Lua functions.
        let Value::Table(math) = vm.get_global(b"math")? else {
            unreachable!("fresh VM math table")
        };
        super::put(vm, math, "randomseed", seed)?;
        super::put(vm, math, "random", random)?;
        vm.set_global(b"MersenneTwister", Value::Table(namespace))
    })();
    vm.unpin(root)?;
    result?;
    Ok(state)
}

fn integer_argument(call: &mut HostCall<'_>, index: usize) -> Result<i32, Error> {
    let value = call
        .number(index)
        .ok()
        .filter(|value| {
            value.is_finite()
                && value.trunc() >= f64::from(i32::MIN)
                && value.trunc() <= f64::from(i32::MAX)
        })
        .ok_or_else(|| {
            call.error(format!("bad argument #{} (number expected)", index + 1).as_bytes())
        })?;
    Ok(value as i32)
}

fn seed_callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let seed = integer_argument(call, 0)?;
    let state = call
        .function_data::<RandomState>()
        .expect("random binding")
        .clone();
    *state.lock() = SongLuaRandom::seeded(effective_seed(seed));
    Ok(())
}

fn random_callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let state = call
        .function_data::<RandomState>()
        .expect("random binding")
        .clone();
    let value = match call.arguments().len() {
        0 => state.lock().real(),
        1 | 2 => {
            let count = call.arguments().len();
            let (lower, upper) = if count == 1 {
                (1, integer_argument(call, 0)?)
            } else {
                (integer_argument(call, 0)?, integer_argument(call, 1)?)
            };
            if lower > upper || (count == 2 && lower == upper) {
                return Err(call.error(b"interval is empty"));
            }
            state.lock().integer(lower, upper)
        }
        _ => return Err(call.error(b"wrong number of arguments")),
    };
    call.push(Value::Number(value))
}
