//! Initial correctness-checked, allocation-instrumented reference measurements.
use deadlib_lua::{Value, Vm, compile};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    time::Instant,
};

struct Counted;
struct SourceFile(std::path::PathBuf);
impl Drop for SourceFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;
static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: this benchmark-only allocator forwards pointers/layouts unchanged to
// System. Instrumentation uses allocation-free atomics and never dereferences pointers.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size() as u64, Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size() as u64, Relaxed);
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live pointer, layout and new size.
        let p = unsafe { System.realloc(p, layout, size) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            REALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size as u64, Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies this live System allocation and its layout.
        unsafe { System.dealloc(p, layout) }
    }
}

fn measure(
    mut work: impl FnMut(),
    repeats: u32,
    metric: &str,
    engine: &str,
    name: &str,
    sample: usize,
) {
    ALLOCS.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
    let start = Instant::now();
    for _ in 0..repeats {
        work();
    }
    let ns = start.elapsed().as_nanos() / u128::from(repeats);
    ENABLED.store(false, Relaxed);
    println!(
        "{metric}\t{engine}\t{name}\t{sample}\t{repeats}\t{ns}\t{}\t{}\t{}",
        ALLOCS.load(Relaxed),
        REALLOCS.load(Relaxed),
        BYTES.load(Relaxed)
    );
}

fn main() {
    println!(
        "metric\tengine\tworkload\tsample\trepeats\tns_per_iteration\trust_allocations_total\trust_reallocations_total\trust_bytes_total"
    );
    measure(
        || {
            black_box(Vm::new());
        },
        1000,
        "startup_drop",
        "rust",
        "state",
        0,
    );
    measure(
        || {
            black_box(lua_reference::Lua::new());
        },
        1000,
        "startup_drop",
        "lua51",
        "state",
        0,
    );
    println!(
        "calendar_tz_environment\t{}",
        std::env::var("TZ").unwrap_or_else(|_| "<system>".into())
    );
    let indexed_targets = (1..=16)
        .map(|i| format!("t[{i}]"))
        .collect::<Vec<_>>()
        .join(",");
    let indexed_values = (0..16)
        .map(|i| format!("n+{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let indexed_kernel =
        format!("local t={{}};for n=1,1000 do {indexed_targets}={indexed_values} end;return t[16]");
    for (name, source, expected, repeats) in [
        (
            "numeric_for",
            "local s=0; for i=1,100000 do s=s+i end; return s",
            5000050000.0,
            100,
        ),
        (
            "numeric_while",
            "local i,s=0,0; while i<100000 do i=i+1; s=s+i end; return s",
            5000050000.0,
            100,
        ),
        (
            "operand_reuse",
            "local x=1;local y=0;for i=1,10000 do y=(x+x)+(x+x)+(x+x)+(x+x)end;return y",
            8.0,
            50,
        ),
        (
            "constant_left",
            "local x=1.25;local y=0;for i=1,10000 do y=1+(2*(3+(4*(5+x))))end;return y",
            57.0,
            50,
        ),
        (
            "os_difftime",
            "local diff=os.difftime;local sum=0;for i=1,10000 do sum=sum+diff(i+.75,i-1)end;return sum",
            10000.0,
            50,
        ),
        (
            "os_getenv",
            "local getenv=os.getenv;local sum=0;for i=1,1000 do if getenv('DEADSYNC_LUA_UNSET_7F189225')==nil then sum=sum+1 end end;return sum",
            1000.0,
            30,
        ),
        (
            "os_shell_query",
            "local execute=os.execute;local n=execute();local sum=0;for i=1,100 do if execute()==n then sum=sum+1 end end;return sum",
            100.0,
            10,
        ),
        ("os_execute", "return os.execute('exit 0')", 0.0, 5),
        (
            "os_time",
            "local time=os.time;if not calendar_input then calendar_input={year=2000,month=1,day=1,isdst=false}end;local t=calendar_input;local reference=time(t);local n=0;for i=1,1000 do if time(t)==reference then n=n+1 end end;return n",
            1000.0,
            30,
        ),
        (
            "os_date_format",
            "local date=os.date;local n=0;for i=1,1000 do n=n+#date('!%Y-%m-%d %H:%M:%S',946684800+i)end;return n",
            19000.0,
            30,
        ),
        (
            "os_date_table",
            "local date=os.date;local n=0;for i=1,1000 do local d=date('!*t',946684800+i);n=n+d.year+d.month+d.day end;return n",
            2002000.0,
            20,
        ),
        (
            "os_clock",
            "local clock=os.clock;local last=clock();local n=0;for i=1,10000 do local now=clock(false,nil,1);if now>=last then n=n+1 end;last=now end;return n",
            10000.0,
            30,
        ),
        (
            "os_rename",
            "local rename=os.rename;for i=1,32 do assert(rename(rename_from,rename_to));assert(rename(rename_to,rename_from))end;return 32",
            32.0,
            5,
        ),
        (
            "call_table",
            "local function f(t)return t end;local sum=0;for i=1,1000 do sum=sum+f({key=i}).key end;return sum",
            500500.0,
            30,
        ),
        ("indexed_assignment", indexed_kernel.as_str(), 1015.0, 30),
        (
            "array_table",
            "local t={}; for i=1,10000 do t[i]=i end; local s=0; for i=1,10000 do s=s+t[i] end; return s",
            50005000.0,
            50,
        ),
        (
            "recursive_fib",
            "local function f(n) if n<2 then return n end; return f(n-1)+f(n-2) end; return f(20)",
            6765.0,
            30,
        ),
        (
            "closure_calls",
            "local x=0; local function f(n) x=x+n end; for i=1,10000 do f(i) end; return x",
            50005000.0,
            50,
        ),
        (
            "byte_concat",
            "local s=''; for i=1,1000 do s=s..'x' end; return #s",
            1000.0,
            30,
        ),
        (
            "pattern_find",
            "local s=0;for i=1,1000 do local a,b=string.find('prefix abc123 xyz','(%a+)(%d+)');s=s+a+b end;return s",
            21000.0,
            30,
        ),
        (
            "pattern_replace",
            "local s=0;for i=1,1000 do local r,n=string.gsub('abc 123 def 456','(%a+)','[%1]');s=s+#r+n end;return s",
            21000.0,
            30,
        ),
        (
            "pattern_callback",
            "local s=0;for i=1,1000 do local r,n=string.gsub('abc 123 def 456','%a+',string.upper);s=s+#r+n end;return s",
            17000.0,
            30,
        ),
        (
            "string_format",
            "local s=0;for i=1,1000 do s=s+#string.format('%08d:%.3f',i,i/1000) end;return s",
            14000.0,
            30,
        ),
        (
            "table_sort",
            "local s=0;for k=1,100 do local t={};for i=1,128 do t[i]=129-i end;table.sort(t);s=s+t[1]+t[128] end;return s",
            12900.0,
            20,
        ),
        (
            "table_sort_callback",
            "local s=0;local function less(a,b)return a<b end;for k=1,100 do local t={};for i=1,128 do t[i]=129-i end;table.sort(t,less);s=s+t[1]+t[128] end;return s",
            12900.0,
            20,
        ),
        (
            "coroutine_resume",
            "local co=coroutine.create(function()for i=1,1000 do coroutine.yield(i) end;return 0 end);local s=0;for i=1,1001 do local ok,n=coroutine.resume(co);assert(ok);s=s+n end;return s",
            500500.0,
            30,
        ),
        (
            "coroutine_wrap",
            "local f=coroutine.wrap(function()for i=1,1000 do coroutine.yield(i) end;return 0 end);local s=0;for i=1,1001 do s=s+f() end;return s",
            500500.0,
            30,
        ),
        (
            "coroutine_deep_yield",
            "local function f(n)if n==0 then for i=1,1000 do coroutine.yield(i) end;return 0 end;return f(n-1)+0 end;local c=coroutine.create(function()return f(128)end);local s=0;for i=1,1001 do local ok,n=coroutine.resume(c);assert(ok);s=s+n end;return s",
            500500.0,
            30,
        ),
        (
            "coroutine_reuse",
            "if not cachedco then cachedco=coroutine.create(function()while true do coroutine.yield(1) end end) end;local s=0;for i=1,1000 do local ok,n=coroutine.resume(cachedco);assert(ok);s=s+n end;return s",
            1000.0,
            30,
        ),
        (
            "gc_cycles",
            "local s=0;for i=1,5000 do local t={i};t.self=t;s=s+t[1] end;collectgarbage();return s",
            12502500.0,
            20,
        ),
        (
            "gc_weak_values",
            "local w=setmetatable({},{__mode='v'});for i=1,3000 do w[i]={i} end;collectgarbage();return next(w)==nil and 1 or 0",
            1.0,
            20,
        ),
        (
            "userdata_proxy",
            "local p=newproxy(true);local m=getmetatable(p);local n=0;for i=1,5000 do local u=newproxy(p);if getmetatable(u)==m then n=n+1 end end;collectgarbage();return n",
            5000.0,
            20,
        ),
        (
            "userdata_finalize",
            "collectgarbage('stop');local n=0;local p=newproxy(true);getmetatable(p).__gc=function()n=n+1 end;for i=1,3000 do newproxy(p)end;p=nil;collectgarbage();collectgarbage('restart');return n",
            3001.0,
            20,
        ),
        (
            "gc_restart",
            "collectgarbage('stop');local weak=setmetatable({},{__mode='v'});local keep={};for i=1,1000 do local t={};weak[i]=t;keep[i]=t end;collectgarbage();keep=nil;for i=1,10000 do local t={}end;return next(weak)==nil and 1 or 0",
            1.0,
            10,
        ),
        (
            "gc_steps",
            "collectgarbage('stop');local keep={};for i=1,256 do keep[i]={i}end;for i=1,2048 do local t={}end;local step=collectgarbage;for i=1,64 do step('step',0)end;local n=0;for i=1,256 do n=n+keep[i][1]end;collectgarbage();return n",
            32896.0,
            20,
        ),
        (
            "gc_mark_mutation",
            "collectgarbage('stop');local roots={};local weak=setmetatable({},{__mode='v'});for i=1,512 do roots[i]={i};weak[i]={i}end;collectgarbage('step',0);for i=1,512 do local v=weak[i] or{i};roots[i].child=v;if i%8==0 then collectgarbage('step',0)end end;collectgarbage();local n=0;for i=1,512 do n=n+roots[i].child[1]end;return n",
            131328.0,
            10,
        ),
        (
            "locale_numeric",
            "local old=os.setlocale(nil,'numeric');assert(os.setlocale('ptb','numeric'));local n=0;for i=1,1000 do local v=tonumber('12,5');if string.format('%.2f',v)=='12,50'then n=n+1 end end;assert(os.setlocale(old,'numeric'));return n",
            1000.0,
            10,
        ),
        (
            "locale_ctype",
            "local old=os.setlocale(nil,'ctype');assert(os.setlocale('ptb','ctype'));local s=string.char(233,225,65,97);local n=0;for i=1,1000 do n=n+#string.upper(s);n=n+select(2,string.gsub(s,'%a','x'))end;assert(os.setlocale(old,'ctype'));return n",
            8000.0,
            10,
        ),
        (
            "binary_header_reject",
            "local bad='\\27Lua'..string.char(0,0,1,4,8,4,8,0);local n=0;for i=1,1000 do local f,e=loadstring(bad,'=binary');if f==nil and e=='binary: bad header in precompiled chunk'then n=n+1 end end;return n",
            1000.0,
            10,
        ),
        (
            "reader_load_single",
            "local sum=0;for i=1,200 do local once=false;local f=load(function()if once then return nil end;once=true;return 'return 1+2'end);sum=sum+f()end;return sum",
            600.0,
            10,
        ),
        (
            "reader_load_bytes",
            "local source='local t={x=7};return t.x*3';local sum=0;for i=1,100 do local at=0;local f=load(function()at=at+1;if at>#source then return nil end;return string.sub(source,at,at)end);sum=sum+f()end;return sum",
            2100.0,
            10,
        ),
        (
            "debug_info",
            "local function f(n)return n+1 end;local sum=0;for i=1,1000 do local d=debug.getinfo(f,'SufL');sum=sum+d.linedefined+d.func(1)end;return sum",
            3000.0,
            10,
        ),
        (
            "debug_locals",
            "local function f(n)local _,v=debug.getlocal(1,1);return v end;local sum=0;for i=1,1000 do sum=sum+f(i)end;return sum",
            500500.0,
            10,
        ),
        (
            "debug_traceback",
            r#"local function f(n)return debug.traceback(n,1)end;local suffix="\nstack traceback:\n\t(traceback-bench):1: in function 'f'\n\t(traceback-bench):1: in main chunk";local sum=0;for i=1,200 do local text=f(i);if text==i..suffix then sum=sum+1 end end;return sum"#,
            200.0,
            10,
        ),
        (
            "debug_hook",
            r#"local lines=0;local f=assert(loadstring("local sum=0\nfor i=1,100 do\nsum=sum+i\nend\nreturn sum\n"));debug.sethook(function(e,l)lines=lines+l end,"l");local sum=f();debug.sethook();return sum+lines"#,
            5558.0,
            10,
        ),
        (
            "io_tmpfile",
            r#"local f=assert(io.tmpfile());for i=1,64 do assert(f:write(i,'\n'))end;assert(f:seek('set',0)==0);local sum=0;for line in f:lines()do sum=sum+tonumber(line)end;assert(f:close());return sum"#,
            2080.0,
            10,
        ),
        (
            "loadfile_text",
            "local f=assert(loadfile(loadfile_path));return f()",
            2080.0,
            10,
        ),
        (
            "require_cached",
            "local m=require('math');local sum=0;for i=1,1000 do if require('math')==m then sum=sum+1 end end;return sum",
            1000.0,
            10,
        ),
        (
            "require_preload",
            "package.preload.module_bench=function(n)return {name=n}end;local sum=0;for i=1,100 do package.loaded.module_bench=nil;local m=require('module_bench');if m.name=='module_bench' then sum=sum+1 end end;return sum",
            100.0,
            10,
        ),
        (
            "syntax_error",
            r#"local sum=0;for i=1,100 do local f,e=loadstring("local x={1\n\n","=syntax-bench");if f==nil and e=="syntax-bench:3: '}' expected (to close '{' at line 1) near '<eof>'" then sum=sum+1 end end;return sum"#,
            100.0,
            10,
        ),
        (
            "runtime_error",
            r#"local function f()local target=false;return target+1 end;local sum=0;for i=1,100 do local ok,e=pcall(f);if not ok and e=="runtime-bench:1: attempt to perform arithmetic on local 'target' (a boolean value)" then sum=sum+1 end end;return sum"#,
            100.0,
            10,
        ),
        (
            "native_arg_error",
            r#"local function f()return math.sin(false)end;local sum=0;for i=1,100 do local ok,e=pcall(f);if not ok and e=="native-bench:1: bad argument #1 to 'sin' (number expected, got boolean)" then sum=sum+1 end end;return sum"#,
            100.0,
            10,
        ),
        (
            "stack_overflow",
            r#"collectgarbage('stop');local n=0;local function f()n=n+1;f()end;local ok,e=pcall(f);return not ok and n>10000 and e=="stack-bench:1: stack overflow" and 1 or 0"#,
            1.0,
            3,
        ),
        (
            "capture_limit",
            r#"local vars={};for i=1,61 do vars[i]='a'..i end;local s='local '..table.concat(vars,',')..'\nlocal function f()\n';for i=1,61 do s=s..'a'..i..'=1\n' end;local sum=0;for i=1,100 do local f,e=loadstring(s,'=capture-bench');if f==nil and e=='capture-bench:63: function at line 2 has more than 60 upvalues' then sum=sum+1 end end;return sum"#,
            100.0,
            10,
        ),
        (
            "native_chunk_numeric",
            "local n=0;for i=1,20000 do n=n+i end;return n",
            200010000.0,
            100,
        ),
        (
            "native_chunk_captures",
            "local x=0;local function f(n)x=x+n end;for i=1,2000 do f(i)end;return x",
            2001000.0,
            100,
        ),
        (
            "native_chunk_tables",
            "local t={1,2,3,4,5};for i=6,500 do t[i]=i end;local n=0;for k,v in ipairs(t)do n=n+v end;return n",
            125250.0,
            100,
        ),
    ] {
        let native;
        let source = if name.starts_with("native_chunk_") {
            let lua = lua_reference::Lua::new();
            native = lua
                .load(source)
                .set_name("=(native bench)")
                .into_function()
                .unwrap()
                .dump(false);
            native.as_slice()
        } else {
            source.as_bytes()
        };
        let chunk_name = match name {
            "debug_traceback" => Some("=(traceback-bench)"),
            "runtime_error" => Some("=runtime-bench"),
            "native_arg_error" => Some("=native-bench"),
            "stack_overflow" => Some("=stack-bench"),
            _ => None,
        };
        let program = match chunk_name {
            Some(name) => deadlib_lua::compile_named(source, name.as_bytes()),
            None => compile(source),
        }
        .unwrap();
        println!(
            "rust_program_layout\t{name}\t{}\t{}",
            program.instruction_count(),
            program.register_count()
        );
        let mut vm = Vm::new();
        let rename_files = (name == "os_rename").then(|| {
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/lua-bench-fixtures");
            std::fs::create_dir_all(&directory).unwrap();
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let from =
                SourceFile(directory.join(format!("rename-{}-{stamp}.from", std::process::id())));
            let to =
                SourceFile(directory.join(format!("rename-{}-{stamp}.to", std::process::id())));
            std::fs::write(&from.0, b"rename fixture").unwrap();
            println!("rename_from_path_bytes\t{}", from.0.as_os_str().len());
            println!("rename_to_path_bytes\t{}", to.0.as_os_str().len());
            for (key, path) in [(b"rename_from".as_slice(), &from.0), (b"rename_to", &to.0)] {
                let value = vm.intern(path.to_str().unwrap().as_bytes());
                vm.set_global(key, value).unwrap();
            }
            [from, to]
        });
        let source_file = (name == "loadfile_text").then(|| {
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/lua-bench-fixtures");
            std::fs::create_dir_all(&directory).unwrap();
            let file = SourceFile(directory.join(format!("loadfile-{}.lua", std::process::id())));
            let source = format!(
                "#!lua\n--[=[{}]=]\nlocal sum=0;for i=1,64 do sum=sum+i end;return sum",
                "x".repeat(4096)
            );
            std::fs::write(&file.0, source).unwrap();
            let path = vm.intern(file.0.to_str().unwrap().as_bytes());
            vm.set_global(b"loadfile_path", path).unwrap();
            file
        });
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(expected)]);
        let reference = if name.starts_with("debug_") {
            // SAFETY: only these fixed developer workloads need the debug library.
            unsafe {
                lua_reference::Lua::unsafe_new_with(
                    lua_reference::StdLib::ALL_SAFE | lua_reference::StdLib::DEBUG,
                    lua_reference::LuaOptions::default(),
                )
            }
        } else {
            lua_reference::Lua::new()
        };
        if let Some(file) = &source_file {
            reference
                .globals()
                .set("loadfile_path", file.0.to_str().unwrap())
                .unwrap();
        }
        if let Some([from, to]) = &rename_files {
            reference
                .globals()
                .set("rename_from", from.0.to_str().unwrap())
                .unwrap();
            reference
                .globals()
                .set("rename_to", to.0.to_str().unwrap())
                .unwrap();
        }
        let chunk = reference.load(source);
        let function = if let Some(name) = chunk_name {
            chunk.set_name(name)
        } else {
            chunk
        }
        .into_function()
        .unwrap();
        assert_eq!(function.call::<f64>(()).unwrap(), expected);
        measure(
            || {
                black_box(
                    match chunk_name {
                        Some(name) => {
                            deadlib_lua::compile_named(black_box(source), name.as_bytes())
                        }
                        None => compile(black_box(source)),
                    }
                    .unwrap(),
                );
            },
            1000,
            "compile_drop",
            "rust",
            name,
            0,
        );
        measure(
            || {
                let chunk = reference.load(black_box(source));
                black_box(
                    if let Some(name) = chunk_name {
                        chunk.set_name(name)
                    } else {
                        chunk
                    }
                    .into_function()
                    .unwrap(),
                );
            },
            1000,
            "compile_drop",
            "lua51",
            name,
            0,
        );
        for sample in 0..5 {
            if sample % 2 == 0 {
                measure(
                    || {
                        black_box(vm.execute(black_box(&program)).unwrap());
                    },
                    repeats,
                    "execute",
                    "rust",
                    name,
                    sample,
                );
                measure(
                    || {
                        black_box(function.call::<f64>(()).unwrap());
                    },
                    repeats,
                    "execute",
                    "lua51",
                    name,
                    sample,
                );
            } else {
                measure(
                    || {
                        black_box(function.call::<f64>(()).unwrap());
                    },
                    repeats,
                    "execute",
                    "lua51",
                    name,
                    sample,
                );
                measure(
                    || {
                        black_box(vm.execute(black_box(&program)).unwrap());
                    },
                    repeats,
                    "execute",
                    "rust",
                    name,
                    sample,
                );
            }
        }
        println!(
            "reference_live_heap_bytes\t{name}\t{}",
            reference.used_memory()
        );
        for (phase, stats) in [
            ("before_collect", vm.heap_stats()),
            ("after_collect", vm.collect_garbage().unwrap()),
        ] {
            println!(
                "rust_heap_estimate\t{name}\t{phase}\t{}\t{}\t{}\t{}\t{}\t{}",
                stats.live_bytes,
                stats.reserved_bytes,
                stats.peak_reserved_bytes,
                stats.collections,
                stats.reclaimed_objects,
                stats.userdata
            );
        }
        reference.gc_collect().unwrap();
        println!(
            "reference_after_collect_heap_bytes\t{name}\t{}",
            reference.used_memory()
        );
    }
    println!("value_size_bytes\t{}", std::mem::size_of::<Value>());
    println!("debug_reference_libraries\tALL_SAFE plus DEBUG for debug_* workloads only");
    // Source bytes and initial correctness checks remain outside compile timing.
    let mut frontends = Vec::with_capacity(22);
    for (size, repeats) in [(10_000, 30), (100_000, 10), (1_000_000, 3)] {
        for quoted in [false, true] {
            let name = format!(
                "frontend_{}_{}",
                if quoted { "quoted" } else { "comment" },
                size
            );
            let text = "x".repeat(size);
            let (source, expected) = if quoted {
                (format!("local s='{text}';return #s"), size as f64)
            } else {
                (format!("--[=[{text}]=]\nreturn 7"), 7.0)
            };
            frontends.push((name, source, expected, repeats));
        }
    }
    for (scopes, repeats) in [(1000, 5), (10000, 2)] {
        let source = format!(
            "local function consume(x,y)return x end;\n{}return 7",
            "do local x=1;consume(x,consume(x,0))end;\n".repeat(scopes)
        );
        frontends.push((format!("frontend_scopes_{scopes}"), source, 7.0, repeats));
        let source = format!(
            "{}return 7",
            "do local a,b=1,2;local function f()return a+b end end;\n".repeat(scopes)
        );
        frontends.push((format!("frontend_captures_{scopes}"), source, 7.0, repeats));
    }
    let mut source = String::from("local root=7\n");
    for index in 1..=190 {
        source.push_str(&format!("local function f{index}()\n"));
    }
    source.push_str("root=root+1;return root\n");
    for index in (1..=190).rev() {
        source.push_str("end\n");
        if index > 1 {
            source.push_str(&format!("return f{index}()\n"));
        }
    }
    source.push_str("return f1()");
    frontends.push(("frontend_functions_190".into(), source, 8.0, 20));
    let source = format!(
        "local n=0;local function f()n=n+1;return n end;local value={};return value",
        vec!["f()"; 20_000].join(" and ")
    );
    frontends.push(("frontend_logical_20000".into(), source, 20_000.0, 3));
    let source = format!("local x=1.25;return {}", vec!["x"; 10_000].join("+"));
    frontends.push(("frontend_arithmetic_10000".into(), source, 12_500.0, 3));
    let source = format!(
        "local t={{}};t.a=t;return t{}==t and 1 or 0",
        ".a".repeat(10_000)
    );
    frontends.push(("frontend_index_10000".into(), source, 1.0, 3));
    let source = format!(
        "local t={}7{};local n=0;while type(t)=='table' do n=n+1;t=t.key end;return n",
        "{key=".repeat(190),
        "}".repeat(190)
    );
    frontends.push(("frontend_constructor_190".into(), source, 190.0, 20));
    let source = format!(
        "local n=0;local function f(a)n=n+(a or 1);return f end;local result=f{};return n",
        "(1)".repeat(10_000)
    );
    frontends.push(("frontend_call_chain_10000".into(), source, 10_000.0, 3));
    let source = format!(
        "local n=0;local obj={{}};function obj:again(x)n=n+x;return self end;local result=obj{};return n",
        ":again(1)".repeat(10_000)
    );
    frontends.push(("frontend_method_chain_10000".into(), source, 10_000.0, 3));
    let names = (1..=199)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let args = (1..=247)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let source = format!("local function f(...)return ... end;local {names}=f({args});return a199");
    frontends.push(("frontend_results_199".into(), source, 199.0, 30));
    let targets = (1..=150)
        .map(|i| format!("t[{i}]"))
        .collect::<Vec<_>>()
        .join(",");
    let values = (1..=150)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let source = format!("local t={{}};{targets}={values};return t[150]");
    frontends.push(("frontend_indexed_targets_150".into(), source, 150.0, 30));
    let pad = (1..=253)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "do local pad={{true,false,nil}}end;do local pad={{{pad}}}end;local t={{}};{targets}={values};return t[150]"
    );
    frontends.push(("frontend_compact_pool_150".into(), source, 150.0, 30));
    let names = (1..=100)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!("local {names};local x=1;return {}x", "2^".repeat(190));
    frontends.push(("frontend_right_power_190".into(), source, f64::INFINITY, 30));
    let source = format!("local {names};local x=1;return {}x", "'2'^".repeat(190));
    frontends.push((
        "frontend_right_string_power_190".into(),
        source,
        f64::INFINITY,
        30,
    ));
    for (name, source, expected, repeats) in frontends {
        let program = deadlib_lua::compile_named(source.as_bytes(), b"=(frontend)").unwrap();
        println!(
            "rust_program_layout\t{name}\t{}\t{}",
            program.instruction_count(),
            program.register_count()
        );
        let mut vm = Vm::new();
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(expected)]);
        let reference = lua_reference::Lua::new();
        let function = reference
            .load(source.as_bytes())
            .set_name("=(frontend)")
            .into_function()
            .unwrap();
        assert_eq!(function.call::<f64>(()).unwrap(), expected);
        for sample in 0..5 {
            for engine in if sample % 2 == 0 {
                ["rust", "lua51"]
            } else {
                ["lua51", "rust"]
            } {
                if engine == "rust" {
                    measure(
                        || {
                            black_box(
                                deadlib_lua::compile_named(
                                    black_box(source.as_bytes()),
                                    b"=(frontend)",
                                )
                                .unwrap(),
                            );
                        },
                        repeats,
                        "compile_drop",
                        engine,
                        &name,
                        sample,
                    );
                } else {
                    measure(
                        || {
                            black_box(
                                reference
                                    .load(black_box(source.as_bytes()))
                                    .set_name("=(frontend)")
                                    .into_function()
                                    .unwrap(),
                            );
                        },
                        repeats,
                        "compile_drop",
                        engine,
                        &name,
                        sample,
                    );
                }
            }
        }
    }
    println!(
        "allocation_scope\tRust GlobalAlloc only; native C malloc excluded; totals cover all iterations in each row"
    );
    println!(
        "limitations\tIncremental mark frontier and slot sweeping; individual object scans, final root/weak work, cache rebuilding and callback batches remain atomic; heap numbers are capacity-based estimates, not RSS or exact allocator peaks; startup opens different libraries; execution includes host entry/return; compile includes drop; instrumented timings are preliminary"
    );
}
