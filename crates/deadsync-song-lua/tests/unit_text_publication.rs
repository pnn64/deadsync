//! Original immutable text publication fixtures on the Rust VM.
use deadlib_lua::{Error, HostCall, Root, Value};
use deadlib_present::actors::TextAttribute;
use deadsync_song_lua::{
    SongLuaCompileContext, {SongRuntime, test_support},
};
use std::sync::{Arc, Mutex};
#[expect(
    dead_code,
    reason = "this fixture uses scoped measurement from the shared allocation API"
)]
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "support/baked_program.rs"]
mod api;

#[derive(Clone, Copy)]
enum Operation {
    SyncText,
    SyncAttributes,
    ReadText,
    ReadAttributes,
    LookupAttributes,
}
struct Output {
    text: Arc<str>,
    attributes: Arc<[TextAttribute]>,
    decoded_text: Option<String>,
}
struct Binding {
    operation: Operation,
    output: Arc<Mutex<Option<Output>>>,
}
struct TextFixture {
    host: SongRuntime,
    actor: Value,
    functions: [Root; 5],
    output: Arc<Mutex<Option<Output>>>,
}
fn reader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call.function_data::<Binding>().unwrap();
    let op = binding.operation;
    let output = binding.output.clone();
    let Value::Table(actor) = call.captures()[2] else {
        unreachable!()
    };
    let mut current = output.lock().unwrap().take().unwrap();
    let result = (|| {
        match op {
            Operation::SyncText => test_support::fixture_sync_text(call, actor, &mut current.text)?,
            Operation::SyncAttributes => {
                test_support::fixture_sync_text_attributes(call, actor, &mut current.attributes)?
            }
            Operation::ReadText => {
                current.decoded_text = Some(test_support::fixture_text_content(call, actor)?)
            }
            Operation::ReadAttributes => {
                current.attributes = test_support::fixture_text_attributes(call, actor)?
            }
            Operation::LookupAttributes => {
                let key = call.try_intern(b"__songlua_text_attributes")?;
                let _ = call.table_get(Value::Table(actor), key)?;
            }
        }
        Ok(())
    })();
    *output.lock().unwrap() = Some(current);
    result
}
impl TextFixture {
    fn new(mut host: SongRuntime, actor: Value) -> Self {
        let output = Arc::new(Mutex::new(Some(Output {
            text: Arc::from(""),
            attributes: Arc::from([]),
            decoded_text: None,
        })));
        let functions = [
            Operation::SyncText,
            Operation::SyncAttributes,
            Operation::ReadText,
            Operation::ReadAttributes,
            Operation::LookupAttributes,
        ]
        .map(|operation| {
            test_support::prepare_actor_reader(
                &mut host,
                actor,
                reader,
                Binding {
                    operation,
                    output: output.clone(),
                },
            )
            .unwrap()
        });
        Self {
            host,
            actor,
            functions,
            output,
        }
    }
    fn run(&mut self, operation: usize) -> Result<(), String> {
        let vm = self.host.vm_mut();
        let function = vm.rooted(self.functions[operation]).unwrap();
        vm.call(function, &[])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    fn exec(&mut self, source: &str) {
        api::eval(self.host.vm_mut(), source);
    }
    fn number(&mut self, name: &[u8]) -> i32 {
        api::as_number(self.host.vm_mut().get_global(name).unwrap()) as i32
    }
}
impl Drop for TextFixture {
    fn drop(&mut self) {
        for root in self.functions {
            self.host.vm_mut().unpin(root).unwrap();
        }
    }
}
fn runtime() -> SongRuntime {
    SongRuntime::new(&SongLuaCompileContext::new(".", "text publication unit")).unwrap()
}
fn fixture(source: &str) -> TextFixture {
    let mut host = runtime();
    let actor = api::eval(host.vm_mut(), source)[0];
    TextFixture::new(host, actor)
}
fn sync_text(f: &mut TextFixture, text: &mut Arc<str>) -> Result<(), String> {
    f.output.lock().unwrap().as_mut().unwrap().text = text.clone();
    let result = f.run(0);
    *text = f.output.lock().unwrap().as_ref().unwrap().text.clone();
    result
}
fn sync_attributes(
    f: &mut TextFixture,
    attributes: &mut Arc<[TextAttribute]>,
) -> Result<(), String> {
    f.output.lock().unwrap().as_mut().unwrap().attributes = attributes.clone();
    let result = f.run(1);
    *attributes = f
        .output
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .attributes
        .clone();
    result
}
fn read_text(f: &mut TextFixture) -> Result<String, String> {
    f.run(2)?;
    Ok(f.output
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .decoded_text
        .take()
        .unwrap())
}
fn read_attributes(f: &mut TextFixture) -> Result<Arc<[TextAttribute]>, String> {
    f.run(3)?;
    Ok(f.output
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .attributes
        .clone())
}
fn same_attribute(a: &TextAttribute, b: &TextAttribute) -> bool {
    a.start == b.start
        && a.length == b.length
        && a.color.map(f32::to_bits) == b.color.map(f32::to_bits)
        && a.vertex_colors.map(|c| c.map(|c| c.map(f32::to_bits)))
            == b.vertex_colors.map(|c| c.map(|c| c.map(f32::to_bits)))
        && a.glow.map(|c| c.map(f32::to_bits)) == b.glow.map(|c| c.map(f32::to_bits))
}
fn assert_attributes(f: &mut TextFixture, attributes: &mut Arc<[TextAttribute]>) {
    let expected = read_attributes(f).unwrap();
    sync_attributes(f, attributes).unwrap();
    assert_eq!(attributes.len(), expected.len());
    assert!(
        attributes
            .iter()
            .zip(expected.iter())
            .all(|(a, b)| same_attribute(a, b))
    );
}
fn field(f: &mut TextFixture, owner: Value, name: &[u8]) -> Value {
    api::get(f.host.vm_mut(), owner, name)
}
fn index(f: &mut TextFixture, owner: Value, index: usize) -> Value {
    let Value::Table(owner) = owner else {
        panic!("fixture table expected")
    };
    let value = f
        .host
        .vm_mut()
        .raw_table_get(owner, Value::Number(index as f64))
        .unwrap();
    f.host.vm_mut().pin(value).unwrap();
    value
}
fn set_index(f: &mut TextFixture, owner: Value, index: usize, value: Value) {
    let Value::Table(owner) = owner else {
        panic!("fixture table expected")
    };
    f.host
        .vm_mut()
        .raw_table_set(owner, Value::Number(index as f64), value)
        .unwrap();
}

#[test]
fn text_values_preserve_conversion_errors_and_old_snapshots() {
    let mut f = fixture("return {}");
    let mut text: Arc<str> = Arc::from("original");
    let original = text.clone();
    let long = ORIGINAL_LONG.repeat(1024);
    let vm = f.host.vm_mut();
    let values = [
        api::string(vm, ORIGINAL_LABEL),
        api::string(vm, &long),
        Value::Number(i64::MAX as f64),
        Value::Number(-0.0),
        Value::Number(0.125),
        Value::Number(f64::INFINITY),
        Value::Number(f64::NAN),
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Nil,
        api::table(vm),
    ];
    for value in values {
        api::set(f.host.vm_mut(), f.actor, b"Text", value);
        let expected = read_text(&mut f).unwrap();
        sync_text(&mut f, &mut text).unwrap();
        assert_eq!(text.as_ref(), expected);
        let same = text.clone();
        sync_text(&mut f, &mut text).unwrap();
        assert!(Arc::ptr_eq(&same, &text));
    }
    let invalid = api::string(f.host.vm_mut(), [0xff]);
    api::set(f.host.vm_mut(), f.actor, b"Text", invalid);
    let last = text.clone();
    assert_eq!(
        sync_text(&mut f, &mut text).unwrap_err(),
        read_text(&mut f).unwrap_err()
    );
    assert!(Arc::ptr_eq(&last, &text));
    assert_eq!(original.as_ref(), "original");
}
#[test]
fn text_lookup_runs_once_per_publication_and_retains_errors() {
    let mut f = fixture(ORIGINAL_TEXT_LOOKUP);
    let mut text = Arc::from("");
    for read in 1..=3 {
        sync_text(&mut f, &mut text).unwrap();
        assert_eq!(f.number(b"reads"), read);
    }
    let last = text.clone();
    f.host
        .vm_mut()
        .set_global(b"fail", Value::Boolean(true))
        .unwrap();
    assert!(
        sync_text(&mut f, &mut text)
            .unwrap_err()
            .contains("authored text failure")
    );
    assert_eq!(f.number(b"reads"), 4);
    assert!(Arc::ptr_eq(&last, &text));
}
#[test]
fn attribute_publication_observes_in_place_edits_and_preserves_snapshots() {
    let mut f = fixture(ORIGINAL_ATTRIBUTES);
    let actor = f.actor;
    let source = field(&mut f, actor, b"__songlua_text_attributes");
    let mut attributes = Arc::from([]);
    assert_attributes(&mut f, &mut attributes);
    assert_eq!((attributes[0].start, attributes[0].length), (0, usize::MAX));
    assert_eq!((attributes[1].start, attributes[1].length), (2, 3));
    assert_eq!(attributes[1].color, [1.0; 4]);
    let original = attributes.clone();
    assert_attributes(&mut f, &mut attributes);
    assert!(Arc::ptr_eq(&original, &attributes));
    let first = index(&mut f, source, 1);
    let color = field(&mut f, first, b"color");
    set_index(&mut f, color, 1, Value::Number(0.75));
    assert_attributes(&mut f, &mut attributes);
    assert_eq!(attributes[0].color[0], 0.75);
    assert_eq!(original[0].color[0], 0.25);
    let fourth = api::eval(f.host.vm_mut(), "return {start=7, length=2}")[0];
    set_index(&mut f, source, 4, fourth);
    assert_attributes(&mut f, &mut attributes);
    assert_eq!(attributes.len(), 3);
    set_index(&mut f, source, 2, Value::Nil);
    assert_attributes(&mut f, &mut attributes);
    assert_eq!(attributes.len(), 1, "sequence stops at its first hole");
    api::set(
        f.host.vm_mut(),
        actor,
        b"__songlua_text_attributes",
        Value::Nil,
    );
    assert_attributes(&mut f, &mut attributes);
    assert!(attributes.is_empty());
    let empty = attributes.clone();
    assert_attributes(&mut f, &mut attributes);
    assert!(Arc::ptr_eq(&empty, &attributes));
}
#[test]
fn attribute_publication_preserves_float_bits() {
    let mut f = fixture(ORIGINAL_ATTRIBUTE_BITS);
    let actor = f.actor;
    let source = field(&mut f, actor, b"__songlua_text_attributes");
    let entry = index(&mut f, source, 1);
    let mut attributes = Arc::from([]);
    assert_attributes(&mut f, &mut attributes);
    for name in ["color", "glow", "vertex_colors"] {
        let mut values = field(&mut f, entry, name.as_bytes());
        if name == "vertex_colors" {
            values = index(&mut f, values, 3);
        }
        for value in [-0.0_f32, f32::NAN, f32::INFINITY, 0.0] {
            let old = attributes.clone();
            set_index(&mut f, values, 1, Value::Number(value as f64));
            assert_attributes(&mut f, &mut attributes);
            assert!(!Arc::ptr_eq(&old, &attributes), "changed bits in {name}");
            let same = attributes.clone();
            assert_attributes(&mut f, &mut attributes);
            assert!(Arc::ptr_eq(&same, &attributes));
        }
    }
}
#[test]
fn attribute_lookup_runs_once_and_errors_do_not_publish_partial_arrays() {
    let mut f = fixture(ORIGINAL_ATTRIBUTE_LOOKUP);
    let mut attributes = Arc::from([]);
    sync_attributes(&mut f, &mut attributes).unwrap();
    assert_eq!(f.number(b"reads"), 1);
    let original = attributes.clone();
    f.exec("attrs[1].start=8; attrs[2]=false; reads=0");
    let error = sync_attributes(&mut f, &mut attributes).unwrap_err();
    assert_eq!(f.number(b"reads"), 1);
    assert_eq!(error, read_attributes(&mut f).unwrap_err());
    assert!(Arc::ptr_eq(&original, &attributes));
    f.exec("attrs=false");
    assert_eq!(
        sync_attributes(&mut f, &mut attributes).unwrap_err(),
        read_attributes(&mut f).unwrap_err()
    );
    assert!(Arc::ptr_eq(&original, &attributes));
}
#[test]
fn unchanged_text_and_attributes_reduce_allocation_traffic() {
    let mut f = fixture(ORIGINAL_STEADY_TEXT);
    let mut text = Arc::from("");
    let mut attributes = Arc::from([]);
    for _ in 0..8 {
        sync_text(&mut f, &mut text).unwrap();
        sync_attributes(&mut f, &mut attributes).unwrap();
    }
    let (_, text_churn) = allocations::measure(|| {
        for _ in 0..32 {
            sync_text(&mut f, &mut text).unwrap();
        }
    });
    let (_, attribute_churn) = allocations::measure(|| {
        for _ in 0..32 {
            sync_attributes(&mut f, &mut attributes).unwrap();
        }
    });
    let (_, prior_churn) = allocations::measure(|| {
        for _ in 0..32 {
            let current = read_text(&mut f).unwrap();
            if text.as_ref() != current {
                text = current.into();
            }
            attributes = read_attributes(&mut f).unwrap();
        }
    });
    println!(
        "32 publications: text={text_churn:?}, attributes={attribute_churn:?}, prior publication with shared decoder={prior_churn:?}"
    );
    assert!(text_churn.allocs + attribute_churn.allocs < prior_churn.allocs);
    assert!(
        text_churn.allocated_bytes + attribute_churn.allocated_bytes < prior_churn.allocated_bytes
    );
    api::set(
        f.host.vm_mut(),
        f.actor,
        b"__songlua_text_attributes",
        Value::Nil,
    );
    sync_attributes(&mut f, &mut attributes).unwrap();
    let (_, lookup) = allocations::measure(|| {
        for _ in 0..32 {
            f.run(4).unwrap();
        }
    });
    let (_, publication) = allocations::measure(|| {
        for _ in 0..32 {
            sync_attributes(&mut f, &mut attributes).unwrap();
        }
    });
    assert_eq!(publication, lookup);
}

const ORIGINAL_TEXT_LOOKUP: &str = r#"
reads, fail = 0, false
return setmetatable({}, {__index=function(_, key)
    assert(key == 'Text')
    reads = reads + 1
    if fail then error('authored text failure') end
    return 'label'
end})
"#;
const ORIGINAL_ATTRIBUTES: &str = r#"
return {__songlua_text_attributes={
    {start=-4, length=-1, color={0.25,0.5,0.75,1}},
    {start=1, length=0},
    {start='2', length='3', color=false, glow={0.1,0.2,0.3,0.4}},
}}
"#;
const ORIGINAL_ATTRIBUTE_BITS: &str = r#"
return {__songlua_text_attributes={{color={0,0,0,1},glow={0,0,0,1},
vertex_colors={{0,0,0,1},{0,0,0,1},{0,0,0,1},{0,0,0,1}}}}}
"#;
const ORIGINAL_ATTRIBUTE_LOOKUP: &str = r#"
reads, attrs = 0, {{start=1}}
return setmetatable({}, {__index=function(_, key)
    assert(key == '__songlua_text_attributes')
    reads = reads + 1
    return attrs
end})
"#;
const ORIGINAL_STEADY_TEXT: &str = r#"
return {Text='steady text', __songlua_text_attributes={
    {start=0,length=3,color={1,0.5,0.25,1},glow={0.1,0.2,0.3,0.4}},
    {start=4,length=-1},
}}
"#;
const ORIGINAL_LONG: &str = "雪\0é\n";
const ORIGINAL_LABEL: &str = "label\0雪";
