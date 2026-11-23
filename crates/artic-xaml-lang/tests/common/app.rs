//! An app outside the workspace, built with `cargo` from a test, for what only a real build shows:
//! a build script compiling views, rustc errors in the views it writes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// The repository root.
pub fn repository() -> &'static Path {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .ancestors()
    .nth(2)
    .expect("the crate is two levels below the repository")
}

/// `path` as TOML and messages write it, with `/` separators.
pub fn slashes(path: &Path) -> String {
  path.to_string_lossy().replace('\\', "/")
}

/// An app named `name` that depends on artic, with `build_script` as its `build.rs`.
pub struct App {
  /// Its directory.
  pub dir: PathBuf,
}

impl App {
  /// Writes the app's manifest, build script and the workspace's lock file under the test's
  /// temporary directory, with `targets` (such as `[[bin]]` tables) in the manifest, and removes
  /// `fresh`, a folder under it whose files the test writes again.
  pub fn new(name: &str, targets: &str, build_script: &str, fresh: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let fresh = dir.join(fresh);
    if fresh.exists() {
      fs::remove_dir_all(&fresh).expect("the old files removed");
    }
    fs::create_dir_all(&fresh).expect("the app's directory");
    let artic = slashes(&repository().join("crates/artic"));
    let manifest = format!(
      "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
       [workspace]\n\n{targets}[dependencies]\nartic = {{ path = '{artic}' }}\n\n\
       [build-dependencies]\nartic = {{ path = '{artic}', features = [\"xaml-compiler\"] }}\n"
    );
    fs::write(dir.join("Cargo.toml"), manifest).expect("the manifest");
    fs::write(dir.join("build.rs"), build_script).expect("the build script");
    fs::copy(repository().join("Cargo.lock"), dir.join("Cargo.lock")).expect("the workspace's lock file");
    App { dir }
  }

  /// Runs `cargo` with `args` in the app, without the outer package's variables, and with a
  /// target directory every app shares, so artic is built once for all of them.
  pub fn cargo(&self, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO"));
    for (name, _) in std::env::vars_os() {
      let name = name.to_string_lossy();
      if OUTER_PACKAGE.iter().any(|prefix| name.starts_with(prefix)) {
        command.env_remove(name.as_ref());
      }
    }
    command
      .args(args)
      .current_dir(&self.dir)
      .env("CARGO_TARGET_DIR", Path::new(env!("CARGO_TARGET_TMPDIR")).join("apps-target"))
      .output()
      .expect("cargo runs")
  }

  /// `output`'s stderr with the app's directory written `$DIR` and `/` separators.
  pub fn stderr(&self, output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr)
      .replace('\\', "/")
      .replace(&slashes(&self.dir), "$DIR")
  }
}
