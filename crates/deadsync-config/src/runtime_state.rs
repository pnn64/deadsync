use crate::ini::SimpleIni;
use crate::writer::push_line;
use std::fmt::Display;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeStateIds {
    pub smx_p1_serial: Option<String>,
    pub smx_p2_serial: Option<String>,
    pub default_profile_p1: Option<String>,
    pub default_profile_p2: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeStateIdTokens<'a> {
    pub smx_p1_serial: &'a str,
    pub smx_p2_serial: &'a str,
    pub default_profile_p1: &'a str,
    pub default_profile_p2: &'a str,
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

pub fn push_runtime_state_id_option_lines(content: &mut String, ids: RuntimeStateIdTokens<'_>) {
    push_line(content, "SmxP1Serial", ids.smx_p1_serial);
    push_line(content, "SmxP2Serial", ids.smx_p2_serial);
    push_line(content, "DefaultLocalProfileIDP1", ids.default_profile_p1);
    push_line(content, "DefaultLocalProfileIDP2", ids.default_profile_p2);
}

pub fn push_pad_order_option_lines<I, V>(content: &mut String, lines: I)
where
    I: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    for (key, value) in lines {
        push_line(content, key, value);
    }
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

    #[test]
    fn writes_runtime_state_id_option_lines() {
        let mut content = String::new();

        push_runtime_state_id_option_lines(
            &mut content,
            RuntimeStateIdTokens {
                smx_p1_serial: "p1",
                smx_p2_serial: "p2",
                default_profile_p1: "profile-a",
                default_profile_p2: "profile-b",
            },
        );

        assert_eq!(
            content,
            concat!(
                "SmxP1Serial=p1\n",
                "SmxP2Serial=p2\n",
                "DefaultLocalProfileIDP1=profile-a\n",
                "DefaultLocalProfileIDP2=profile-b\n",
            ),
        );
    }

    #[test]
    fn writes_pad_order_option_lines() {
        let mut content = String::new();

        push_pad_order_option_lines(
            &mut content,
            [("PadOrderRawInput", "0,1"), ("PadOrderSmx", "")],
        );

        assert_eq!(content, "PadOrderRawInput=0,1\nPadOrderSmx=\n");
    }
}
