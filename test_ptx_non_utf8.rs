#[test]
fn test_non_utf8_filename() {
    use std::os::unix::ffi::OsStringExt;
    use uutests::at_and_ucmd;
    
    let (at, mut ucmd) = at_and_ucmd!();
    
    let filename = std::ffi::OsString::from_vec(vec![0xFF, 0xFE]);
    std::fs::write(at.plus(&filename), b"hello world\ntest line\n").unwrap();
    
    ucmd.args(&["-G"])
        .arg(&filename)
        .succeeds()
        .stdout_contains(".xx");
}