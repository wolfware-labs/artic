//! Code types that must fail to build against real compiled views. Each case in
//! `tests/code_type_errors/` is a binary of an app whose build script compiles the views beside
//! it; `cargo check` on it must print exactly the errors in its `.stderr` file. trybuild can't run
//! these, since its projects have no build script. Regenerate the errors with `UPDATE_STDERR=1`.

mod common;

use common::app::App;
use std::fs;
use std::path::Path;

const CASES: [&str; 4] = ["wrong_code_type", "missing_code_type", "no_compiled_view", "private_field"];

/// Writes the app the cases are binaries of.
fn write_app() -> App {
  let bins: String = CASES
    .iter()
    .map(|case| format!("[[bin]]\nname = \"{case}\"\npath = \"src/bin/{case}.rs\"\n\n"))
    .collect();
  let app = App::new(
    "code-type-errors",
    &bins,
    "fn main() {\n  artic::xaml::compile_dir(\"src/bin\", &artic::xaml::namespaces()).unwrap_or_else(|error| error.exit());\n}\n",
    "src/bin",
  );
  let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/code_type_errors");
  for entry in fs::read_dir(&cases).expect("the cases") {
    let path = entry.expect("a case").path();
    if matches!(path.extension().and_then(|extension| extension.to_str()), Some("rs" | "xaml")) {
      fs::copy(&path, app.dir.join("src/bin").join(path.file_name().expect("a file name"))).expect("a case copied");
    }
  }
  app
}

/// What `cargo check` prints for `case`, from its first error on, with the app's directory written
/// `$DIR`, the build script's output directory `$OUT_DIR`, `/` separators, and cargo's summary
/// lines left out.
fn errors(app: &App, case: &str) -> String {
  let output = app.cargo(&["check", "--quiet", "--color=never", "--message-format=human", "--bin", case]);
  assert!(!output.status.success(), "{case} built");
  app
    .stderr(&output)
    .lines()
    .skip_while(|line| !line.starts_with("error"))
    .filter(|line| {
      !line.starts_with("error: could not compile")
        && !line.starts_with("warning: build failed")
        && !line.starts_with("For more information about")
        && !line.starts_with("Some errors have detailed explanations")
    })
    .map(out_dir)
    .collect::<Vec<_>>()
    .join("\n")
    .trim_end()
    .to_owned()
    + "\n"
}

/// `line` with the build script's output directory, `.../build/<package>-<hash>/out`, written
/// `$OUT_DIR`.
fn out_dir(line: &str) -> String {
  let Some(start) = line.find("/apps-target/").or_else(|| line.find("$DIR/target/")) else {
    return line.to_owned();
  };
  let start = line[..start].rfind(' ').map_or(0, |space| space + 1);
  match line[start..].find("/out/") {
    Some(end) => format!("{}$OUT_DIR{}", &line[..start], &line[start + end + "/out".len()..]),
    None => line.to_owned(),
  }
}

// Each case fails to build with the errors its `.stderr` file holds.
#[test]
fn code_type_errors() {
  let app = write_app();
  let expected_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/code_type_errors");
  let update = std::env::var_os("UPDATE_STDERR").is_some();
  let mut mismatches = Vec::new();
  for case in CASES {
    let actual = errors(&app, case);
    let path = expected_dir.join(format!("{case}.stderr"));
    if update {
      fs::write(&path, &actual).expect("the errors written");
      continue;
    }
    let expected = fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    if actual != expected {
      mismatches.push(format!("{case}:\n--- expected\n{expected}--- actual\n{actual}"));
    }
  }
  assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
