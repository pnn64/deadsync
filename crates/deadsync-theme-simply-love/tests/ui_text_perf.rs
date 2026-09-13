//! Exact production modules with frozen parent implementations for comparison.
//! An isolated test binary keeps allocation tracking independent of gameplay's
//! process-wide allocator diagnostics.
// The two private theme DSL arms used by the included HUD. Both invoke the
// same production builders as src/lib.rs.
mod act_macro {
    macro_rules! act {
    (quad: $($tail:tt)+) => {{
        ::deadlib_present::__act_from_builder!(
            ($($tail)+) ::deadsync_assets::present_dsl::SpriteBuilder::solid()
        )
    }};
    (text: $($tail:tt)+) => {{
        ::deadlib_present::__act_from_builder!(
            ($($tail)+) ::deadsync_assets::present_dsl::TextBuilder::new()
        )
    }};
}
    pub(crate) use act;
}
pub(crate) use act_macro::act;

#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[allow(dead_code)]
mod lobby {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/screens/components/shared/lobby_hud.rs"
    ));

    mod ui_text_lobby_perf {
        include!("perf/ui_text/lobby.rs");
    }
}

mod wrapping {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/screens/options/text_wrap.rs"
    ));

    mod ui_text_wrap_perf {
        include!("perf/ui_text/wrap.rs");
    }
}
