//! Emit the fixed prototype of a closed chunk returning one literal.
use super::*;
use crate::lexer::{DirectScanner, DirectToken};

enum Literal {
    Scalar(Constant),
    String(Lexeme),
}

fn scan(source: &[u8]) -> Result<Option<(Literal, u32, u32)>, Error> {
    let mut scanner = DirectScanner::default();
    let mut token = DirectToken::default();
    scanner.next(source, &mut token)?;
    if token.kind != Kind::Return {
        return Ok(None);
    }
    scanner.next(source, &mut token)?;
    let value_line = token.end_line;
    let literal = match token.kind {
        Kind::Nil => Literal::Scalar(Constant::Nil),
        Kind::False => Literal::Scalar(Constant::Boolean(false)),
        Kind::True => Literal::Scalar(Constant::Boolean(true)),
        Kind::Number => Literal::Scalar(Constant::Number(token.number())),
        Kind::String => Literal::String(scanner.take_text(token)),
        _ => return Ok(None),
    };
    scanner.next(source, &mut token)?;
    let final_line = if token.kind == Kind::Semi {
        let line = token.end_line;
        scanner.next(source, &mut token)?;
        line
    } else {
        value_line
    };
    Ok((token.kind == Kind::Eof).then_some((literal, value_line, final_line)))
}

#[inline(never)]
pub(crate) fn compile_literal_return(
    source: &[u8],
    name: &[u8],
    spare: &mut Spare,
) -> Result<Option<Program>, Error> {
    let (literal, value_line, final_line) = match scan(source) {
        Ok(Some(literal)) => literal,
        Err(error) if error.kind() == ErrorKind::Memory => return Err(error),
        _ => return Ok(None),
    };
    let (mut prototypes, mut reused, name_storage) = match spare.code.take(3) {
        Some(saved) => saved.into_parts(),
        None => (
            fallible::with_capacity(1)?,
            PrototypeBody::default(),
            Box::default(),
        ),
    };
    let (constant, strings) = match literal {
        Literal::Scalar(value) => (value, Box::default()),
        Literal::String(text) => {
            let bytes = text.bytes(source);
            let constant = Constant::PooledString {
                start: 0,
                end: bytes.len() as u32,
            };
            (
                constant,
                copy_boxed(bytes, Some(std::mem::take(&mut reused.constant_strings)))?,
            )
        }
    };
    let nil = matches!(constant, Constant::Nil);
    let mut constants = reusable_vec(1, Some(reused.original_constants))?;
    constants.push(constant);
    let instructions = [
        Instruction {
            op: Op::Load,
            a: 0,
            b: u16::from(nil),
            c: 0,
        },
        Instruction {
            op: Op::Return,
            a: 0,
            b: 0,
            c: 1,
        },
        Instruction {
            op: Op::Return,
            a: 0,
            b: 0,
            c: 0,
        },
    ];
    // The literal occupies register zero through the first return.
    let metadata = [
        InstructionMetadata {
            line: value_line,
            hook_line: if nil { 0 } else { value_line },
            gc_live: 1 << 16,
        },
        InstructionMetadata {
            line: value_line,
            hook_line: value_line,
            gc_live: (1 << 16) | 1,
        },
        InstructionMetadata {
            line: final_line,
            hook_line: final_line,
            gc_live: 0,
        },
    ];
    let lines = [value_line, final_line];
    let active = &lines[..1 + usize::from(value_line != final_line)];
    let mut debug = reused.debug;
    debug.stack = 2;
    debug.storage.try_replace(
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        active,
        &[],
        &[],
    )?;
    prototypes.push(Prototype::new(
        PrototypeBody {
            code: copy_boxed(&instructions, Some(reused.code))?,
            gc_maps: RegisterMaps::Prefix { start: 0, len: 2 },
            original_constants: constants.try_into_boxed_slice()?,
            constant_strings: strings,
            registers: 2,
            vararg: true,
            debug,
            ..PrototypeBody::default()
        },
        copy_boxed(&metadata, Some(reused.metadata))?,
        [0; 2],
    ));
    let program = Program::try_new_reusing_name(prototypes, 0, name, name_storage)?;
    program.verify()?;
    Ok(Some(program))
}

#[cfg(test)]
#[test]
fn fixed_literal_prototypes_match_general_compilation() {
    let inputs: &[&[u8]] = &[
        b"return 'hello'",
        b"return[==[\r\nthere\n]==];",
        b"return\n false",
        b"return nil",
        b"return 0",
        b"return .25\r\n; -- a comment",
        b"return \"escaped\\nstring\"\n",
        b"return [==[\nfirst\nlast]==]\n;\n",
        b"return '';",
    ];
    for &source in inputs {
        let compiled = crate::compile(source).unwrap();
        let direct = compile_literal_return(source, b"=literal", &mut Spare::default())
            .unwrap()
            .unwrap();
        let expected = &compiled.data.prototypes[0];
        let actual = &direct.data.prototypes[0];
        assert_eq!(actual.registers, expected.registers, "{source:?}");
        assert_eq!(actual.debug.stack, expected.debug.stack, "{source:?}");
        assert_eq!(
            actual.debug.active_lines(),
            expected.debug.active_lines(),
            "{source:?}"
        );
        assert_eq!(&*actual.gc_maps, &*expected.gc_maps, "{source:?}");
        for (i, j) in actual.code.iter().zip(expected.code.iter()) {
            assert_eq!((i.op, i.a, i.b, i.c), (j.op, j.a, j.b, j.c), "{source:?}");
        }
        for (i, j) in actual.metadata.iter().zip(expected.metadata.iter()) {
            assert_eq!(
                (i.line, i.hook_line, i.gc_live),
                (j.line, j.hook_line, j.gc_live),
                "{source:?}"
            );
        }
        let mut vm = crate::Vm::new();
        let expected = vm.execute(&compiled).unwrap().to_vec();
        assert_eq!(vm.execute(&direct).unwrap(), expected, "{source:?}");
    }
    for source in [
        b"return 'first', 2".as_slice(),
        b"return nil;;",
        b"return 1; x=2",
        b"return 1 + 2",
        b"returning = nil",
        b"return 'unterminated",
    ] {
        assert!(
            compile_literal_return(source, b"=literal", &mut Spare::default())
                .unwrap()
                .is_none()
        );
    }
}
