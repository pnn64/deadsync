use crate::ini::SimpleIni;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeStateIds {
    pub smx_p1_serial: Option<String>,
    pub smx_p2_serial: Option<String>,
    pub default_profile_p1: Option<String>,
    pub default_profile_p2: Option<String>,
}

pub fn load_runtime_state_ids(conf: &SimpleIni) -> RuntimeStateIds {
    RuntimeStateIds {
        smx_p1_serial: nonempty_option(conf, "SmxP1Serial"),
        smx_p2_serial: nonempty_option(conf, "SmxP2Serial"),
        default_profile_p1: profile_id(conf, "DefaultLocalProfileIDP1", "LastProfileP1"),
        default_profile_p2: profile_id(conf, "DefaultLocalProfileIDP2", "LastProfileP2"),
    }
}

fn profile_id(conf: &SimpleIni, key: &str, fallback_key: &str) -> Option<String> {
    nonempty_option(conf, key).or_else(|| nonempty_option(conf, fallback_key))
}

fn nonempty_option(conf: &SimpleIni, key: &str) -> Option<String> {
    conf.get("Options", key)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ini(content: &str) -> SimpleIni {
        let mut conf = SimpleIni::new();
        conf.load_str(content);
        conf
    }

    #[test]
    fn trims_smx_serials_and_ignores_blanks() {
        let ids = load_runtime_state_ids(&ini("[Options]\n\
SmxP1Serial= P1-123 \n\
SmxP2Serial=   \n"));

        assert_eq!(ids.smx_p1_serial.as_deref(), Some("P1-123"));
        assert_eq!(ids.smx_p2_serial, None);
    }

    #[test]
    fn current_profile_ids_take_precedence() {
        let ids = load_runtime_state_ids(&ini("[Options]\n\
DefaultLocalProfileIDP1= current-p1 \n\
LastProfileP1= legacy-p1\n\
DefaultLocalProfileIDP2=current-p2\n\
LastProfileP2=legacy-p2\n"));

        assert_eq!(ids.default_profile_p1.as_deref(), Some("current-p1"));
        assert_eq!(ids.default_profile_p2.as_deref(), Some("current-p2"));
    }

    #[test]
    fn profile_ids_fall_back_to_legacy_keys() {
        let ids = load_runtime_state_ids(&ini("[Options]\n\
DefaultLocalProfileIDP1=   \n\
LastProfileP1= legacy-p1 \n\
LastProfileP2=legacy-p2\n"));

        assert_eq!(ids.default_profile_p1.as_deref(), Some("legacy-p1"));
        assert_eq!(ids.default_profile_p2.as_deref(), Some("legacy-p2"));
    }

    #[test]
    fn missing_runtime_ids_are_none() {
        assert_eq!(
            load_runtime_state_ids(&ini("[Options]\n")),
            RuntimeStateIds::default()
        );
    }
}
