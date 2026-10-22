use super::*;
use crate::draw_capture::perf::allocations;
use crate::lua_util::{lua_text_value, read_bitmap_text_attributes};
use mlua::Lua;

#[test]
fn text_values_preserve_conversion_errors_and_old_snapshots() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let mut text: Arc<str> = Arc::from("original");
    let original = text.clone();
    let long = "雪\0é\n".repeat(1024);
    for value in [
        Value::String(lua.create_string("label\0雪").unwrap()),
        Value::String(lua.create_string(&long).unwrap()),
        Value::Integer(i64::MAX),
        Value::Number(-0.0),
        Value::Number(0.125),
        Value::Number(f64::INFINITY),
        Value::Number(f64::NAN),
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Nil,
        Value::Table(lua.create_table().unwrap()),
    ] {
        actor.raw_set("Text", value).unwrap();
        // Lua 5.1 stores numbers as doubles, including large Rust integers.
        let expected = lua_text_value(actor.get("Text").unwrap()).unwrap();
        sync_text(&actor, &mut text).unwrap();
        assert_eq!(text.as_ref(), expected);
        let same = text.clone();
        sync_text(&actor, &mut text).unwrap();
        assert!(Arc::ptr_eq(&same, &text));
    }
    actor
        .raw_set("Text", lua.create_string([0xff]).unwrap())
        .unwrap();
    let last = text.clone();
    assert_eq!(
        sync_text(&actor, &mut text).unwrap_err().to_string(),
        lua_text_value(actor.get("Text").unwrap())
            .unwrap_err()
            .to_string()
    );
    assert!(Arc::ptr_eq(&last, &text));
    assert_eq!(original.as_ref(), "original");
}

#[test]
fn text_lookup_runs_once_per_publication_and_retains_errors() {
    let lua = Lua::new();
    let actor: Table = lua
        .load(
            r#"
reads, fail = 0, false
return setmetatable({}, {__index=function(_, key)
    assert(key == 'Text')
    reads = reads + 1
    if fail then error('authored text failure') end
    return 'label'
end})
"#,
        )
        .eval()
        .unwrap();
    let mut text = Arc::from("");
    for read in 1..=3 {
        sync_text(&actor, &mut text).unwrap();
        assert_eq!(lua.globals().get::<i32>("reads").unwrap(), read);
    }
    let last = text.clone();
    lua.globals().set("fail", true).unwrap();
    assert!(
        sync_text(&actor, &mut text)
            .unwrap_err()
            .to_string()
            .contains("authored text failure")
    );
    assert_eq!(lua.globals().get::<i32>("reads").unwrap(), 4);
    assert!(Arc::ptr_eq(&last, &text));
}

fn assert_attributes(actor: &Table, attributes: &mut Arc<[TextAttribute]>) {
    let expected = read_bitmap_text_attributes(actor).unwrap();
    sync_attributes(actor, attributes).unwrap();
    assert_eq!(attributes.len(), expected.len());
    assert!(
        attributes
            .iter()
            .zip(expected.iter())
            .all(|(a, b)| same_attribute(a, b))
    );
}

#[test]
fn attribute_publication_observes_in_place_edits_and_preserves_snapshots() {
    let lua = Lua::new();
    let actor: Table = lua
        .load(
            r#"
return {__songlua_text_attributes={
    {start=-4, length=-1, color={0.25,0.5,0.75,1}},
    {start=1, length=0},
    {start='2', length='3', color=false, glow={0.1,0.2,0.3,0.4}},
}}
"#,
        )
        .eval()
        .unwrap();
    let source: Table = actor.get("__songlua_text_attributes").unwrap();
    let mut attributes = Arc::from([]);
    assert_attributes(&actor, &mut attributes);
    assert_eq!((attributes[0].start, attributes[0].length), (0, usize::MAX));
    assert_eq!((attributes[1].start, attributes[1].length), (2, 3));
    assert_eq!(attributes[1].color, [1.0; 4]);
    let original = attributes.clone();
    assert_attributes(&actor, &mut attributes);
    assert!(Arc::ptr_eq(&original, &attributes));
    let first: Table = source.get(1).unwrap();
    first.get::<Table>("color").unwrap().set(1, 0.75).unwrap();
    assert_attributes(&actor, &mut attributes);
    assert_eq!(attributes[0].color[0], 0.75);
    assert_eq!(original[0].color[0], 0.25);
    source
        .set(
            4,
            lua.load("return {start=7, length=2}")
                .eval::<Table>()
                .unwrap(),
        )
        .unwrap();
    assert_attributes(&actor, &mut attributes);
    assert_eq!(attributes.len(), 3);
    source.set(2, Value::Nil).unwrap();
    assert_attributes(&actor, &mut attributes);
    assert_eq!(attributes.len(), 1, "sequence stops at its first hole");
    actor.set("__songlua_text_attributes", Value::Nil).unwrap();
    assert_attributes(&actor, &mut attributes);
    assert!(attributes.is_empty());
    let empty = attributes.clone();
    assert_attributes(&actor, &mut attributes);
    assert!(Arc::ptr_eq(&empty, &attributes));
}

#[test]
fn attribute_publication_preserves_float_bits() {
    let lua = Lua::new();
    let actor: Table = lua
        .load(
            r#"
return {__songlua_text_attributes={{color={0,0,0,1},glow={0,0,0,1},
vertex_colors={{0,0,0,1},{0,0,0,1},{0,0,0,1},{0,0,0,1}}}}}
"#,
        )
        .eval()
        .unwrap();
    let entry = actor
        .get::<Table>("__songlua_text_attributes")
        .unwrap()
        .get::<Table>(1)
        .unwrap();
    let mut attributes = Arc::from([]);
    assert_attributes(&actor, &mut attributes);
    for field in ["color", "glow", "vertex_colors"] {
        let mut values: Table = entry.get(field).unwrap();
        if field == "vertex_colors" {
            values = values.get(3).unwrap();
        }
        for value in [-0.0_f32, f32::NAN, f32::INFINITY, 0.0] {
            let old = attributes.clone();
            values.set(1, value).unwrap();
            assert_attributes(&actor, &mut attributes);
            assert!(!Arc::ptr_eq(&old, &attributes), "changed bits in {field}");
            let same = attributes.clone();
            assert_attributes(&actor, &mut attributes);
            assert!(Arc::ptr_eq(&same, &attributes));
        }
    }
}

#[test]
fn attribute_lookup_runs_once_and_errors_do_not_publish_partial_arrays() {
    let lua = Lua::new();
    let actor: Table = lua
        .load(
            r#"
reads, attrs = 0, {{start=1}}
return setmetatable({}, {__index=function(_, key)
    assert(key == '__songlua_text_attributes')
    reads = reads + 1
    return attrs
end})
"#,
        )
        .eval()
        .unwrap();
    let mut attributes = Arc::from([]);
    sync_attributes(&actor, &mut attributes).unwrap();
    assert_eq!(lua.globals().get::<i32>("reads").unwrap(), 1);
    let original = attributes.clone();
    // A valid changed prefix must not replace the snapshot if a later entry fails.
    lua.load("attrs[1].start=8; attrs[2]=false; reads=0")
        .exec()
        .unwrap();
    let error = sync_attributes(&actor, &mut attributes).unwrap_err();
    assert_eq!(lua.globals().get::<i32>("reads").unwrap(), 1);
    assert_eq!(error, read_bitmap_text_attributes(&actor).unwrap_err());
    assert!(Arc::ptr_eq(&original, &attributes));
    lua.load("attrs=false").exec().unwrap();
    assert_eq!(
        sync_attributes(&actor, &mut attributes).unwrap_err(),
        read_bitmap_text_attributes(&actor).unwrap_err()
    );
    assert!(Arc::ptr_eq(&original, &attributes));
}

#[test]
fn unchanged_text_and_attributes_reduce_allocation_traffic() {
    let lua = Lua::new();
    let actor: Table = lua
        .load(
            r#"
return {Text='steady text', __songlua_text_attributes={
    {start=0,length=3,color={1,0.5,0.25,1},glow={0.1,0.2,0.3,0.4}},
    {start=4,length=-1},
}}
"#,
        )
        .eval()
        .unwrap();
    let mut text = Arc::from("");
    let mut attributes = Arc::from([]);
    for _ in 0..8 {
        sync_text(&actor, &mut text).unwrap();
        sync_attributes(&actor, &mut attributes).unwrap();
    }
    let (_, text_churn) = allocations::measure(|| {
        for _ in 0..32 {
            sync_text(&actor, &mut text).unwrap();
        }
    });
    let (_, attribute_churn) = allocations::measure(|| {
        for _ in 0..32 {
            sync_attributes(&actor, &mut attributes).unwrap();
        }
    });
    let (_, prior_churn) = allocations::measure(|| {
        for _ in 0..32 {
            let current = lua_text_value(actor.get("Text").unwrap()).unwrap();
            if text.as_ref() != current { text = current.into(); }
            attributes = read_bitmap_text_attributes(&actor).unwrap();
        }
    });
    println!("32 publications: text={text_churn:?}, attributes={attribute_churn:?}, prior publication with shared decoder={prior_churn:?}");
    assert!(text_churn.allocs + attribute_churn.allocs < prior_churn.allocs);
    assert!(text_churn.allocated_bytes + attribute_churn.allocated_bytes < prior_churn.allocated_bytes);
    actor.set("__songlua_text_attributes", Value::Nil).unwrap();
    sync_attributes(&actor, &mut attributes).unwrap();
    // Missing fields still require the normal protected Lua lookup, which may
    // invoke __index. Publication must add no allocations beyond that lookup.
    let (_, lookup) = allocations::measure(|| {
        for _ in 0..32 {
            actor.get::<Option<Table>>("__songlua_text_attributes").unwrap();
        }
    });
    let (_, publication) = allocations::measure(|| {
        for _ in 0..32 {
            sync_attributes(&actor, &mut attributes).unwrap();
        }
    });
    assert_eq!(publication, lookup);
}
