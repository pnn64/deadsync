#[path = "../src/snapshot_paths.rs"]
mod snapshot_paths;

fn serialized(value: &[u8]) -> Vec<u8> {
    [format!("s{}:", value.len()).into_bytes(), value.to_vec()].concat()
}

#[test]
fn checkout_and_temp_directory_lengths_do_not_change_snapshots() {
    for path in [
        "C:/Users/test/Temp/short",
        "/a/much/longer/fixture/directory",
    ] {
        let paths = [(path.to_owned(), "$FIXTURE0".to_owned())];
        let input = serialized(format!("{path}/files/A.txt").as_bytes());
        assert_eq!(
            snapshot_paths::normalize(&input, &paths),
            b"s21:$FIXTURE0/files/A.txt"
        );
    }
}

#[test]
fn binary_strings_and_embedded_serialization_markers_remain_literal() {
    let paths = [("/fixture".to_owned(), "$FIXTURE0".to_owned())];
    let binary = b"\0\xff;s12:literal;{}";
    let mut input = b"{s4:name=".to_vec();
    input.extend(serialized(b"/fixture/a"));
    input.extend(b";s4:data=");
    input.extend(serialized(binary));
    input.push(b'}');
    let mut expected = b"{s4:name=s11:$FIXTURE0/a;s4:data=".to_vec();
    expected.extend(serialized(binary));
    expected.push(b'}');
    assert_eq!(snapshot_paths::normalize(&input, &paths), expected);
}

#[test]
fn malformed_lengths_and_unregistered_paths_are_preserved() {
    let paths = [("/fixture".to_owned(), "$FIXTURE0".to_owned())];
    for input in [
        b"s999:/fixture".as_slice(),
        b"s9999999999999999999999999999:x",
        b"s7:unknown",
        b"s:bad",
        b"s0:",
    ] {
        assert_eq!(snapshot_paths::normalize(input, &paths), input);
    }
}
