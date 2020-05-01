use common::util::*;

#[test]
fn test_date_email() {
    let (_, mut ucmd) = at_and_ucmd!();
    let result = ucmd.arg("--rfc-email").run();
    assert!(result.success);
}

#[test]
fn test_date_email2() {
    let (_, mut ucmd) = at_and_ucmd!();
    let result = ucmd.arg("-R").run();
    assert!(result.success);
}

#[test]
fn test_date_rfc_3339() {
    let (_, mut ucmd) = at_and_ucmd!();
    let result = ucmd.arg("--rfc-3339=ns").run();
    assert!(result.success);
}

#[test]
fn test_date_rfc_8601() {
    let (_, mut ucmd) = at_and_ucmd!();
    let result = ucmd.arg("--iso-8601=ns").run();
    assert!(result.success);
}

#[test]
fn test_date_rfc_8601_second() {
    let (_, mut ucmd) = at_and_ucmd!();
    let result = ucmd.arg("--iso-8601=second").run();
    assert!(result.success);
}

#[test]
fn test_date_utc() {
    let (_, mut ucmd) = at_and_ucmd!();
    let result = ucmd.arg("--utc").run();
    assert!(result.success);
}

#[test]
fn test_date_universal() {
    let (_, mut ucmd) = at_and_ucmd!();
    let result = ucmd.arg("--universal").run();
    assert!(result.success);
}

#[test]
fn test_date_format_y() {
    let (_, mut ucmd) = at_and_ucmd!();
    let result = ucmd.arg("+%y").run();
    assert!(result.success);
}
