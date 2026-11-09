use super::*;
#[cfg(feature = "test-support")]
use crate::rust_engine::host_clocks::Inputs;
use chrono::Datelike;

#[derive(Clone)]
struct Calendar {
    #[cfg(feature = "test-support")]
    name: &'static str,
    value: Option<i32>,
    #[cfg(feature = "test-support")]
    inputs: std::sync::Arc<std::sync::Mutex<Inputs>>,
}

pub(super) fn install(vm: &mut Vm, _context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    install_for_date(vm, _context, crate::song_lua_local_date_globals())
}
pub(super) fn install_for_date(
    vm: &mut Vm,
    _context: &crate::SongLuaCompileContext,
    date: crate::SongLuaDateGlobals,
) -> Result<(), Error> {
    let day_of_year = date
        .month_of_year
        .checked_add(1)
        .and_then(|month| {
            chrono::NaiveDate::from_ymd_opt(date.year, month as u32, date.day_of_month as u32)
        })
        .map(|date| date.ordinal0() as i32);
    #[cfg(feature = "test-support")]
    let inputs = std::sync::Arc::new(std::sync::Mutex::new(Inputs::new(_context)));
    for (name, value) in [
        ("Year", Some(date.year)),
        ("MonthOfYear", Some(date.month_of_year)),
        ("DayOfMonth", Some(date.day_of_month)),
        ("DayOfYear", day_of_year),
        ("Hour", Some(date.hour)),
        ("Minute", Some(date.minute)),
        ("Second", Some(date.second)),
    ] {
        let function = vm.new_host_function_with_data(
            calendar,
            &[],
            Calendar {
                #[cfg(feature = "test-support")]
                name,
                value,
                #[cfg(feature = "test-support")]
                inputs: inputs.clone(),
            },
        )?;
        bind(vm, name, function)?;
    }
    constant_global(vm, "HolidayCheer", Value::Boolean(date.month_of_year == 11))
}

fn calendar(call: &mut HostCall<'_>) -> Result<(), Error> {
    let calendar = call
        .function_data::<Calendar>()
        .expect("calendar helper")
        .clone();
    #[cfg(feature = "test-support")]
    if let Some(value) = Inputs::next(&calendar.inputs, call, calendar.name)? {
        return call.push(Value::Number(value));
    }
    let value = calendar
        .value
        .ok_or_else(|| call.error(b"invalid calendar date for DayOfYear"))?;
    call.push(Value::Number(value as f64))
}
