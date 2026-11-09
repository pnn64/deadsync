use super::*;

#[test]
fn aliases_preserve_identity_raw_order_and_metamethod_effects() {
    super::check_aliases(false);
}

#[cfg(windows)]
#[test]
#[ignore = "release benchmark; run alone"]
fn benchmark_noteskin_aliases() {
    super::callbacks_perf::pin();
    for count in [0, 8, 64, 512] {
        let mut session = Session::new().unwrap();
        let noteskin = run(
            &mut session,
            &format!(
                "local t={{}};for i=0,{} do t['key'..i]=i end;return {{Rotate=t}}",
                count - 1
            ),
        );
        let names: Vec<_> = (0..8).map(|i| format!("Missing{i}")).collect();
        super::callbacks_perf::measure(&format!("noteskin_aliases_{count}"), 256, || {
            session
                .scoped(|session| session.normalize(noteskin, &names, &[]))
                .unwrap();
        });
    }
}
