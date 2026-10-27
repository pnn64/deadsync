use std::{
    io::Write,
    process::{Command, Stdio},
};

// Expected bytes were checked against the unmodified Lua 5.1.5 CLI. The
// independent process audit also compares these operations with the C engine.
fn check(source: &str, input: &[u8], stdout: &[u8], stderr: &[u8]) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_deadlua"))
        .args(["-e", source])
        .env_remove("LUA_INIT")
        .env_remove("LUA_PATH")
        .env_remove("LUA_CPATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "{result:?}");
    let text = |bytes: &[u8]| {
        let mut translated = Vec::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            if cfg!(windows) && byte == b'\n' {
                translated.push(b'\r');
            }
            translated.push(byte);
        }
        translated
    };
    assert_eq!(result.stdout, text(stdout));
    assert_eq!(result.stderr, text(stderr));
}

#[test]
fn commands_mutate_suspended_caller_locals_and_return_no_results() {
    check(
        "local x=17;local n=select('#',debug.debug('kept',false));print('after',n,x,shared)",
        b"shared=42\nlocal n,v=debug.getlocal(3,1);print(n,v);debug.setlocal(3,1,19)\ncont\n",
        b"x\t17\nafter\t0\t19\t42\n",
        b"lua_debug> lua_debug> lua_debug> ",
    );
}

#[test]
fn syntax_and_runtime_errors_recover_without_outer_handlers() {
    check("local n=0;print(xpcall(function()debug.debug();return 'done'end,function(e)n=n+1;return 'wrong'end));print('handlers',n)",
        b"local =\nerror('failed',0)\nerror(17,0)\nprint('alive')\ncont\n",
        b"alive\ntrue\tdone\nhandlers\t0\n",
        b"lua_debug> (debug command):1: '<name>' expected near '='\nlua_debug> failed\nlua_debug> 17\nlua_debug> lua_debug> ");
    check(
        "print(xpcall(function()debug.debug()end,function()return 'wrong'end))",
        b"local f,e=load(function()error('reader',0)end);print(f,e)\ncont\n",
        b"nil\treader\ntrue\n",
        b"lua_debug> lua_debug> ",
    );
}

#[test]
fn nested_sessions_resume_each_command_in_order() {
    check(
        "debug.debug();print('done',n)",
        b"n=1;debug.debug();n=n+10\nn=n+2\ncont\nprint(n)\ncont\n",
        b"13\ndone\t13\n",
        b"lua_debug> lua_debug> lua_debug> lua_debug> lua_debug> ",
    );
}

#[test]
fn original_stdin_is_shared_with_lua_reads() {
    check(
        "io.stdin=nil;io.stderr=nil;collectgarbage();debug.debug();print('done')",
        b"print(io.read())\nconsumed\ncont\n",
        b"consumed\ndone\n",
        b"lua_debug> lua_debug> ",
    );
}

#[test]
fn fgets_fragments_and_c_strings_keep_their_boundaries() {
    let mut input = b"--".to_vec();
    input.extend_from_slice(&[b'x'; 247]);
    input.extend_from_slice(b"print('fragment')\ncont\n");
    check(
        "debug.debug()",
        &input,
        b"fragment\n",
        b"lua_debug> lua_debug> lua_debug> ",
    );
    check(
        "debug.debug();print('done')",
        b"print('before')\0error('ignored')\ncont\n",
        b"before\ndone\n",
        b"lua_debug> lua_debug> ",
    );
    check(
        "debug.debug();print('done')",
        b"cont",
        b"done\n",
        b"lua_debug> (debug command):1: '=' expected near '<eof>'\nlua_debug> ",
    );
    check(
        "debug.debug();print('done')",
        b"=17\ncont\n",
        b"done\n",
        b"lua_debug> (debug command):1: unexpected symbol near '='\nlua_debug> ",
    );
}

#[test]
fn initial_arguments_stop_rooting_values_after_the_first_command() {
    check("weak=setmetatable({},{__mode='v'});local function arg()local t={};weak[1]=t;return t end;debug.debug(arg());print('done')",
        b"collectgarbage();print(weak[1]~=nil,debug.getlocal(2,1)~=nil)\ncollectgarbage();print(weak[1]==nil,debug.getlocal(2,1)==nil)\ncont\n",
        b"true\ttrue\ntrue\ttrue\ndone\n", b"lua_debug> lua_debug> lua_debug> ");
}

#[test]
fn return_hooks_observe_the_native_stack_after_command_cleanup() {
    check("local fn=debug.debug;debug.sethook(function(e)if debug.getinfo(2,'f').func==fn then local n,v=debug.getlocal(2,1);print(e,n,type(v),type(v)=='number' and v or '')end end,'cr');fn(17);fn(19);debug.sethook()",
        b"print(debug.getlocal(2,1))\nprint(debug.getlocal(2,1))\ncont\ncont\n",
        b"call\t(*temporary)\tnumber\t17\n(*temporary)\t17\nnil\nreturn\t(*temporary)\ttable\t\ncall\t(*temporary)\tnumber\t19\nreturn\t(*temporary)\tnumber\t19\n",
        b"lua_debug> lua_debug> lua_debug> lua_debug> ");
}

#[test]
fn commands_use_the_thread_environment_and_cannot_yield_through_debug() {
    check(
        "local g=_G;setfenv(0,setmetatable({answer=42},{__index=g}));debug.debug()",
        b"print(answer,debug.getinfo(1,'S').source)\ncont\n",
        b"42\t=(debug command)\n",
        b"lua_debug> lua_debug> ",
    );
    check(
        "local co=coroutine.create(function()debug.debug();return 42 end);print(coroutine.resume(co))",
        b"coroutine.yield(17)\nprint('recovered')\ncont\n",
        b"recovered\ntrue\t42\n",
        b"lua_debug> attempt to yield across metamethod/C-call boundary\nlua_debug> lua_debug> ",
    );
}
