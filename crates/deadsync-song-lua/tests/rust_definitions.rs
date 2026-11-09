use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{self as song, SongLuaCompileContext, rust_engine::RustSongRuntime};
use std::{
    fs,
    path::{Path, PathBuf},
};

const ORIGINAL: &str = include_str!(
    "../../../tests/fixtures/itgmania-song-lua-micro/actor-definition-source/02 ActorDef.lua"
);
const PROVENANCE: &str = include_str!(
    "../../../tests/fixtures/itgmania-song-lua-micro/actor-definition-source/provenance.json"
);

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}

struct Pair {
    directory: tempfile::TempDir,
    rust: RustSongRuntime,
}

impl Pair {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        reference_test::register_path(directory.path());
        let context =
            SongLuaCompileContext::new(directory.path(), "Rust actor declaration regression");
        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.vm_mut()
            .execute(
                &compile_named(
                    b"warnings={};function Warn(message) warnings[#warnings+1]=message end",
                    b"=test setup",
                )
                .unwrap(),
            )
            .unwrap();
        Self { directory, rust }
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.directory.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, source).unwrap();
        PathBuf::from(song::file_path_string(&path))
    }

    fn compare(&mut self, source: &str) -> Vec<Scalar> {
        let values = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/observations.lua").unwrap())
            .unwrap()
            .to_vec();
        let actual = self.scalars(values);
        reference_test::assert_source_debug(
            "song/tests/rust_definitions.rs::Pair::compare",
            format!(
                "{}\n{}\n{source}",
                ORIGINAL.replace("\r\n", "\n"),
                PROVENANCE.replace("\r\n", "\n")
            ),
            &actual,
        );
        actual
    }

    fn scalars(&mut self, values: Vec<Value>) -> Vec<Scalar> {
        values
            .into_iter()
            .map(|value| match value {
                Value::Nil => Scalar::Nil,
                Value::Boolean(value) => Scalar::Boolean(value),
                Value::Number(value) => Scalar::Number(value.to_bits()),
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                value => panic!("non-scalar Rust result {value:?}"),
            })
            .collect()
    }

    fn load(&mut self, name: &str, global: &str, args: &[Value]) {
        let path = PathBuf::from(song::file_path_string(&self.directory.path().join(name)));
        let value = self.rust.execute_file(&path, args).unwrap()[0];
        self.rust
            .vm_mut()
            .set_global(global.as_bytes(), value)
            .unwrap();
    }
}

#[test]
fn constructors_preserve_declarations_identity_source_metadata_and_sparse_fields() {
    let mut pair = Pair::new();
    pair.compare(
        r#"
        local child = {Class='Quad'}
        local t = {[1]=child,[4]=child,Name='Root',Condition=false,_Dir='stale/'}
        local before = t
        t = Def.ActorFrame(t)
        return t == before, t[1] == t[4], t.Class, t.Name, t.Condition,
            t._Source, t._Line, t._Dir, t[2], t.__songlua_actor_type,
            Def.Quad == Def.Quad, getmetatable(t) == DefMetatable
    "#,
    );
    pair.compare(
        r#"
        local t = WrapInActorFrame({NullActor,NullActor})
        return t.Class, t.children[1] == t.children[2], NullActor.Class,
            NullActor._Source, NullActor._Dir, type(getmetatable(NullActor)), NullActor:getaux()
    "#,
    );
}

#[test]
fn registered_inventory_and_invalid_constructors_follow_the_pinned_source() {
    let mut pair = Pair::new();
    let provenance: serde_json::Value = serde_json::from_str(PROVENANCE).unwrap();
    let classes = provenance["registered_classes"].as_array().unwrap();
    assert_eq!(classes.len(), 43);
    for class in classes {
        let name = class["class"].as_str().unwrap();
        pair.compare(&format!(
            "local t=Def.{name}{{}}; return ActorUtil.IsRegisteredClass('{name}'),t.Class"
        ));
    }
    pair.compare(
        r#"
        local unknown = Def.MissingClass
        local ok, err = pcall(unknown, {})
        local bad = pcall(Def.Quad, false)
        return type(unknown), ok, err:find('is not a registered actor class',1,true) ~= nil,
            bad, ActorUtil.IsRegisteredClass('quad'), ActorUtil.IsRegisteredClass('')
    "#,
    );
}

#[test]
fn concatenation_keeps_inputs_shallow_shares_upvalues_and_composes_all_results() {
    let mut pair = Pair::new();
    pair.compare(
        r#"
        local n, order = 0, ''
        local shared = {}
        local left = Def.ActorFrame{[1]=shared,Name='left',Flag=false,
            Run=function(...) n=n+1; order=order..'a'; return 'discarded' end}
        local right = {[2]=shared,Name='right',Flag=true,
            Run=function(...) n=n+10; order=order..'b'; return n, select('#',...), ... end}
        local merged = left .. right
        local count, arguments, a, b, c = merged.Run(3,nil,5)
        collectgarbage()
        return merged~=left, merged~=right, left.Name, merged.Name,
            merged[1]==shared,merged[2]==shared,getmetatable(merged)==getmetatable(left),
            count, arguments,a,b,c,order,#warnings,merged.Flag,left.Flag,
            type(merged.Run),debug.getinfo(merged.Run,'S').what
    "#,
    );
    pair.compare(
        r#"
        local err={}
        local second=false
        local t=Def.Actor{Run=function() error(err) end}..{Run=function() second=true end}
        local ok,value=pcall(t.Run)
        return ok,value==err,second
    "#,
    );
}

#[test]
fn source_levels_and_collecting_metatables_match_original_lua_code() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local saved={_Level='2',_Dir='stale/'}
        local t=setmetatable({}, {
            __index=function(_,k) collectgarbage(); return saved[k] end,
            __newindex=function(_,k,v) collectgarbage(); saved[k]=v end,
        })
        local function helper(t) local d=Def.Quad(t); return d end
        local result=helper(t)
        return result==t,saved.Class,saved._Source,saved._Line,saved._Dir,getmetatable(result)==DefMetatable
    "#);
}

#[test]
fn load_actor_resolves_the_callers_source_and_preserves_varargs_and_nested_templates() {
    let mut pair = Pair::new();
    pair.write("child.lua", "error('wrong directory')");
    pair.write(
        "helper/child.lua",
        "local n=...; return Def.Quad{Name='Child',Number=n},nil,37,nil",
    );
    pair.write(
        "helper/factory.lua",
        "return function(...) local a,b,c,d=LoadActor('child.lua',...); return a,b,c,d end",
    );
    pair.write(
        "main.lua",
        r#"
        local factory=LoadActor('helper/factory.lua')
        local a,b,c,d=factory(...)
        return Def.ActorFrame{a,Child=a,Results={b,c,d},Count=select('#',factory(...))}
    "#,
    );
    pair.load("main.lua", "root", &[Value::Number(41.0)]);
    pair.compare("return root.Class,root[1]==root.Child,root[1].Class,root[1].Number,root.Count,root.Results[1],root.Results[2],root.Results[3]");
    pair.compare("return root[1]._Source,root[1]._Line,root[1]._Dir, __songlua_script_dir,__songlua_current_script_path");
}

#[test]
fn exact_wildcard_and_redirect_paths_match_existing_resolver_for_lua_files() {
    let mut pair = Pair::new();
    pair.write("scripts/target.lua", "return Def.Actor{Name='Resolved'}");
    pair.write("scripts/alias.redir", "target\n");
    pair.write("dir/default.lua", "return Def.ActorFrame{Name='Directory'}");
    pair.write(
        "main.lua",
        "return Def.ActorFrame{LoadActor('./scripts/../scripts/alias.redir'),LoadActor('dir')}",
    );
    pair.load("main.lua", "root", &[]);
    pair.compare("return root[1].Class,root[1].Name,root[2].Class,root[2].Name,root[1]._Source,root[2]._Source");
    pair.write("scripts/target-extra.lua", "return Def.Actor{}");
    pair.write(
        "failure.lua",
        "local ok=pcall(LoadActor,'scripts/alias.redir'); return {ok=ok}",
    );
    pair.load("failure.lua", "failure", &[]);
    pair.compare("return failure.ok");
}

#[test]
fn native_media_model_and_directory_templates_are_declarations_without_execution() {
    let mut pair = Pair::new();
    for name in ["image.PNG", "sound.ogg", "movie.ogv", "model.txt"] {
        pair.write(name, "");
    }
    fs::create_dir(pair.directory.path().join("animation")).unwrap();
    pair.write("media.lua", r#"
        return {LoadActor('image.PNG'),LoadActor('sound.ogg'),LoadActor('movie.ogv'),LoadActor('model.txt'),LoadActor('animation')}
    "#);
    let path = PathBuf::from(song::file_path_string(
        &pair.directory.path().join("media.lua"),
    ));
    let value = pair.rust.load_actor(&path, &[]).unwrap()[0];
    pair.rust.vm_mut().set_global(b"media", value).unwrap();
    let observations = br#"
        assert(media[1].Class=='Sprite' and media[1].Texture:match('image.PNG$'))
        assert(media[2].Class=='Sound' and media[2].File:match('sound.ogg$'))
        assert(media[3].Class=='Sprite' and media[3].Texture:match('movie.ogv$'))
        assert(media[4].Class=='Model' and media[4].Meshes==media[4].Materials and media[4].Meshes==media[4].Bones)
        assert(media[5].Class=='BGAnimation' and media[5].AniDir:match('animation$'))
        for _,t in ipairs(media) do assert(t.__songlua_actor_type==nil and t._Source:match('media.lua$')) end
        return true
    "#;
    assert_eq!(
        pair.rust
            .vm_mut()
            .execute(&compile_named(observations, b"=media controls").unwrap())
            .unwrap(),
        &[Value::Boolean(true)]
    );
}

#[test]
fn optional_paths_redirect_failures_and_native_file_types_are_explicit() {
    let mut pair = Pair::new();
    let prefix = song::file_path_string(pair.directory.path());
    pair.write("empty.redir", "");
    pair.write("a.redir", "b.redir");
    pair.write("b.redir", "a.redir");
    pair.write("dup.png", "");
    pair.write("duplicate.png", "");
    let source = format!(
        r#"
        local prefix=[[{prefix}/]]
        assert(select('#',ActorUtil.ResolvePath(prefix..'missing',1,true))==0)
        assert(not pcall(ActorUtil.ResolvePath,prefix..'missing',1))
        assert(not pcall(ActorUtil.ResolvePath,prefix..'a.redir',1))
        assert(ActorUtil.ResolvePath(prefix..'empty.redir',1)=='')
        assert(not pcall(ActorUtil.ResolvePath,prefix..'du',1))
        assert(ActorUtil.ResolvePath(prefix..'dup.png',1):match('dup.png$'))
        assert(ActorUtil.GetFileType('actor.TXT')=='FileType_Model')
        assert(ActorUtil.GetFileType('actor.sprite')=='FileType_Sprite')
        assert(ActorUtil.GetFileType('actor.xml')=='FileType_Xml')
        assert(ActorUtil.GetFileType('actor.ini')=='FileType_Ini')
        assert(ActorUtil.GetFileType('actor.unknown')=='FileType_Invalid')
        assert(ActorUtil.GetFileType('actor.webp')=='FileType_Bitmap')
        assert(ActorUtil.GetFileType('actor.qoi')=='FileType_Bitmap')
        assert(ActorUtil.GetFileType('actor.flac')=='FileType_Sound')
        assert(ActorUtil.GetFileType('actor.opus')=='FileType_Sound')
        assert(not pcall(LoadActor,''))
        return true
    "#
    );
    assert_eq!(
        pair.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=path controls").unwrap())
            .unwrap(),
        &[Value::Boolean(true)]
    );
}

#[test]
fn tail_called_relative_loader_retains_the_native_missing_source_rejection() {
    let mut pair = Pair::new();
    pair.write("child.lua", "return Def.Actor{}");
    pair.write(
        "helper.lua",
        "return function() return LoadActor('child.lua') end",
    );
    pair.write("main.lua", "local helper=LoadActor('helper.lua'); local ok,err=pcall(helper); return {ok=ok,string_error=type(err)=='string'}");
    pair.load("main.lua", "tail_result", &[]);
    pair.compare("return tail_result.ok");
    // The Rust file host preserves errors; the PUC source control has ordinary
    // Lua errors. Both must fail, rather than choosing another asset directory.
    pair.compare("return tail_result.string_error");
}

#[test]
fn process_relative_prefix_resolution_uses_the_current_directory() {
    let mut pair = Pair::new();
    let file = tempfile::Builder::new()
        .prefix("rust-actor-path-")
        .suffix(".png")
        .tempfile_in(std::env::current_dir().unwrap())
        .unwrap();
    let name = file.path().file_name().unwrap().to_str().unwrap();
    let prefix = file.path().file_stem().unwrap().to_str().unwrap();
    let script = format!("return ActorUtil.ResolvePath('{prefix}',1)=='{name}'");
    assert_eq!(
        pair.rust
            .vm_mut()
            .execute(&compile_named(script.as_bytes(), b"=relative path").unwrap())
            .unwrap(),
        &[Value::Boolean(true)]
    );
}

#[test]
fn existing_song_fixtures_load_as_declaration_trees_without_running_commands() {
    let mut pair = Pair::new();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/song-lua");
    for name in [
        "queued-child-tween.lua",
        "color-inheritance.lua",
        "child-table.lua",
        "cmd-function-args.lua",
    ] {
        let path = PathBuf::from(song::file_path_string(&fixtures.join(name)));
        let value = pair.rust.load_actor(&path, &[]).unwrap()[0];
        pair.rust.vm_mut().set_global(b"root", value).unwrap();
        let observation = r#"
            local nodes,commands,names=0,0,{}
            local function visit(t)
                nodes=nodes+1; names[#names+1]=t.Class..':'..(t.Name or '')
                for key,value in pairs(t) do
                    if type(key)=='string' and key:match('Command$') and type(value)=='function' then commands=commands+1 end
                end
                for _,child in ipairs(t.children or t) do if type(child)=='table' then visit(child) end end
            end
            visit(root)
            return nodes,commands,table.concat(names,'|')
        "#;
        pair.compare(observation);
    }
}

#[test]
fn loaded_functions_preserve_native_filename_markers_in_debug_info() {
    let mut pair = Pair::new();
    let path = pair.write("source.lua", "return function() end");
    let function = pair.rust.execute_file(&path, &[]).unwrap()[0];
    pair.rust
        .vm_mut()
        .set_global(b"source_function", function)
        .unwrap();
    let value = pair
        .rust
        .vm_mut()
        .execute(
            &compile_named(
                b"return debug.getinfo(source_function,'S').source",
                b"=source observation",
            )
            .unwrap(),
        )
        .unwrap()[0];
    let Value::String(id) = value else {
        panic!("source name")
    };
    assert_eq!(
        pair.rust.vm_mut().string_bytes(id).unwrap(),
        format!("@{}", song::file_path_string(&path)).as_bytes()
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
