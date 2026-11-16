//! Lua 5.1 literal arithmetic folding, without reassociation or speculative coercion.

use crate::{
    lexer::Kind,
    parser::{Ast, Expr},
};

/// Fold every node into `folded`, replacing its contents.
pub(crate) fn literals_into(ast: &Ast, folded: &mut Vec<Option<f64>>) -> Result<(), crate::Error> {
    // Children precede parents in the flat arena, so no recursive walk is needed.
    folded.clear();
    crate::memory::fallible::reserve(folded, ast.exprs.len())?;
    for (expr, _) in &ast.exprs {
        let result = match expr {
            Expr::Number(n) => Some(*n),
            Expr::Paren(id) => folded[*id],
            Expr::Unary(Kind::Minus, id) => folded[*id].map(|n: f64| -n),
            Expr::Binary(op, left, right, _) => {
                folded[*left].zip(folded[*right]).and_then(|(x, y)| {
                    Some(match op {
                        Kind::Plus => x + y,
                        Kind::Minus => x - y,
                        Kind::Star => x * y,
                        Kind::Slash if y != 0.0 => x / y,
                        Kind::Percent if y != 0.0 => x - (x / y).floor() * y,
                        Kind::Caret => x.powf(y),
                        _ => return None,
                    })
                })
            }
            _ => None,
        };
        folded.push(result.filter(|n| !n.is_nan()));
    }
    Ok(())
}
