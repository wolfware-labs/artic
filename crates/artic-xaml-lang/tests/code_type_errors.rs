//! Code types that must fail to build against real compiled views. Each case in
//! `tests/code_type_errors/` is a binary of an app whose build script compiles the views beside
//! it; `cargo check` on it must print exactly the errors in its `.stderr` file. trybuild can't run
//! these, since its projects have no build script. Regenerate the errors with `UPDATE_STDERR=1`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The variables cargo sets for the package whose tests run, or that a coverage run sets for its
/// own profiles, by name or prefix. The app is another package, so it gets none of them; the
/// user's configuration, such as `CARGO_NET_OFFLINE` or a linker, stays.
const OUTER_PACKAGE: [&str; 11] = [
  "CARGO_MANIFEST_",
  "CARGO_PKG_",
  "CARGO_CRATE_NAME",
  "CARGO_BIN_",
  "CARGO_PRIMARY_PACKAGE",
  "CARGO_TARGET_TMPDIR",
  "CARGO_TARGET_DIR",
  "CARGO_RUSTC_CURRENT_DIR",
  "CARGO_LLVM_COV",
  "OUT_DIR",
  "LLVM_PROFILE_FILE",
];

const CASES: [&str; 4] = ["wrong_code_type", "missing_code_type", "no_compiled_view", "private_field"];

/// The repository root.
fn repository() -> &'static Path {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .nth(2)
    .expect("the crate is two levels below the repository")
}

/// `path` as TOML and messages write it, with `/` separators.
fn slashes(path: &Path) -> String {
  path.to_string_lossy().replace('\\', "/")
}

/// Writes the app the cases are binaries of, and returns its directory.
fn write_app() -> PathBuf {
  let app = Path::new(env!("CARGO_TARGET_TMPDIR")).join("code-type-errors");
  let bin = app.join("src/bin");
  if bin.exists() {
    fs::remove_dir_all(&bin).expect("the old cases removed");
  }
  fs::create_dir_all(&bin).expect("the app's directory");
  let artic = slashes(&repository().join("crates/artic"));
  let bins: String = CASES
    .iter()
    .map(|case| format!("[[bin]]\nname = \"{case}\"\npath = \"src/bin/{case}.rs\"\n\n"))
    .collect();
  let manifest = format!(
    "[package]\nname = \"code-type-errors\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
     [workspace]\n\n{bins}[dependencies]\nartic = {{ path = '{artic}' }}\n\n\
     [build-dependencies]\nartic = {{ path = '{artic}', features = [\"xaml-compiler\"] }}\n"
  );
  fs::write(app.join("Cargo.toml"), manifest).expect("the manifest");
  fs::write(
    app.join("build.rs"),
    "fn main() {\n  artic::xaml::compile_dir(\"src/bin\", &artic::xaml::namespaces()).unwrap_or_else(|error| panic!(\"{error:?}\"));\n}\n",
  )
  .expect("the build script");
  fs::copy(repository().join("Cargo.lock"), app.join("Cargo.lock")).expect("the workspace's lock file");
  let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/code_type_errors");
  for entry in fs::read_dir(&cases).expect("the cases") {
    let path = entry.expect("a case").path();
    if matches!(path.extension().and_then(|extension| extension.to_str()), Some("rs" | "xaml")) {
      fs::copy(&path, bin.join(path.file_name().expect("a file name"))).expect("a case copied");
    }
  }
  app
}

/// What `cargo check` prints for `case`, from its first error on, with the app's directory written
/// `$DIR`, the build script's output directory `$OUT_DIR`, `/` separators, and cargo's summary
/// lines left out.
fn errors(app: &Path, case: &str) -> String {
  let mut command = Command::new(env!("CARGO"));
  for (name, _) in std::env::vars_os() {
    let name = name.to_string_lossy();
    if OUTER_PACKAGE.iter().any(|prefix| name.starts_with(prefix)) {
      command.env_remove(name.as_ref());
    }
  }
  let output = command
    .args(["check", "--quiet", "--color=never", "--message-format=human", "--bin", case])
    .current_dir(app)
    .env("CARGO_TARGET_DIR", app.join("target"))
    .output()
    .expect("cargo runs");
  assert!(!output.status.success(), "{case} built");
  let stderr = String::from_utf8_lossy(&output.stderr).replace('\\', "/");
  let directory = slashes(app);
  stderr
    .lines()
    .skip_while(|line| !line.starts_with("error"))
    .filter(|line| {
      !line.starts_with("error: could not compile")
        && !line.starts_with("warning: build failed")
        && !line.starts_with("For more information about")
        && !line.starts_with("Some errors have detailed explanations")
    })
    .map(|line| out_dir(&line.replace(&directory, "$DIR")))
    .collect::<Vec<_>>()
    .join("\n")
    .trim_end()
    .to_owned()
    + "\n"
}

/// `line` with the build script's output directory, `$DIR/target/.../build/<package>-<hash>/out`,
/// written `$OUT_DIR`.
fn out_dir(line: &str) -> String {
  let Some(start) = line.find("$DIR/target/") else {
    return line.to_owned();
  };
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
