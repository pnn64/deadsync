//! Existing cmd preprocessor unit fixtures, executing results on the Rust VM.
use deadlib_lua::{Value, Vm, compile_named};
use deadsync_song_lua::preprocess_lua_cmd_syntax;
#[path = "support/baked_program.rs"]
mod api;
fn index(vm: &Vm, value: Value, index: usize) -> Value {
    let Value::Table(table) = value else {
        panic!("fixture table expected")
    };
    vm.raw_table_get(table, Value::Number(index as f64))
        .unwrap()
}
fn len(vm: &Vm, value: Value) -> usize {
    let Value::Table(table) = value else {
        panic!("fixture table expected")
    };
    vm.raw_table_len(table).unwrap()
}
fn argument(vm: &Vm, calls: Value, call: usize) -> f64 {
    api::as_number(index(vm, index(vm, calls, call), 2))
}

#[test]
fn preprocess_lua_cmd_turns_commands_into_function() {
    assert_eq!(
        preprocess_lua_cmd_syntax("return cmd(diffuse, 1, 0, 0, 1; zoom, 2)").unwrap(),
        "return function(self) self:diffuse(1, 0, 0, 1); self:zoom(2); return self end"
    );
}

#[test]
fn preprocess_lua_cmd_accepts_native_adjacent_commands() {
    let source = r#"return cmd(sleep,0
pulse; effectmagnitude,1,1,1;
value, (2 + 3) * 4 nextvalue, false or 7;
value, function() return {3, 4} end nextvalue, ({9, 10})[2];
value, "literal" nextvalue, [=[long literal]=];
value, math.max(2, 3) nextvalue, .5e+1;
value, (function(x) return x end){11} nextvalue, -2^3;
)"#;
    let expanded = preprocess_lua_cmd_syntax(source).unwrap();
    assert_eq!(expanded.matches('\n').count(), source.matches('\n').count());
    let mut vm = Vm::new();
    let command = api::eval(&mut vm, expanded)[0];
    let actor = api::eval(
        &mut vm,
        r#"
            local a = {calls={}}
            for _, name in ipairs({'sleep','pulse','effectmagnitude','value','nextvalue'}) do
                a[name] = function(self, ...) self.calls[#self.calls+1] = {name, ...} end
            end
            return a
        "#,
    )[0];
    api::call(&mut vm, command, &[actor]);
    let calls = api::get(&mut vm, actor, b"calls");
    assert_eq!(len(&vm, calls), 13);
    assert_eq!(argument(&vm, calls, 1), 0.);
    assert_eq!(len(&vm, index(&vm, calls, 2)), 1);
    assert_eq!(argument(&vm, calls, 4), 20.);
    assert_eq!(argument(&vm, calls, 5), 7.);
    assert_eq!(argument(&vm, calls, 7), 10.);
    assert_eq!(argument(&vm, calls, 11), 5.);
    assert_eq!(argument(&vm, calls, 13), -8.);
}

#[test]
fn preprocess_lua_cmd_accepts_comments_between_commands_and_arguments() {
    let source = r#"return cmd(
            -- Disabled: x, 99; )
            x, --[=[ ignored ; ) ]=] 12;
            -- another command
            settext, "--literal; )";
            settext, [=[--long literal]=];
            y, 34 -- trailing comment
        )"#;
    let mut vm = Vm::new();
    let command = api::eval(&mut vm, preprocess_lua_cmd_syntax(source).unwrap())[0];
    let actor = api::eval(
        &mut vm,
        r#"return {
            x = function(self, value) self.a = value end,
            y = function(self, value) self.b = value end,
            settext = function(self, value) self.label = (self.label or '') .. value end,
        }"#,
    )[0];
    api::call(&mut vm, command, &[actor]);
    assert_eq!(api::get_number(&mut vm, actor, b"a"), 12.);
    assert_eq!(api::get_number(&mut vm, actor, b"b"), 34.);
    assert_eq!(
        api::get_bytes(&mut vm, actor, b"label"),
        b"--literal; )--long literal"
    );
}

#[test]
fn preprocess_lua_cmd_preserves_function_arguments() {
    let source = r#"return cmd(SetUpdateFunction, function(self)
            local count = 0;
            for i = 1, 3 do
                if i == 1 then count = count + i;
                elseif i == 2 then do count = count + i; end
                else count = count + i; end
            end
            while count < 7 do count = count + 1; end
            repeat count = count + 1; until count == 8
            local nested = function() return "function; do; end", count; end;
            local label, result = nested(); self.label = label; self.count = result;
        end; zoom, 2)"#;
    let mut vm = Vm::new();
    let command = api::eval(&mut vm, preprocess_lua_cmd_syntax(source).unwrap())[0];
    let actor = api::eval(
        &mut vm,
        r#"return {
                SetUpdateFunction = function(self, callback) self.callback = callback end,
                zoom = function(self, value) self.scale = value end,
            }"#,
    )[0];
    api::call(&mut vm, command, &[actor]);
    let callback = api::get(&mut vm, actor, b"callback");
    api::call(&mut vm, callback, &[actor]);
    assert_eq!(api::get_number(&mut vm, actor, b"count"), 8.);
    assert_eq!(api::get_number(&mut vm, actor, b"scale"), 2.);
    assert_eq!(
        api::get_bytes(&mut vm, actor, b"label"),
        b"function; do; end"
    );
}

#[test]
fn preprocess_lua_cmd_ignores_strings_and_comments() {
    let source = "local s = \"cmd(zoom, 2)\" -- cmd(diffuse, 1)\nreturn cmd(linear, 1)";

    assert_eq!(
        preprocess_lua_cmd_syntax(source).unwrap(),
        "local s = \"cmd(zoom, 2)\" -- cmd(diffuse, 1)\nreturn function(self) self:linear(1); return self end"
    );
}

#[test]
fn preprocess_lua_cmd_appends_multiple_expansions_and_preserves_surroundings() {
    let source = "local a = cmd(); local b = cmd(zoom, f(1, 2); diffuse, {1, 0, 0, 1}); return cmd(settext, [=[cmd(x); y]=]) -- cmd(noop)";
    assert_eq!(
        preprocess_lua_cmd_syntax(source).unwrap(),
        "local a = function(self) return self end; local b = function(self) self:zoom(f(1, 2)); self:diffuse({1, 0, 0, 1}); return self end; return function(self) self:settext([=[cmd(x); y]=]); return self end -- cmd(noop)"
    );
}

#[test]
fn preprocess_lua_cmd_keeps_errors_after_an_earlier_expansion() {
    for (command, expected) in [
        ("cmd(zoom, 1", "unterminated cmd expression"),
        ("cmd(1zoom, 1)", "invalid cmd command '1zoom, 1'"),
        ("cmd(zoom 1)", "invalid cmd command 'zoom 1'"),
        ("cmd(settext, 'abc)", "unterminated Lua string"),
        ("cmd(settext, [[abc)", "unterminated Lua long string"),
    ] {
        let source = format!("local a = cmd(zoom, 2); return {command}");
        assert_eq!(preprocess_lua_cmd_syntax(&source).unwrap_err(), expected);
    }
}

#[test]
fn preprocess_lua_cmd_reuses_openings_across_whitespace_and_long_strings() {
    let source = "local s = [==[cmd(noop); )]==]; --[=[cmd(noop)]=]\nreturn cmd \t\r\n (settext, [==[x; cmd(noop); )]==]; zoom, 2)";
    assert_eq!(
        preprocess_lua_cmd_syntax(source).unwrap(),
        "local s = [==[cmd(noop); )]==]; --[=[cmd(noop)]=]\nreturn \nfunction(self) self:settext([==[x; cmd(noop); )]==]); self:zoom(2); return self end"
    );
    for source in ["cmd", "cmd  \t", "cmdx(zoom, 1)", "xcmd(zoom, 1)"] {
        assert_eq!(preprocess_lua_cmd_syntax(source).unwrap(), source);
    }
}

#[test]
fn preprocess_lua_cmd_preserves_native_function_source_lines() {
    let source = "local first = cmd\n(\nzoom, 2;\nlinear, 1\n)\nlocal second = function() return 7 end\nreturn {first, second}\n";
    let expanded = preprocess_lua_cmd_syntax(source).unwrap();
    assert_eq!(expanded.matches('\n').count(), source.matches('\n').count());
    let mut vm = Vm::new();
    let code = compile_named(expanded.as_bytes(), b"@cmd-source-lines.lua").unwrap();
    let functions = vm.execute(&code).unwrap()[0];
    vm.set_global(b"fixture_functions", functions).unwrap();
    let info = api::eval(
        &mut vm,
        b"return debug.getinfo(fixture_functions[1],'S'),debug.getinfo(fixture_functions[2],'S')",
    );
    assert_eq!(api::get_number(&mut vm, info[0], b"linedefined"), 2.);
    assert_eq!(api::get_number(&mut vm, info[0], b"lastlinedefined"), 5.);
    assert_eq!(api::get_number(&mut vm, info[1], b"linedefined"), 6.);
    assert_eq!(api::get_number(&mut vm, info[1], b"lastlinedefined"), 6.);
}

#[test]
fn preprocess_lua_cmd_preserves_invalid_comment_openings() {
    for comment in [
        "--[not a long string",
        "--[=not a long string",
        "--[=[unclosed",
    ] {
        let source = format!("{comment}\nreturn cmd(zoom, 2)");
        assert_eq!(
            preprocess_lua_cmd_syntax(&source).unwrap(),
            format!("{comment}\nreturn function(self) self:zoom(2); return self end")
        );
    }
    assert_eq!(
        preprocess_lua_cmd_syntax("return [=[unclosed").unwrap_err(),
        "unterminated Lua long string"
    );
}
