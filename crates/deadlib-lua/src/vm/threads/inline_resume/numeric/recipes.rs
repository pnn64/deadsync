//! Short arithmetic chains that produce numeric yield arguments.
use super::*;

pub(super) const MAX_RESULTS: usize = 4;

#[derive(Clone, Copy)]
enum Transform {
    Negate,
    AddLeft(f64),
    AddRight(f64),
    SubtractLeft(f64),
    SubtractRight(f64),
    MultiplyLeft(f64),
    MultiplyRight(f64),
    DivideLeft(f64),
    DivideRight(f64),
}

impl Transform {
    fn binary(op: Op, constant: f64, constant_left: bool) -> Option<Self> {
        Some(match (op, constant_left) {
            (Op::Add, true) => Self::AddLeft(constant),
            (Op::Add, false) => Self::AddRight(constant),
            (Op::Sub, true) => Self::SubtractLeft(constant),
            (Op::Sub, false) => Self::SubtractRight(constant),
            (Op::Mul, true) => Self::MultiplyLeft(constant),
            (Op::Mul, false) => Self::MultiplyRight(constant),
            (Op::Div, true) => Self::DivideLeft(constant),
            (Op::Div, false) => Self::DivideRight(constant),
            _ => return None,
        })
    }

    #[inline]
    fn apply(self, value: f64) -> f64 {
        match self {
            Self::Negate => -value,
            Self::AddLeft(k) => Vm::number_op(Op::Add, k, value),
            Self::AddRight(k) => Vm::number_op(Op::Add, value, k),
            Self::SubtractLeft(k) => Vm::number_op(Op::Sub, k, value),
            Self::SubtractRight(k) => Vm::number_op(Op::Sub, value, k),
            Self::MultiplyLeft(k) => Vm::number_op(Op::Mul, k, value),
            Self::MultiplyRight(k) => Vm::number_op(Op::Mul, value, k),
            Self::DivideLeft(k) => Vm::number_op(Op::Div, k, value),
            Self::DivideRight(k) => Vm::number_op(Op::Div, value, k),
        }
    }
}

#[derive(Clone, Copy)]
enum Recipe {
    Index,
    Constant(f64),
    One(Transform),
    Two(Transform, Transform),
}

impl Recipe {
    fn then(self, step: Transform) -> Option<Self> {
        match self {
            Self::Index => Some(Self::One(step)),
            Self::One(first) => Some(Self::Two(first, step)),
            Self::Constant(_) | Self::Two(..) => None,
        }
    }

    #[inline]
    fn evaluate(self, index: f64) -> f64 {
        match self {
            Self::Index => index,
            Self::Constant(value) => value,
            Self::One(step) => step.apply(index),
            Self::Two(first, second) => second.apply(first.apply(index)),
        }
    }
}

pub(super) struct Recipes([Recipe; MAX_RESULTS]);

impl Recipes {
    pub(super) fn decode(
        writes: &[Write],
        index: usize,
        args: usize,
        count: usize,
    ) -> Option<Self> {
        let mut results = [None; MAX_RESULTS];
        let used = results.get_mut(..count)?;
        for write in writes {
            let output = usize::from(write.register).checked_sub(args)?;
            if output >= count {
                return None;
            }
            let operand = |input| match input {
                Input::Constant(value) => Some(Recipe::Constant(value)),
                Input::Argument(register) if register == index => Some(Recipe::Index),
                Input::Argument(register) => used.get(register.checked_sub(args)?).copied()?,
            };
            used[output] = Some(match write.expression {
                Expression::Value(input) => operand(input)?,
                Expression::Negate(input) => operand(input)?.then(Transform::Negate)?,
                Expression::Binary(op, left, right) => match (operand(left)?, operand(right)?) {
                    (Recipe::Constant(left), right) => {
                        right.then(Transform::binary(op, left, true)?)?
                    }
                    (left, Recipe::Constant(right)) => {
                        left.then(Transform::binary(op, right, false)?)?
                    }
                    _ => return None,
                },
            });
        }
        if used.iter().any(Option::is_none) {
            return None;
        }
        Some(Self(results.map(|recipe| recipe.unwrap_or(Recipe::Index))))
    }

    #[inline]
    pub(super) fn evaluate<const N: usize>(&self, index: f64) -> [Value; N] {
        std::array::from_fn(|i| Value::Number(self.0[i].evaluate(index)))
    }

    #[inline]
    pub(super) fn write(&self, index: f64, registers: &mut [Value]) {
        for (recipe, register) in self.0.iter().zip(registers) {
            *register = Value::Number(recipe.evaluate(index));
        }
    }
}
