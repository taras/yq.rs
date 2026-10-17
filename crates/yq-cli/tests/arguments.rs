#[cfg(unix)]
#[test]
fn non_utf8_argument_is_decoded_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::process::Command;

    let output = Command::new(env!("CARGO_BIN_EXE_yq"))
        .arg("-n")
        .arg(OsString::from_vec(b"\"\xff\"".to_vec()))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, "\u{fffd}\n".as_bytes());
    assert!(output.stderr.is_empty());
}
