//! Tables owned by the existing compiler's recurring callback readers.
use super::*;
use actors::compiler_tables::Query;

/// A compiler-owned batch of table identities, in native discovery order.
/// Release after consuming the tables; Rust handles alone do not trace Lua.
pub struct SongCallbackTables {
    owner: Option<Root>,
}
impl SongRuntime {
    pub fn read_song_update_function_tables(
        &mut self,
        root: Value,
        names: &[&str],
    ) -> Result<SongCallbackTables, Error> {
        self.read_song_callback_tables(
            root,
            Query::Updates {
                names: names.iter().map(|s| (*s).to_owned()).collect(),
                nested: false,
            },
        )
    }
    pub fn read_song_update_function_nested_tables(
        &mut self,
        root: Value,
        names: &[&str],
    ) -> Result<SongCallbackTables, Error> {
        self.read_song_callback_tables(
            root,
            Query::Updates {
                names: names.iter().map(|s| (*s).to_owned()).collect(),
                nested: true,
            },
        )
    }
    pub fn read_song_global_function_nested_tables(
        &mut self,
        namespace: &str,
        functions: &[&str],
        names: &[&str],
    ) -> Result<SongCallbackTables, Error> {
        self.read_song_callback_tables(
            Value::Nil,
            Query::Global {
                namespace: namespace.to_owned(),
                functions: functions.iter().map(|s| (*s).to_owned()).collect(),
                names: names.iter().map(|s| (*s).to_owned()).collect(),
            },
        )
    }
    fn read_song_callback_tables(
        &mut self,
        root: Value,
        query: Query,
    ) -> Result<SongCallbackTables, Error> {
        let (owner, _) = self
            .actors
            .read_compiler_tables(&mut self.vm, root, query)?;
        Ok(SongCallbackTables { owner: Some(owner) })
    }
    pub fn song_callback_tables(&self, tables: &SongCallbackTables) -> Result<Vec<Value>, String> {
        let owner = tables.owner.ok_or("released callback tables")?;
        let Value::Table(table) = self.vm.rooted(owner).map_err(|e| e.to_string())? else {
            unreachable!()
        };
        (1..=self.vm.raw_table_len(table).map_err(|e| e.to_string())?)
            .map(|i| {
                self.vm
                    .raw_table_get(table, Value::Number(i as f64))
                    .map_err(|e| e.to_string())
            })
            .collect()
    }
    pub fn release_song_callback_tables(
        &mut self,
        tables: &mut SongCallbackTables,
    ) -> Result<bool, Error> {
        let Some(owner) = tables.owner else {
            return Ok(false);
        };
        self.vm.unpin(owner)?;
        tables.owner = None;
        Ok(true)
    }
    /// Preserve the production reader's serialized-chunk substring test. This
    /// deliberately includes literals, nested prototypes and debug/source names.
    pub fn song_update_tree_reads_global(
        &mut self,
        root: Value,
        name: &str,
    ) -> Result<bool, Error> {
        let (owner, found) = self.actors.read_compiler_tables(
            &mut self.vm,
            root,
            Query::ReadsGlobal(name.to_owned()),
        )?;
        self.vm.unpin(owner)?;
        Ok(found)
    }
}
