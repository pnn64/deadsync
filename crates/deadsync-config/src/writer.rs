use std::fmt::{Display, Write as _};

pub fn push_section(content: &mut String, name: &str) {
    content.push_str(name);
    content.push('\n');
}

pub fn push_line(content: &mut String, key: &str, value: impl Display) {
    writeln!(content, "{key}={value}").expect("writing into String cannot fail");
}

pub fn push_bool(content: &mut String, key: &str, enabled: bool) {
    push_line(content, key, if enabled { 1 } else { 0 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_ini_lines() {
        let mut content = String::new();

        push_section(&mut content, "[Options]");
        push_line(&mut content, "Number", 7);
        push_bool(&mut content, "Enabled", true);
        push_bool(&mut content, "Disabled", false);

        assert_eq!(content, "[Options]\nNumber=7\nEnabled=1\nDisabled=0\n");
    }
}
