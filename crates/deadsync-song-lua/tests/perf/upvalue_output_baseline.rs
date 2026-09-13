//! Frozen from eeb449635856c265a7e337c4cb7c171904065d4a; only visibility and formatting differ.

use super::*;

pub(super) fn nested_function_named_upvalue_tables(
    getupvalue: &Function,
    function: &Function,
    names: &[&str],
    seen_tables: &mut HashSet<usize>,
    seen_functions: &mut HashSet<usize>,
) -> Result<Vec<Table>, String> {
    if !seen_functions.insert(function.to_pointer() as usize) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for index in 1..=function.info().num_upvalues {
        let (name, value): (Value, Value) = getupvalue
            .call((function.clone(), i64::from(index)))
            .map_err(|err| err.to_string())?;
        if let Value::String(name) = &name {
            let name = name.to_str().map_err(|err| err.to_string())?;
            if names.iter().any(|candidate| name.as_ref() == *candidate)
                && let Value::Table(table) = &value
                && seen_tables.insert(table.to_pointer() as usize)
            {
                out.push(table.clone());
            }
        }
        if let Value::Function(child) = value {
            out.extend(nested_function_named_upvalue_tables(
                getupvalue,
                &child,
                names,
                seen_tables,
                seen_functions,
            )?);
        }
    }
    Ok(out)
}
