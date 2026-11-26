//! Reuse the parser state of a closed method callback after checking every
//! source byte and the equality classes of its numeric arguments.
use super::*;

const MAX_ARGS: usize = 8;
const MAX_SHAPES: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
struct Argument {
    number: f64,
    negative: bool,
    // Offsets in the checked body. An empty range leaves this token fixed.
    start: u16,
    end: u16,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::parser::resume) struct NumericLeaf {
    parsed: ParsedLeaf,
    first_expr: ExprId,
    arguments: [Argument; MAX_ARGS],
    argc: usize,
}

fn call_body(ast: &Ast, body: Span) -> Option<(StmtId, ExprId)> {
    let &[statement] = ast.ids(body) else {
        return None;
    };
    let Stmt::Call(call) = ast.stmts[statement].0 else {
        return None;
    };
    Some((statement, call))
}

fn local_method_call(ast: &Ast, mut call: ExprId) -> bool {
    for _ in 0..4 {
        let Expr::Method(receiver, Lexeme::Source { .. }, args, _) = ast.exprs[call].0 else {
            return false;
        };
        for &arg in ast.ids(args) {
            let number = match ast.exprs[arg].0 {
                Expr::Number(n) => n,
                Expr::Unary(Kind::Minus, id) => {
                    let Expr::Number(n) = ast.exprs[id].0 else {
                        return false;
                    };
                    n
                }
                _ => return false,
            };
            if !number.is_finite() || number == 0.0 {
                return false;
            }
        }
        let (receiver_expr, span) = &ast.exprs[receiver];
        if matches!(receiver_expr, Expr::Name(Lexeme::Source { .. })) {
            return span.binding.local_index() == Some(0);
        }
        call = receiver;
    }
    false
}

impl NumericLeaf {
    fn new(ast: &Ast, input: &[u8], first_expr: ExprId, parsed: ParsedLeaf) -> Option<Self> {
        let Expr::Function {
            params,
            vararg: false,
            body,
            ..
        } = ast.exprs[parsed.id].0
        else {
            return None;
        };
        let (_, call) = call_body(ast, body)?;
        if params.len() != 1 || parsed.zero_sign.is_some() || !local_method_call(ast, call) {
            return None;
        }
        if !ast.exprs[first_expr..parsed.id].iter().all(|(expr, _)| {
            matches!(
                expr,
                Expr::Name(Lexeme::Source { .. })
                    | Expr::Method(..)
                    | Expr::Number(_)
                    | Expr::Unary(Kind::Minus, _)
            )
        }) {
            return None;
        }
        let mut numbers =
            ast.exprs[first_expr..parsed.id]
                .iter()
                .filter_map(|(expr, _)| match *expr {
                    Expr::Number(n) => Some(n),
                    _ => None,
                });
        let [start, end] = parsed.offsets.map(|offset| offset as usize);
        let text = &input[start..end];
        if !text.is_ascii() {
            return None;
        }
        let mut scanner = Scanner::default();
        let mut token = Token::default();
        let mut previous = Kind::Eof;
        let mut arguments = [Argument::default(); MAX_ARGS];
        let mut argc = 0;
        loop {
            let before = scanner.position();
            scanner.next_closed(text, &mut token).ok()?;
            if token.kind == Kind::Eof {
                break;
            }
            if token.kind == Kind::Number {
                let number = numbers.next()?;
                if token.number != number {
                    return None;
                }
                let end = scanner.position();
                let start = before
                    + text[before..end]
                        .iter()
                        .position(|&byte| !crate::locale::ascii_space(byte))?;
                let (start, end) =
                    if crate::lexer::short_decimal_integer(&text[start..end]).is_some() {
                        (start as u16, end as u16)
                    } else {
                        (0, 0)
                    };
                *arguments.get_mut(argc)? = Argument {
                    number,
                    negative: previous == Kind::Minus,
                    start,
                    end,
                };
                argc += 1;
            }
            previous = token.kind;
        }
        if numbers.next().is_some() || !arguments[..argc].iter().any(|arg| arg.start != arg.end) {
            return None;
        }
        Some(Self {
            parsed,
            first_expr,
            arguments,
            argc,
        })
    }

    fn match_body(&self, input: &[u8], start: usize) -> Option<(usize, [f64; MAX_ARGS])> {
        let [old_start, old_end] = self.parsed.offsets.map(|offset| offset as usize);
        let old = &input[old_start..old_end];
        let mut tail = &input[start..];
        let mut previous = 0;
        let mut numbers = [0.0; MAX_ARGS];
        for (new, arg) in numbers.iter_mut().zip(&self.arguments[..self.argc]) {
            *new = arg.number;
            if arg.start == arg.end {
                continue;
            }
            tail = tail.strip_prefix(&old[previous..usize::from(arg.start)])?;
            let digits = tail
                .iter()
                .take(16)
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            *new = crate::lexer::short_decimal_integer(&tail[..digits])?;
            if *new == 0.0 {
                return None;
            }
            tail = &tail[digits..];
            previous = usize::from(arg.end);
        }
        tail = tail.strip_prefix(&old[previous..])?;
        if tail
            .first()
            .is_some_and(|&byte| byte == b'_' || crate::locale::is_alnum(byte))
        {
            return None;
        }
        // Distinct signed values must keep the same parser resource slots.
        for (i, a) in self.arguments[..self.argc].iter().enumerate() {
            for (j, b) in self.arguments[..i].iter().enumerate() {
                if a.negative == b.negative
                    && ((a.number == b.number) != (numbers[i] == numbers[j]))
                {
                    return None;
                }
            }
        }
        Some((input.len() - tail.len(), numbers))
    }
}

fn same_name(text: &Lexeme) -> Lexeme {
    let Lexeme::Source {
        start,
        end,
        string_id,
    } = *text
    else {
        unreachable!("verified callback names borrow source bytes");
    };
    Lexeme::Source {
        start,
        end,
        string_id,
    }
}

impl Parser {
    pub(super) fn remember_numeric_leaf(
        &mut self,
        input: &[u8],
        first_expr: ExprId,
        leaf: ParsedLeaf,
    ) {
        if self.numeric_leaves.len() == MAX_SHAPES || crate::memory::compilation::active() {
            return;
        }
        if let Some(leaf) = NumericLeaf::new(&self.ast, input, first_expr, leaf) {
            let _ = self.numeric_leaves.try_push(leaf);
        }
    }

    pub(super) fn repeated_numeric_leaf(
        &mut self,
        input: &[u8],
        line: u32,
        start: usize,
    ) -> Result<Option<ExprId>, Error> {
        let matched = self.numeric_leaves.iter().rev().find_map(|leaf| {
            if self.depth > leaf.parsed.depth {
                return None;
            }
            let (end, numbers) = leaf.match_body(input, start)?;
            Some((*leaf, end, numbers))
        });
        let Some((leaf, end, numbers)) = matched else {
            return Ok(None);
        };
        let (expr, old_span) = &self.ast.exprs[leaf.parsed.id];
        let Expr::Function {
            params,
            body,
            end_line,
            ..
        } = *expr
        else {
            unreachable!("verified callback expression");
        };
        let old_line = old_span.start;
        let Some(new_end) = line.checked_add(end_line - old_line) else {
            return Ok(None);
        };
        let Ok(start_offset) = u32::try_from(start) else {
            return Ok(None);
        };
        let Ok(end_offset) = u32::try_from(end) else {
            return Ok(None);
        };
        if end_offset - start_offset > MAX_TEMPLATE_BODY {
            return Ok(None);
        }
        let (statement, call) = call_body(&self.ast, body).expect("verified callback statement");
        let delta = line.wrapping_sub(old_line);
        let new_first = self.ast.exprs.len();
        let remap = |id: ExprId| new_first + id - leaf.first_expr;
        let mut numbers = numbers.into_iter();
        let count = leaf.parsed.id - leaf.first_expr;
        fallible::reserve(&mut self.ast.exprs, count)?;
        self.resources.skip_child_values(count)?;
        for id in leaf.first_expr..leaf.parsed.id {
            let (expr, span) = &self.ast.exprs[id];
            let span = SourceSpan {
                start: span.start.wrapping_add(delta),
                end: span.end.wrapping_add(delta),
                discharge: span.discharge.wrapping_add(delta),
                binding: span.binding,
                scalar_cost: span.scalar_cost,
                operand: 0,
            };
            let expr = match expr {
                Expr::Name(name) => Expr::Name(same_name(name)),
                Expr::Number(_) => Expr::Number(numbers.next().expect("verified numeric argument")),
                Expr::Unary(Kind::Minus, inner) => Expr::Unary(Kind::Minus, remap(*inner)),
                Expr::Method(receiver, name, args, line) => {
                    let args = *args;
                    let start = u32::try_from(self.ast.ids.len()).map_err(|_| Error::memory())?;
                    fallible::reserve(&mut self.ast.ids, args.len())?;
                    for index in args.range() {
                        self.ast.ids.push(remap(self.ast.ids[index]));
                    }
                    Expr::Method(
                        remap(*receiver),
                        same_name(name),
                        Span {
                            start,
                            len: args.len,
                        },
                        line.wrapping_add(delta),
                    )
                }
                _ => unreachable!("verified callback body"),
            };
            self.ast.exprs.push((expr, span));
        }
        let body_line = self.ast.stmts[statement].1.wrapping_add(delta);
        let statement = self.rstatement_node(Stmt::Call(remap(call)), body_line)?;
        let body = self.one(statement)?;
        self.scanner.skip_closed_bytes(end, new_end);
        self.bump(input)?;
        self.previous_line = new_end;
        self.ast
            .stacks
            .try_push((self.ast.exprs.len(), leaf.parsed.stack))?;
        self.expression_with::<false>(
            input,
            Expr::Function {
                params,
                vararg: false,
                body,
                end_line: new_end,
                template_source: [start_offset, end_offset],
                template_line: 0,
            },
            line,
        )
        .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Program;

    fn compile(source: &str) -> (usize, Program) {
        let mut parser = Parser::try_new_in(&mut Spare::default(), 0).unwrap();
        let bytes = source.as_bytes();
        let ast = parser.resume(bytes, true).unwrap().unwrap();
        let name = b"@parsed-callbacks.lua";
        // The regular emitter must also accept the cloned syntax tree.
        let program = crate::compiler::compile(ast, bytes, name).unwrap();
        let reference = crate::compile_named(bytes, name).unwrap();
        let dump = |program: &Program| {
            crate::chunk51::dump(&program.data, program.data.entry as usize).unwrap()
        };
        assert_eq!(dump(&program), dump(&reference));
        (parser.numeric_leaves.len(), program)
    }

    fn callback(first: u32, second: u32, rotation: u32) -> String {
        format!("function(item)\n item:size({first},{second},4.0):rotate(-{rotation})\nend")
    }

    #[test]
    fn parsed_callbacks_rebase_multiline_code_and_check_argument_collisions() {
        let pad = ".".repeat(2200);
        let first = callback(1, 2, 3);
        let moved = callback(1000, 2000, 3000);
        let source = format!("return {first},\r\n\r\n{moved} --{pad}");
        let (shapes, program) = compile(&source);
        assert_eq!(shapes, 1);
        assert_eq!(program.data.prototypes[0].defined, [1, 3]);
        assert_eq!(program.data.prototypes[1].defined, [5, 7]);

        for changed in [
            callback(4, 20, 30),
            callback(1000, 1000, 30),
            callback(1, 2, 0),
        ] {
            compile(&format!("return {first},\n{moved},\n{changed} --{pad}"));
        }
    }
}
