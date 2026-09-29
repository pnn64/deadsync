//! Preserve Lua 5.1 number coercion in song expressions executed by Lua 5.4.

use full_moon::{
    ast::{BinOp, Expression},
    node::Node,
    visitors::Visitor,
};
use mlua::{Function, Lua};

pub(crate) fn install_concat(lua: &Lua) -> mlua::Result<()> {
    // Lua 5.4's builtin concatenation does not consult __tostring and adds
    // ".0" to integral floats. ITGmania uses Lua 5.1's %.14g conversion.
    let concat = lua
        .load(
            r#"
        local type, format = type, string.format
        return function(left, right)
            local lt, rt = type(left), type(right)
            if (lt == 'number' or lt == 'string') and
               (rt == 'number' or rt == 'string') then
                if lt == 'number' then left = format('%.14g', left) end
                if rt == 'number' then right = format('%.14g', right) end
            end
            return left .. right
        end
    "#,
        )
        .eval::<Function>()?;
    lua.globals().set("__songlua_concat", concat)
}

#[derive(Default)]
struct ConcatEdits(Vec<(usize, usize, &'static str)>);

impl Visitor for ConcatEdits {
    fn visit_expression(&mut self, expression: &Expression) {
        if let Expression::BinaryOperator {
            lhs,
            binop: BinOp::TwoDots(token),
            rhs,
        } = expression
        {
            // Node::end_position can exclude a closing index bracket. Use the
            // complete token extent, including every contained delimiter.
            let start = lhs
                .tokens()
                .map(|token| token.token().start_position().bytes())
                .min()
                .expect("parsed left operand has tokens");
            let end = rhs
                .tokens()
                .map(|token| token.token().end_position().bytes())
                .max()
                .expect("parsed right operand has tokens");
            self.0.push((start, start, "__songlua_concat("));
            self.0.push((
                token.token().start_position().bytes(),
                token.token().end_position().bytes(),
                ",(",
            ));
            self.0.push((end, end, "))"));
        }
    }
}

pub(crate) fn preprocess_source(source: &str) -> Result<String, String> {
    let source = crate::preprocess_lua_cmd_syntax(source)?;
    if !source.contains("..") {
        return Ok(source);
    }
    // An AST preserves precedence, right associativity, comments, strings,
    // varargs, and nested function bodies without guessing operand boundaries.
    let ast = full_moon::parse(&source).map_err(|errors| {
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let mut edits = ConcatEdits::default();
    edits.visit_ast(&ast);
    if edits.0.is_empty() {
        return Ok(source);
    }
    edits.0.sort_by_key(|&(start, end, _)| (start, end));
    let mut out = String::with_capacity(source.len() + edits.0.len() * 24);
    // Capture before setfenv/_ENV changes, without adding a source line.
    let mut cursor = if source.starts_with("#!") {
        source.find('\n').map_or(source.len(), |end| end + 1)
    } else {
        0
    };
    out.push_str(&source[..cursor]);
    out.push_str("local __songlua_concat = __songlua_concat; ");
    for (start, end, replacement) in edits.0 {
        out.push_str(&source[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(&source[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_preserves_lua51_numbers_and_expressions() {
        let lua = Lua::new();
        install_concat(&lua).expect("install concatenation");
        let source = r#"
            local function many() return 5.0, 99 end
            assert(5.0 .. ' centered' == '5 centered')
            assert('x' .. -5.0 == 'x-5')
            assert(1.25 .. 5.0 == '1.255')
            assert('x' .. 2 + 3 == 'x5')
            assert('x' .. many() == 'x5')
            assert('x' .. (function(...) return ... end)(5.0) == 'x5')
            assert('x' .. -- keep this comment and newline
                5.0 == 'x5')
            assert('..' .. [=[...]=] == '.....')
            local values = {5.0, 'dark'}
            assert(values[1] .. ' ' .. values[2] == '5 dark')
            assert('x' .. ({5.0})[1] == 'x5')
            local object = setmetatable({}, {__concat = function(a,b)
                return type(a) .. ':' .. type(b)
            end})
            assert(5.0 .. object == 'number:table')
            assert(object .. 5.0 == 'table:number')
            assert(not pcall(function() return true .. 'x' end))
            assert(1 .. 2 .. object == '1number:table')
        "#;
        // The source is rewritten once, including nested metamethod bodies.
        lua.load(preprocess_source(source).expect("preprocess expressions"))
            .exec()
            .expect("Lua 5.1 concatenation");
    }

    #[test]
    fn concat_keeps_lines_and_ignores_literals() {
        let untouched = "return function(...) return '..', [=[..]=], ... end -- ..\n";
        assert_eq!(
            preprocess_source(untouched).expect("valid source"),
            untouched
        );
        let source = "return ('a'..5.0).. -- comment\n ('b' .. 2.5)\n";
        let rewritten = preprocess_source(source).expect("valid concatenation");
        assert_eq!(source.lines().count(), rewritten.lines().count());
        let lua = Lua::new();
        install_concat(&lua).expect("install concatenation");
        assert_eq!(
            lua.load(&rewritten).eval::<String>().expect("evaluate"),
            "a5b2.5"
        );
    }
}
