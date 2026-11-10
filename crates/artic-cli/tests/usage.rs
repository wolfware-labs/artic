use std::process::Command;

// Run with no command, `cargo artic` prints its usage and exits successfully.
#[test]
fn prints_usage() {
  let output = Command::new(env!("CARGO_BIN_EXE_cargo-artic"))
    .output()
    .expect("the binary runs");

  assert!(output.status.success(), "{output:?}");
  let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
  assert!(stdout.starts_with("Usage: cargo artic <command>"), "{stdout}");
  assert!(stdout.contains("No commands are implemented yet."), "{stdout}");
}
