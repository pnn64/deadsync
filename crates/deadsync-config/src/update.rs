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
    fn float_updates_use_epsilon() {
        let mut value = 1.0f32;
        assert!(!set_f32_if_changed(&mut value, 1.0 + f32::EPSILON));
        assert!(set_f32_if_changed(&mut value, 1.001));

        let mut value = 1.0f64;
        assert!(!set_f64_if_changed(&mut value, 1.0 + f64::EPSILON));
        assert!(set_f64_if_changed(&mut value, 1.001));
    }
}
