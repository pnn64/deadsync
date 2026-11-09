use super::*;

#[derive(Clone, Copy)]
enum Operation {
    Parts,
    Product,
    Exact,
    Minimum,
}

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    for (name, operation) in [
        ("GetVersionParts", Operation::Parts),
        ("GetProductVersion", Operation::Product),
        ("IsProductVersion", Operation::Exact),
        ("IsMinimumProductVersion", Operation::Minimum),
    ] {
        if missing(vm, name)? {
            let function = vm.new_host_function_with_data(dispatch, &[], operation)?;
            bind(vm, name, function)?;
        }
    }
    Ok(())
}

fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let operation = *call.function_data::<Operation>().expect("version helper");
    if matches!(operation, Operation::Parts | Operation::Product) {
        let version = if matches!(operation, Operation::Parts) {
            text(call, arg(call, 0))
        } else {
            None
        };
        let parts = crate::version_parts(
            version
                .as_deref()
                .unwrap_or(crate::SONG_LUA_PRODUCT_VERSION),
        );
        let table = array(call, parts)?;
        return call.push(Value::Table(table));
    }
    let expected = if let Some(version) = text(call, arg(call, 0)) {
        crate::version_parts(&version).to_vec()
    } else {
        call.arguments()
            .iter()
            .filter_map(|&value| float(call, value).map(|value| value.round() as i64))
            .collect()
    };
    let product = crate::version_parts(crate::SONG_LUA_PRODUCT_VERSION);
    let result = match operation {
        Operation::Exact => {
            !expected.is_empty()
                && expected
                    .iter()
                    .enumerate()
                    .all(|(i, value)| product.get(i) == Some(value))
        }
        Operation::Minimum => {
            let mut result = true;
            for (i, value) in expected.iter().enumerate() {
                let actual = product.get(i).copied().unwrap_or(0);
                if actual != *value {
                    result = actual > *value;
                    break;
                }
            }
            result
        }
        _ => unreachable!(),
    };
    call.push(Value::Boolean(result))
}
