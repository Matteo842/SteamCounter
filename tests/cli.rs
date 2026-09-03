use std::process::Command;

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_steamcounter"))
        .args(args)
        .output()
        .expect("avvio della CLI")
}

#[test]
fn help_is_available_without_network() {
    let output = cli(&["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("steamcounter 730"));
    assert!(help.contains("--json"));
    assert!(help.contains("--search"));
}

#[test]
fn invalid_input_fails_without_writing_json_to_stdout() {
    for args in [
        vec![],
        vec!["0", "--json"],
        vec!["76561198000000000", "--json"],
        vec![" ", "--json"],
        vec!["730", "--timeout", "0"],
        vec!["730", "--timeout", "121"],
    ] {
        let output = cli(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
}
