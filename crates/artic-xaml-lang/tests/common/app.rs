//! An app outside the workspace, built with `cargo` from a test, for what only a real build shows:
//! a build script compiling views, rustc errors in the views it writes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The variables cargo sets for the package whose tests run, or that a coverage run sets for its
/// own profiles and rustc wrapper, by name or prefix. The app is another package, so it gets none
/// of them; the user's configuration, such as `CARGO_NET_OFFLINE` or a linker, stays.
const OUTER_PACKAGE: [&str; 12] = [
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
  "__CARGO_LLVM_COV",
];

/// Set when a coverage run made itself `RUSTC_WRAPPER`, to instrument the workspace's crates.
const COVERAGE_WRAPPER: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER";

/// The `RUSTC_WRAPPER` the user had before the coverage run replaced it, such as `sccache`.
const USER_WRAPPER: &str = "__CARGO_LLVM_COV_RUSTC_WRAPPER_PRE_EXISTING";

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

/// The target directory every app shares, so artic is built once for all of them.
fn target() -> PathBuf {
  Path::new(env!("CARGO_TARGET_TMPDIR")).join("apps-target")
}

/// An app that depends on artic, whose build script compiles its views with
/// `artic::xaml::compile_dir` and ends with `CompileError::exit` when they have errors.
pub struct App {
  /// Its directory.
  pub dir: PathBuf,
}

impl App {
  /// Writes the app `name`'s manifest, with `targets` (such as `[[bin]]` tables) in it, its build
  /// script compiling the views under `views`, and the workspace's lock file, under the test's
  /// temporary directory. `views` is emptied, for the test to write the views again.
  pub fn new(name: &str, targets: &str, views: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let views_dir = dir.join(views);
    if views_dir.exists() {
      fs::remove_dir_all(&views_dir).expect("the old views removed");
    }
    fs::create_dir_all(&views_dir).expect("the app's directory");
    let artic = slashes(&repository().join("crates/artic"));
    let manifest = format!(
      "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
       [workspace]\n\n{targets}[dependencies]\nartic = {{ path = '{artic}' }}\n\n\
       [build-dependencies]\nartic = {{ path = '{artic}', features = [\"xaml-compiler\"] }}\n"
    );
    let build_script = format!(
      "fn main() {{\n  artic::xaml::compile_dir(\"{views}\", &artic::xaml::namespaces())\n    \
       .unwrap_or_else(|error| error.exit());\n}}\n"
    );
    fs::write(dir.join("Cargo.toml"), manifest).expect("the manifest");
    fs::write(dir.join("build.rs"), build_script).expect("the build script");
    fs::copy(repository().join("Cargo.lock"), dir.join("Cargo.lock")).expect("the workspace's lock file");
    App { dir }
  }

  /// Runs `cargo` with `args` in the app, without the outer package's variables and with the
  /// user's own rustc wrapper in place of a coverage run's, in the `target` directory every app
  /// shares.
  pub fn cargo(&self, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO"));
    for (name, _) in std::env::vars_os() {
      let name = name.to_string_lossy();
      if OUTER_PACKAGE.iter().any(|prefix| name.starts_with(prefix)) {
        command.env_remove(name.as_ref());
      }
    }
    if std::env::var_os(COVERAGE_WRAPPER).is_some() {
      match std::env::var_os(USER_WRAPPER) {
        Some(wrapper) => command.env("RUSTC_WRAPPER", wrapper),
        None => command.env_remove("RUSTC_WRAPPER"),
      };
    }
    command
      .args(args)
      .current_dir(&self.dir)
      .env("CARGO_TARGET_DIR", target())
      .output()
      .expect("cargo runs")
  }

  /// `output`'s stderr with `/` separators, the app's directory written `$DIR` and the shared
  /// target directory `$TARGET`.
  pub fn stderr(&self, output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr)
      .replace('\\', "/")
      .replace(&slashes(&self.dir), "$DIR")
      .replace(&slashes(&target()), "$TARGET")
  }
}
