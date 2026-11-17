//! Function-ease target probes and retained actor identities.
use super::*;

/// Owned observations and an explicit root batch for the following captures.
pub struct SongFunctionEaseProbe {
    pub target: Option<crate::SongLuaEaseTarget>,
    pub methods: Vec<String>,
    owner: Option<Root>,
}
impl SongRuntime {
    /// Probe at beat zero with RNG restoration. Script and actor writes persist.
    /// Body errors suppress classification; malformed reads and cleanup errors
    /// still propagate in native order. The enclosing compiler owns wider rewind.
    pub fn probe_song_function_ease_target(
        &mut self,
        function: Value,
    ) -> Result<SongFunctionEaseProbe, String> {
        let pin = self.vm.pin(function).map_err(|e| e.to_string())?;
        let mut returned = None;
        let mut owner = None;
        let result = (|| {
            let previous = self.values().map_err(|e| e.to_string())?;
            self.set_beat(0.).map_err(|e| e.to_string())?;
            let session = self
                .actors
                .begin_ease_probe(&mut self.vm)
                .map_err(|e| e.to_string())?;
            owner = Some(session.pin);
            let random = self.random.preserve();
            self.actors.set_ease_probe_active(&mut self.vm, true);
            let body = self
                .vm
                .call(function, &[Value::Number(1.)])
                .map(|v| v.first().copied().unwrap_or(Value::Nil))
                .and_then(|v| self.vm.pin(v).map(|pin| returned = Some(pin)));
            drop(random);
            // Native removes the marker rather than restoring a nested one.
            self.actors.set_ease_probe_active(&mut self.vm, false);
            let read = self
                .actors
                .finish_ease_probe(&mut self.vm, &session)
                .map_err(|e| e.to_string())?;
            self.set_values(previous.0, previous.1)
                .map_err(|e| e.to_string())?;
            let target = if body.is_ok() {
                read.target.map_err(|e| e.to_string())?
            } else {
                None
            };
            Ok(SongFunctionEaseProbe {
                target,
                methods: read.methods,
                owner: owner.take(),
            })
        })();
        if let Some(owner) = owner {
            self.vm.unpin(owner).map_err(|e| e.to_string())?;
        }
        if let Some(pin) = returned {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        self.vm.unpin(pin).map_err(|e| e.to_string())?;
        result
    }
    pub fn song_function_ease_probe_actors(
        &self,
        probe: &SongFunctionEaseProbe,
    ) -> Result<Vec<Value>, String> {
        let owner = probe.owner.ok_or("released function ease probe")?;
        let Value::Table(owner) = self.vm.rooted(owner).map_err(|e| e.to_string())? else {
            unreachable!()
        };
        let Value::Table(actors) = self
            .vm
            .raw_table_get(owner, Value::Number(actors::ease_probe::ACTORS as f64))
            .map_err(|e| e.to_string())?
        else {
            unreachable!()
        };
        let mut values = Vec::new();
        for i in 1usize.. {
            let v = self
                .vm
                .raw_table_get(actors, Value::Number(i as f64))
                .map_err(|e| e.to_string())?;
            if v == Value::Nil {
                break;
            }
            values.push(v);
        }
        Ok(values)
    }
    pub fn release_song_function_ease_probe(
        &mut self,
        probe: &mut SongFunctionEaseProbe,
    ) -> Result<bool, Error> {
        let Some(owner) = probe.owner else {
            return Ok(false);
        };
        self.vm.unpin(owner)?;
        probe.owner = None;
        Ok(true)
    }
}
