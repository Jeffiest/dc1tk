//! src/tests/test_verify.py: verify.py (cli.py verify) is not ported to Rust yet, so its tests wait
//! for it.

#[test]
#[ignore = "needs verify (verify.py: verify, format_report), not ported yet"]
fn reports_every_script() {}

#[test]
#[ignore = "needs verify (verify.py: verify, format_report), not ported yet; also needs C2S_CORPUS"]
fn corpus_round_trip() {}
