use super::*;
use std::hint::black_box;

fn old_lookup(lua: &Lua, actor: &Table) -> mlua::Result<Option<Table>> {
    actor_named_children(lua, actor)?.get("NoteField")
}

fn fixture(source: &str) -> (Lua, Table) {
    let lua = Lua::new();
    let actor = lua.load(source).eval::<Table>().unwrap();
    (lua, actor)
}

fn dummy_fixture() -> (Lua, Table) {
    let lua = Lua::new();
    let actor = create_dummy_actor(&lua, "PlayerActor", |_, _| Ok(())).unwrap();
    let children = actor_children(&lua, &actor).unwrap();
    for name in ["NoteField", "Judgment", "Combo"] {
        children.set(name, lua.create_table().unwrap()).unwrap();
    }
    assert!(actor.metatable().is_some());
    assert_eq!(actor.raw_len(), 0);
    (lua, actor)
}

#[test]
fn direct_lookup_observes_registry_edits_and_ignores_registry_metatables() {
    let (lua, actor) = fixture("return {__songlua_children = {NoteField = {tag = 1}}}");
    let children = actor_children(&lua, &actor).unwrap();
    for tag in 0..8 {
        let field = lua.create_table().unwrap();
        field.set("tag", tag).unwrap();
        children.set("NoteField", field.clone()).unwrap();
        for lookup in [old_lookup, note_field_table] {
            assert_eq!(
                lookup(&lua, &actor).unwrap().unwrap().to_pointer(),
                field.to_pointer()
            );
        }
    }
    children.set("NoteField", Value::Nil).unwrap();
    let metatable = lua
        .load("return {__index = function() error('must not run') end}")
        .eval()
        .unwrap();
    children.set_metatable(Some(metatable)).unwrap();
    assert!(old_lookup(&lua, &actor).unwrap().is_none());
    assert!(note_field_table(&lua, &actor).unwrap().is_none());
    actor
        .set("__songlua_children", lua.create_table().unwrap())
        .unwrap();
    assert!(note_field_table(&lua, &actor).unwrap().is_none());
}

#[test]
fn lookup_preserves_sequence_groups_holes_metatable_effects_and_errors() {
    for source in [
        "return {}",
        "return {__songlua_children = {NoteField = {tag = 1}}, {Name = 'NoteField', tag = 2}}",
        "return {{Name = 'NoteField', tag = 1}, {Name = 'NoteField', tag = 2}}",
        "return {[2] = {Name = 'NoteField', tag = 9}, __songlua_children = {NoteField = {tag = 1}}}",
        "return setmetatable({__songlua_children = {NoteField = {tag = 1}}}, {__index = function() error('actor lookup') end})",
        "return setmetatable({__songlua_children = {NoteField = {tag = 1}}}, {__len = function() return 8 end, __index = function(_, key) if key == 1 then return {Name = 'NoteField', tag = 9} end error('unexpected lookup') end})",
        "return setmetatable({}, {__index = function(actor, key) if key == '__songlua_children' then rawset(actor, 1, {Name = 'NoteField', tag = 2}) return {NoteField = {tag = 1}} end end})",
        "return setmetatable({}, {__newindex = function(actor, key, value) rawset(actor, key, value) rawset(actor, 1, {Name = 'NoteField', tag = 2}) end})",
        "return {__songlua_children = false}",
        "return {__songlua_children = {NoteField = false}}",
        "return {__songlua_children = {NoteField = {tag = 1}}, {Name = false}}",
        "return {__songlua_children = {NoteField = {tag = 1}}, setmetatable({}, {__index = function() error('child lookup') end})}",
    ] {
        let run = |lookup: fn(&Lua, &Table) -> mlua::Result<Option<Table>>| {
            let (lua, actor) = fixture(source);
            // A second lookup exercises mutation of an existing child group too.
            (0..2)
                .map(|_| {
                    lookup(&lua, &actor)
                        .map(|field| {
                            field.map(|field| {
                                (field.get::<Option<i64>>("tag").unwrap(), field.raw_len())
                            })
                        })
                        .map_err(|err| err.to_string())
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(old_lookup), run(note_field_table), "{source}");
    }
}

#[test]
fn warmed_player_lookup_has_no_allocator_churn() {
    for (lua, actor) in [
        fixture("return {__songlua_children = {NoteField = {tag = 1}}}"),
        dummy_fixture(),
    ] {
        for _ in 0..4 {
            black_box(note_field_table(&lua, &actor).unwrap());
        }
        lua.gc_stop();
        crate::perf::assert_no_churn(|| {
            for _ in 0..128 {
                black_box(note_field_table(&lua, &actor).unwrap());
            }
        });
    }
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_note_field_lookup() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, source) in [
        ("dummy", ""),
        (
            "plain",
            "return {__songlua_children = {NoteField = {}, Judgment = {}, Combo = {}}}",
        ),
        (
            "missing",
            "return {__songlua_children = {Judgment = {}, Combo = {}}}",
        ),
        (
            "sequence",
            "return {{Name = 'NoteField'}, {Name = 'Combo'}}",
        ),
        (
            "metatable",
            "return setmetatable({__songlua_children = {NoteField = {}}}, {})",
        ),
    ] {
        let (lua, actor) = if name == "dummy" {
            dummy_fixture()
        } else {
            fixture(source)
        };
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let lookup = if old { old_lookup } else { note_field_table };
            let label = format!("lookup_{name}_{}", if old { "old" } else { "new" });
            crate::perf::measure_sampled(&label, 128, 128, || {
                for _ in 0..128 {
                    black_box(lookup(black_box(&lua), black_box(&actor)).unwrap());
                }
                lua.gc_collect().unwrap();
            });
        }
    }
}
