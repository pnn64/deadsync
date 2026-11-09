"""Stage exact runtime-owner ports outside watched source during verification."""
from pathlib import Path
import hashlib, json
d=Path(__file__).resolve().parent;r=d.parents[2]
source=r/'crates/deadsync-song-lua/src/actor_input.rs'
native=source.read_text()
start=native.index('/// A closed snapshot of screen input callbacks')
end=native.index('pub(crate) fn compile(',start)
owner=native[start:end].replace('SongLuaCompiledActorInputs','CompiledActorInputs')
owner=owner.replace('crate::input::compiled::Frame','super::super::input::Frame')
header='''//! Existing VM-free input snapshots, using the Rust bytecode program compiler.
use super::*;
use crate::{SongLuaActorInputAction, SongLuaInputScalar, SongLuaInputEvent,
    SongLuaOverlayState, SongLuaTrackedActorTarget};
use super::super::baked_program::{self as program, Action, Binding, Datum, Field,
    FieldValue, Group, Parameter, Types};
use deadlib_lua::NativeStack;
use rustc_hash::FxHashMap;
mod tables;
pub(super) mod update;
pub use update::CompiledActorInteractions;

'''
update_source=source.with_name('actor_input')/'update.rs'
update=update_source.read_text()
start=update.index('struct Node {');end=update.index('pub(crate) fn compile(',start)
update_owner=update[start:end].replace('SongLuaCompiledActorInputs','CompiledActorInputs')
update_owner=update_owner.replace('SongLuaCompiledActorInteractions','CompiledActorInteractions')
update_header='''//! Existing combined input/update owner, retaining native hierarchy arithmetic.
use super::*;

pub(in crate::rust_engine::actors) enum SourceJob {
    Enter { actor: TableId, parent: Option<usize>, wrappers: bool },
    Callback { actor: TableId, slot: usize },
}

'''
outputs={'draft-actor-input-snapshot-owner.rs.txt':header+owner,
         'draft-actor-input-snapshot-update-owner.rs.txt':update_header+update_owner}
for name,contents in outputs.items():
    target=d/name;assert not target.exists(),name
    target.write_text(contents)
report={'state':'draft only; no production snapshot factory yet',
        'native_owner_sources':{p.relative_to(r).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()
                                for p in [source,update_source]},
        'drafts':{name:{'lines':contents.count('\n'),'sha256':hashlib.sha256((d/name).read_bytes()).hexdigest()}
                  for name,contents in outputs.items()}}
(d/'draft-actor-input-snapshot-owners.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
