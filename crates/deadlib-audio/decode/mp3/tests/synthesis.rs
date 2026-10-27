use deadlib_audio_decode_mp3::synthesis::{Hybrid, Synthesis};
#[test]
fn silence_reset_overlap_and_invalid_shapes() {
    let mut h = Hybrid::new();
    let mut s = Synthesis::new();
    let mut spectrum = [0.0; 576];
    let mut bands = [0.0; 576];
    h.process(&mut spectrum, 0, 0, &mut bands).unwrap();
    assert_eq!(bands, [0.0; 576]);
    let mut pcm = [0.0; 32];
    s.process(&[0.0; 32], &mut pcm);
    assert_eq!(pcm, [0.0; 32]);
    spectrum[0] = 1.0;
    h.process(&mut spectrum, 0, 0, &mut bands).unwrap();
    assert!(bands.iter().any(|v| v.abs() > 0.01));
    spectrum.fill(0.0);
    h.process(&mut spectrum, 0, 0, &mut bands).unwrap();
    assert!(bands.iter().any(|v| v.abs() > 0.01));
    h.process(&mut spectrum, 0, 0, &mut bands).unwrap();
    assert_eq!(bands, [0.0; 576]);
    assert!(h.process(&mut spectrum, 4, 0, &mut bands).is_err());
    assert!(h.process(&mut spectrum, 2, 33, &mut bands).is_err());
    assert!(h.process(&mut spectrum, 0, 2, &mut bands).is_err());
    h.reset();
    s.reset();
    h.process(&mut spectrum, 0, 0, &mut bands).unwrap();
    assert_eq!(bands, [0.0; 576]);
}
#[test]
fn synthesis_finite_history_and_repeatable_reset() {
    let mut synthesis = Synthesis::new();
    let mut source = [0.0; 32];
    source[0] = 0.5;
    let mut first = [0.0; 32];
    synthesis.process(&source, &mut first);
    assert!(first.iter().any(|&v| v != 0.0));
    source.fill(0.0);
    let mut output = [0.0; 32];
    for _ in 0..16 {
        synthesis.process(&source, &mut output);
    }
    assert_eq!(output, [0.0; 32]);
    synthesis.reset();
    source[0] = 0.5;
    synthesis.process(&source, &mut output);
    assert_eq!(first, output);
}
