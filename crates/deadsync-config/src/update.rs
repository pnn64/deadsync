pub fn set_if_changed<T>(slot: &mut T, value: T) -> bool
where
    T: PartialEq,
{
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

pub fn set_pair_if_changed<T>(a: &mut T, a_value: T, b: &mut T, b_value: T) -> bool
where
    T: PartialEq,
{
    set_if_changed(a, a_value) | set_if_changed(b, b_value)
}

pub fn set_quad_if_changed<T>(
    a: &mut T,
    a_value: T,
    b: &mut T,
    b_value: T,
    c: &mut T,
    c_value: T,
    d: &mut T,
    d_value: T,
) -> bool
where
    T: PartialEq,
{
    set_pair_if_changed(a, a_value, b, b_value) | set_pair_if_changed(c, c_value, d, d_value)
}

pub fn set_f32_if_changed(slot: &mut f32, value: f32) -> bool {
    if (*slot - value).abs() <= f32::EPSILON {
        false
    } else {
        *slot = value;
        true
    }
}

pub fn set_f64_if_changed(slot: &mut f64, value: f64) -> bool {
    if (*slot - value).abs() <= f64::EPSILON {
        false
    } else {
        *slot = value;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DedicatedMenuNavigation {
    pub enabled: bool,
    pub disabled_by_missing_bindings: bool,
}

pub const fn dedicated_menu_navigation_label(three_key_navigation: bool) -> &'static str {
    if three_key_navigation {
        "Three Key Menu"
    } else {
        "Five Key Menu"
    }
}

pub const fn resolve_dedicated_menu_navigation(
    requested: bool,
    dedicated_bindings_supported: bool,
) -> DedicatedMenuNavigation {
    let disabled_by_missing_bindings = requested && !dedicated_bindings_supported;
    DedicatedMenuNavigation {
        enabled: requested && dedicated_bindings_supported,
        disabled_by_missing_bindings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_if_changed_updates_only_different_values() {
        let mut value = 1;

        assert!(!set_if_changed(&mut value, 1));
        assert_eq!(value, 1);
        assert!(set_if_changed(&mut value, 2));
        assert_eq!(value, 2);
    }

    #[test]
    fn grouped_updates_report_any_change() {
        let mut a = 1;
        let mut b = 2;
        assert!(!set_pair_if_changed(&mut a, 1, &mut b, 2));
        assert!(set_pair_if_changed(&mut a, 1, &mut b, 3));
        assert_eq!((a, b), (1, 3));

        let mut c = 3;
        let mut d = 4;
        assert!(!set_quad_if_changed(
            &mut a, 1, &mut b, 3, &mut c, 3, &mut d, 4
        ));
        assert!(set_quad_if_changed(
            &mut a, 5, &mut b, 3, &mut c, 6, &mut d, 4
        ));
        assert_eq!((a, b, c, d), (5, 3, 6, 4));
    }

    #[test]
    fn float_updates_use_epsilon() {
        let mut value = 1.0f32;
        assert!(!set_f32_if_changed(&mut value, 1.0 + f32::EPSILON));
        assert!(set_f32_if_changed(&mut value, 1.001));

        let mut value = 1.0f64;
        assert!(!set_f64_if_changed(&mut value, 1.0 + f64::EPSILON));
        assert!(set_f64_if_changed(&mut value, 1.001));
    }

    #[test]
    fn dedicated_menu_navigation_labels_match_menu_modes() {
        assert_eq!(dedicated_menu_navigation_label(true), "Three Key Menu");
        assert_eq!(dedicated_menu_navigation_label(false), "Five Key Menu");
    }

    #[test]
    fn dedicated_menu_navigation_requires_supported_bindings() {
        assert_eq!(
            resolve_dedicated_menu_navigation(true, true),
            DedicatedMenuNavigation {
                enabled: true,
                disabled_by_missing_bindings: false,
            }
        );
        assert_eq!(
            resolve_dedicated_menu_navigation(true, false),
            DedicatedMenuNavigation {
                enabled: false,
                disabled_by_missing_bindings: true,
            }
        );
        assert_eq!(
            resolve_dedicated_menu_navigation(false, false),
            DedicatedMenuNavigation {
                enabled: false,
                disabled_by_missing_bindings: false,
            }
        );
    }
}
