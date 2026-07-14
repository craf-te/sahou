//! `sahou licenses` must print the bundled third-party notices, so a single distributed binary
//! satisfies the redistribution obligation (Apache-2.0 §4) for Zenoh and the other linked crates.
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn licenses_prints_third_party_notices_including_zenoh() {
    Command::cargo_bin("sahou")
        .unwrap()
        .arg("licenses")
        .assert()
        .success()
        .stdout(predicate::str::contains("zenoh"))
        .stdout(predicate::str::contains("Apache-2.0"));
}
