use super::*;

#[path = "ignored_tables_before.rs"]
mod before;

#[test]
fn style_callbacks_keep_names_widths_columns_and_consumed_argument_methods() {
    let lua = Lua::new();
    for style in ["single", "double", "solo"] {
        let old = before::create_style_table(&lua, style).unwrap();
        let new = create_style_table(&lua, style).unwrap();
        for method in [
            "GetName",
            "GetStepsType",
            "GetStyleType",
            "ColumnsPerPlayer",
            "GetWidth",
        ] {
            let before = old.get::<Function>(method).unwrap();
            let after = new.get::<Function>(method).unwrap();
            for args in crate::callback_args::perf::argument_cases(&lua, &old) {
                assert_eq!(
                    before.call::<Value>(args.clone()).unwrap(),
                    after.call::<Value>(args).unwrap(),
                    "{style}:{method}"
                );
            }
        }
        let a = old
            .get::<Function>("GetColumnInfo")
            .unwrap()
            .call::<Table>((&old, "ignored", 2))
            .unwrap();
        let b = new
            .get::<Function>("GetColumnInfo")
            .unwrap()
            .call::<Table>((&new, "ignored", 2))
            .unwrap();
        for key in ["Name", "Track", "XOffset"] {
            assert_eq!(a.get::<Value>(key).unwrap(), b.get::<Value>(key).unwrap());
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_ignored_tables() {
    crate::callback_args::perf::measure_calls("style_width", |lua, current| {
        let style = if current {
            create_style_table(lua, "single")
        } else {
            before::create_style_table(lua, "single")
        }
        .unwrap();
        let getter = style.get::<Function>("GetWidth").unwrap();
        (getter, style)
    });
}
