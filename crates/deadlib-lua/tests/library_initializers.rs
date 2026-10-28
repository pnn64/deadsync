use deadlib_lua::{Value, Vm, compile};

#[test]
fn bare_states_execute_core_language_without_standard_library_globals() {
    let mut vm = Vm::new_bare();
    for name in [
        b"_G".as_slice(),
        b"_VERSION",
        b"assert",
        b"math",
        b"string",
        b"table",
        b"debug",
        b"coroutine",
        b"io",
        b"os",
        b"package",
    ] {
        assert_eq!(vm.global(name), Value::Nil);
    }
    assert_eq!(vm.heap_stats().tables, 2);
    assert_eq!(
        vm.execute(&compile(b"local t={value=6};return t.value*7,('x').sub,_G").unwrap())
            .unwrap_err()
            .kind(),
        deadlib_lua::ErrorKind::Runtime
    );
    assert_eq!(
        vm.execute(&compile(b"local t={value=6};return t.value*7,_G,math").unwrap())
            .unwrap(),
        &[Value::Number(42.), Value::Nil, Value::Nil]
    );
    vm.collect_garbage().unwrap();
}

#[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
#[test]
fn standard_initializers_open_a_bare_state_and_create_fresh_closures() {
    let mut vm = Vm::new_bare();
    let loader = deadlib_lua::NativeCAbiLoader::default();
    for name in [
        b"base".as_slice(),
        b"math",
        b"string",
        b"table",
        b"os",
        b"debug",
    ] {
        let function = vm
            .new_native_function(loader.standard_library(name).unwrap())
            .unwrap();
        vm.call(function, &[]).unwrap();
        vm.set_global(&[b"open_".as_slice(), name].concat(), function)
            .unwrap();
    }
    vm.execute(&compile(br#"
assert(_G==getfenv(0)and _VERSION=='Lua 5.1'and package==nil and io==nil)
assert(math.sin(0)==0 and string.sub('abc',2)=='bc'and ('abc'):sub(2)=='bc')
local t={3,1,2};table.sort(t);assert(table.concat(t,',')=='1,2,3')
local c=coroutine.create(function()coroutine.yield(17);return 29 end)
local ok,n=coroutine.resume(c);assert(ok and n==17);ok,n=coroutine.resume(c);assert(ok and n==29)
assert(debug.getregistry()._LOADED.math==math and debug.getregistry()._LOADED._G==_G)
local old=math.sin;assert(open_math()==math and math.sin~=old and math.mod==math.fmod)
local old=assert;local oldproxy=newproxy(true);local g,c=open_base();assert(g==_G and c==coroutine and old~=assert)
assert(pcall(newproxy,oldproxy)==false)
local iter=select(2,debug.getupvalue(pairs,1));assert(iter~=next)
local p=newproxy(true);assert(type(newproxy(p))=='userdata')
assert(debug.getinfo(math.sin).what=='C'and os.difftime(9,4)==5)
"#).unwrap()).unwrap();
    assert!(loader.standard_library(b"unknown").is_err());
}

#[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
#[test]
fn bare_states_run_the_public_c_initializer_corpus_in_twelve_orders() {
    use deadlib_lua::{NativeCAbiLoader, NativeLibraryLoader, compile_named};
    use std::{path::PathBuf, process::Command};
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let folder = manifest
        .join("../../target/lua-library-initializers")
        .join(format!("{}-{}", std::process::id(), std::env::consts::ARCH));
    std::fs::create_dir_all(&folder).unwrap();
    let module = folder.join("initializers.dll");
    let headers = manifest.join("../../vendor/lua-src/lua-5.1.5");
    let source = manifest.join("tests/native-default/library-initializers.c");
    let output = if cfg!(windows) {
        let finder = PathBuf::from(std::env::var_os("ProgramFiles(x86)").unwrap())
            .join("Microsoft Visual Studio/Installer/vswhere.exe");
        let found = Command::new(finder)
            .args([
                "-latest",
                "-products",
                "*",
                "-find",
                "VC/Auxiliary/Build/vcvars64.bat",
            ])
            .output()
            .unwrap();
        assert!(found.status.success());
        let setup = String::from_utf8(found.stdout).unwrap().trim().to_string();
        let response = folder.join("module.rsp");
        let args = [
            "/nologo".into(),
            "/LD".into(),
            "/O2".into(),
            "/MT".into(),
            "/DLUA_BUILD_AS_DLL".into(),
            format!("/I{}", headers.display()),
            source.to_string_lossy().into_owned(),
            PathBuf::from(env!("OUT_DIR"))
                .join("lua51.lib")
                .to_string_lossy()
                .into_owned(),
            "/link".into(),
            format!("/OUT:{}", module.display()),
            format!("/IMPLIB:{}", folder.join("initializers.lib").display()),
        ];
        std::fs::write(
            &response,
            args.iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(" "),
        )
        .unwrap();
        let script = folder.join("module.cmd");
        std::fs::write(
            &script,
            format!(
                "@call \"{setup}\" >nul\n@cl @\"{}\"\n@exit /b %errorlevel%\n",
                response.display()
            ),
        )
        .unwrap();
        Command::new("cmd")
            .args(["/d", "/c"])
            .arg(script)
            .current_dir(&folder)
            .output()
            .unwrap()
    } else {
        let mut cc = Command::new("cc");
        cc.args(["-shared", "-fPIC", "-O2"]);
        if cfg!(target_arch = "x86") {
            cc.args(["-m32", "-msse2", "-mfpmath=sse"]);
        }
        cc.arg("-I")
            .arg(headers)
            .arg(source)
            .arg("-o")
            .arg(&module)
            .output()
            .unwrap()
    };
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let loader = NativeCAbiLoader::default();
    let library = loader.open(module.to_string_lossy().as_bytes()).unwrap();
    let program = compile_named(
        include_bytes!("native-boundary/library-initializers.lua"),
        b"@library-initializers.lua",
    )
    .unwrap();
    for order in 1..=12 {
        let mut vm = Vm::new_bare();
        let function = vm
            .new_native_function(
                library
                    .clone()
                    .symbol(b"luaopen_library_initializers")
                    .unwrap(),
            )
            .unwrap();
        vm.call(function, &[]).unwrap();
        vm.set_global(b"library_order", Value::Number(f64::from(order)))
            .unwrap();
        let result = vm.execute(&program).unwrap();
        let [Value::String(text)] = result else {
            panic!("comparison transcript")
        };
        let text = *text;
        assert!(
            vm.string_bytes(text)
                .unwrap()
                .starts_with(b"bare\ttrue\ttrue\ttrue\ttrue\n")
        );
    }
}
