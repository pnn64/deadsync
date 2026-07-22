//! Define reusable aliases for long `#[cfg]` expressions in build scripts.
//!
//! This is the `cfg_aliases` 0.2.1 macro with recursive expression expansions
//! fixed for Rust's `semicolon_in_expressions_from_macros` future-incompat lint.

#![allow(clippy::needless_doctest_main)]

/// Define `cfg` aliases for the crate being built.
#[macro_export]
macro_rules! cfg_aliases {
    (@cfg_is_set $cfgname:ident) => {{
        let cfg_var = stringify!($cfgname).to_uppercase().replace('-', "_");
        let result = std::env::var(format!("CARGO_CFG_{}", &cfg_var)).is_ok();

        if !result && cfg_var == "DEBUG_ASSERTIONS" {
            std::env::var("PROFILE") == Ok("debug".to_owned())
        } else {
            result
        }
    }};

    (@cfg_has_feature $feature:expr) => {{
        std::env::var(format!(
            "CARGO_FEATURE_{}",
            &stringify!($feature)
                .to_uppercase()
                .replace('-', "_")
                .replace('"', "")
        ))
        .map(|value| value == "1")
        .unwrap_or(false)
    }};

    (@cfg_contains $cfgname:ident = $cfgvalue:expr) => {
        std::env::var(format!(
            "CARGO_CFG_{}",
            &stringify!($cfgname).to_uppercase().replace('-', "_")
        ))
        .unwrap_or_default()
        .split(',')
        .any(|value| value == $cfgvalue)
    };

    (
        @parser_emit
        all
        $({$($grouped:tt)+})+
    ) => {
        ($(
            ($crate::cfg_aliases!(@parser $($grouped)+))
        )&&+)
    };

    (
        @parser_emit
        any
        $({$($grouped:tt)+})+
    ) => {
        ($(
            ($crate::cfg_aliases!(@parser $($grouped)+))
        )||+)
    };

    (
        @parser_clause
        $op:ident
        [$({$($grouped:tt)+})*]
        [, $($rest:tt)*]
        $($current:tt)+
    ) => {
        $crate::cfg_aliases!(@parser_clause $op [
            $(
                {$($grouped)+}
            )*
            {$($current)+}
        ] [
            $($rest)*
        ])
    };

    (
        @parser_clause
        $op:ident
        [$({$($grouped:tt)+})*]
        [$tok:tt $($rest:tt)*]
        $($current:tt)*
    ) => {
        $crate::cfg_aliases!(@parser_clause $op [
            $(
                {$($grouped)+}
            )*
        ] [
            $($rest)*
        ] $($current)* $tok)
    };

    (
        @parser_clause
        $op:ident
        [$({$($grouped:tt)+})*]
        []
        $($current:tt)+
    ) => {
        $crate::cfg_aliases!(@parser_emit $op
            $(
                {$($grouped)+}
            )*
            {$($current)+}
        )
    };

    (@parser all($($tokens:tt)+)) => {
        $crate::cfg_aliases!(@parser_clause all [] [$($tokens)+])
    };

    (@parser any($($tokens:tt)+)) => {
        $crate::cfg_aliases!(@parser_clause any [] [$($tokens)+])
    };

    (@parser not($($tokens:tt)+)) => {
        !($crate::cfg_aliases!(@parser $($tokens)+))
    };

    (@parser feature = $value:expr) => {
        $crate::cfg_aliases!(@cfg_has_feature $value)
    };

    (@parser $key:ident = $value:expr) => {
        $crate::cfg_aliases!(@cfg_contains $key = $value)
    };

    (@parser $value:ident) => {
        __cfg_aliases_matcher__!($value)
    };

    (
        @with_dollar[$dol:tt]
        $( $alias:ident : { $($config:tt)* } ),* $(,)?
    ) => {
        macro_rules! __cfg_aliases_matcher__ {
            $(
                ($alias) => {
                    $crate::cfg_aliases!(@parser $($config)*)
                };
            )*
            ($dol value:ident) => {
                $crate::cfg_aliases!(@cfg_is_set $dol value)
            };
        }

        $(
            println!("cargo:rustc-check-cfg=cfg({})", stringify!($alias));
            if $crate::cfg_aliases!(@parser $($config)*) {
                println!("cargo:rustc-cfg={}", stringify!($alias));
            }
        )*
    };

    ($($tokens:tt)*) => {
        $crate::cfg_aliases!(@with_dollar[$] $($tokens)*)
    };
}
