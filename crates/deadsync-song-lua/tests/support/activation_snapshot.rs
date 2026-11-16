use std::{cell::RefCell, path::Path};

thread_local! {
    static PATHS: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}

pub fn register_path(path: &Path) {
    PATHS.with(|paths| {
        let mut paths = paths.borrow_mut();
        let index = paths.len() / 2;
        let raw = path.to_string_lossy();
        for path in [raw.to_string(), raw.replace('\\', "/")] {
            let quoted = format!("{path:?}");
            paths.push((
                quoted[1..quoted.len() - 1].into(),
                format!("$FIXTURE{index}"),
            ));
        }
    });
}

// These readers already return formatted Debug text. Normalize its path
// spelling before the snapshot layer quotes that text for storage.
pub fn text(mut value: String) -> String {
    PATHS.with(|paths| {
        for (path, replacement) in paths.borrow().iter() {
            value = value.replace(path, replacement);
        }
    });
    value
}
