// `compile_dir`'s file work: every view under a directory compiled into a mirrored layout,
// sorted, stopping at the first view with errors.

mod common;

use artic_view_compiler::{CompileError, CratePaths, compile_into};
use common::{Named, registry};
use std::fs;
use std::path::{Path, PathBuf};

const PANEL: &str = "Panel";

/// A fresh directory for one test, with `files` written under `views/`.
fn setup(test: &str, files: &[(&str, &str)]) -> PathBuf {
  let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(&root).expect("a writable temp dir");
  for (path, text) in files {
    let path = root.join("views").join(path);
    fs::create_dir_all(path.parent().expect("a parent")).expect("a writable temp dir");
    fs::write(path, text).expect("a writable temp dir");
  }
  root
}

/// Runs `compile_into` from `dir` into `root/out`.
fn run(root: &Path, dir: &Path) -> Result<Vec<PathBuf>, CompileError> {
  compile_into(dir, &root.join("out"), &Named, &registry(), &CratePaths::new())
}

// Views in subdirectories are written to the same layout under the output, other files are
// skipped, and the files read come back sorted.
#[test]
fn nested_views_mirror_the_layout() {
  let root = setup("nested", &[("b.view", PANEL), ("a/c.view", PANEL), ("notes.txt", "x")]);
  let read = run(&root, &root.join("views")).expect("valid views");
  assert_eq!(read, [root.join("views/a/c.view"), root.join("views/b.view")]);
  assert!(root.join("out/a/c.rs").is_file());
  assert!(root.join("out/b.rs").is_file());
  assert!(!root.join("out/notes.rs").exists());
}

// The first view with errors, in sorted order, stops the run and names its file; views before it
// are already written.
#[test]
fn first_failing_view_stops_the_run() {
  let root = setup("failing", &[("a.view", PANEL), ("b.view", "Gauge"), ("c.view", PANEL)]);
  let error = run(&root, &root.join("views")).expect_err("b has errors");
  let message = error.to_string();
  assert!(message.contains("b.view") && message.contains("no type `Gauge`"), "{message}");
  assert!(root.join("out/a.rs").is_file());
  assert!(!root.join("out/c.rs").exists());
}

// A directory that doesn't exist is an I/O error naming it.
#[test]
fn missing_dir_is_an_io_error() {
  let root = setup("missing", &[]);
  let error = run(&root, &root.join("views")).expect_err("no directory");
  assert!(
    matches!(error, CompileError::Io { ref path, .. } if path.ends_with("views")),
    "{error}"
  );
}

// An absolute directory names each view relative to it, so the header has no absolute path.
#[test]
fn absolute_dir_is_shown_relative() {
  let root = setup("absolute", &[("a.view", PANEL)]);
  run(&root, &root.join("views")).expect("a valid view");
  let rust = fs::read_to_string(root.join("out/a.rs")).expect("written");
  assert!(rust.starts_with("// Generated from a.view.\n"), "{rust}");
}

// A view that's gone leaves no output behind, so an `include!` of it fails instead of building
// the old view.
#[test]
fn removed_view_leaves_no_output() {
  let root = setup("removed", &[("a.view", PANEL)]);
  run(&root, &root.join("views")).expect("a valid view");
  fs::remove_file(root.join("views/a.view")).expect("removable");
  fs::write(root.join("views/b.view"), PANEL).expect("writable");
  run(&root, &root.join("views")).expect("a valid view");
  assert!(!root.join("out/a.rs").exists());
  assert!(root.join("out/b.rs").is_file());
}

// `Debug` shows the rendered diagnostics, so a build script that unwraps the error prints them
// rather than the source map.
#[test]
fn debug_renders_the_diagnostics() {
  let root = setup("debug", &[("a.view", "Gauge")]);
  let error = run(&root, &root.join("views")).expect_err("a has errors");
  let shown = format!("{error:?}");
  assert!(shown.contains("no type `Gauge`") && !shown.contains("SourceMap"), "{shown}");
}

// A symlink to a directory isn't followed, so a link back to an ancestor ends instead of
// recursing until the stack overflows.
#[cfg(unix)]
#[test]
fn directory_symlink_is_not_followed() {
  let root = setup("symlink", &[("a.view", PANEL)]);
  std::os::unix::fs::symlink(root.join("views"), root.join("views/loop")).expect("a symlink");
  let read = run(&root, &root.join("views")).expect("a valid view");
  assert_eq!(read, [root.join("views/a.view")]);
}
