//! Host-driven option phases. Sampling a position never advances option groups.
use super::*;

/// Values already published by the host's player/controller and health phases.
/// `None` uses the construction-time fallback, before live state is available.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerStateStatus {
    pub autoplay: Option<bool>,
    pub health: Option<&'static str>,
}

impl RustSongRuntime {
    pub fn set_player_state_status(&mut self, status: [PlayerStateStatus; LUA_PLAYERS]) {
        *self.player_status.lock().expect("player status") = status;
    }

    /// Advance SongOptions, read RateModsAffectFGChanges, then advance P1 and P2.
    /// Explicit deltas are ordered SongOptions/P1/P2; otherwise use clock elapsed time.
    pub fn advance_options(
        &mut self,
        seconds: f64,
        deltas: Option<[f32; LUA_PLAYERS + 1]>,
    ) -> Result<(), Error> {
        let mut values = vec![Value::Number(seconds)];
        let mut root = None;
        if let Some(deltas) = deltas {
            let table = self.vm.try_new_table()?;
            let pinned = self.vm.pin(Value::Table(table))?;
            let result = (|| {
                for (i, delta) in deltas.into_iter().enumerate() {
                    self.vm
                        .raw_table_set(table, Value::Number((i + 1) as f64), number(delta))?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.vm.unpin(pinned)?;
                return Err(error);
            }
            root = Some(pinned);
            values.push(Value::Table(table));
        }
        let result = self.option_operation("Advance", &values);
        if let Some(root) = root {
            self.vm.unpin(root)?;
        }
        result
    }

    pub fn restart_options(&mut self) -> Result<(), Error> {
        self.option_operation("Restart", &[])
    }

    pub fn clear_attack_options(&mut self) -> Result<(), Error> {
        self.option_operation("ClearAttacks", &[])
    }

    /// Zero-based player and ModsLevel (Preferred/Stage/Song/Current) indices.
    pub fn apply_modifiers(
        &mut self,
        player: usize,
        level: usize,
        modifiers: &str,
    ) -> Result<(), Error> {
        let text = self.vm.try_intern(modifiers.as_bytes())?;
        self.option_operation(
            "Apply",
            &[
                Value::Number(player as f64),
                Value::Number(level as f64),
                text,
            ],
        )
    }

    pub fn rebuild_attack_options(
        &mut self,
        player: usize,
        master: bool,
        modifiers: &[String],
    ) -> Result<(), Error> {
        let table = self.vm.try_new_table()?;
        let root = self.vm.pin(Value::Table(table))?;
        let result = (|| {
            for (index, text) in modifiers.iter().enumerate() {
                let text = self.vm.try_intern(text.as_bytes())?;
                self.vm
                    .raw_table_set(table, Value::Number((index + 1) as f64), text)?;
            }
            self.option_operation(
                "RebuildAttacks",
                &[
                    Value::Number(player as f64),
                    Value::Boolean(master),
                    Value::Table(table),
                ],
            )
        })();
        self.vm.unpin(root)?;
        result
    }

    fn option_operation(&mut self, name: &str, arguments: &[Value]) -> Result<(), Error> {
        let mut roots = Vec::new();
        let result = (|| {
            for &value in arguments {
                roots.push(self.vm.pin(value)?);
            }
            let Value::Table(catalog) = self.vm.rooted(self.catalog)? else {
                unreachable!()
            };
            let key = self.vm.try_intern(b"option_operations")?;
            let Value::Table(operations) = self.vm.raw_table_get(catalog, key)? else {
                unreachable!()
            };
            let key = self.vm.try_intern(name.as_bytes())?;
            let function = self.vm.raw_table_get(operations, key)?;
            self.vm.call(function, arguments).map(|_| ())
        })();
        for root in roots {
            self.vm.unpin(root)?;
        }
        result
    }
}
