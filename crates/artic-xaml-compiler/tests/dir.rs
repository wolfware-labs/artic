// `compile_dir`'s file work: every view under a directory compiled into a mirrored layout,
// sorted, stopping at the first view with errors.

use artic_registry::Registry;
use artic_xaml_compiler::{CompileError, compile_into};
use std::fs;
use std::path::{Path, PathBuf};

const BORDER: &str = r#"<Border xmlns="https://artic.rs/ui"/>"#;

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
  let registry = Registry::from_link_time().expect("no duplicate types");
  compile_into(dir, &root.join("out"), &registry, &artic::xaml::namespaces())
}

// Views in subdirectories are written to the same layout under the output, other files are
// skipped, and the files read come back sorted.
#[test]
fn nested_views_mirror_the_layout() {
  let root = setup("nested", &[("b.xaml", BORDER), ("a/c.xaml", BORDER), ("notes.txt", "x")]);
  let read = run(&root, &root.join("views")).expect("valid views");
  assert_eq!(read, [root.join("views/a/c.xaml"), root.join("views/b.xaml")]);
  assert!(root.join("out/a/c.rs").is_file());
  assert!(root.join("out/b.rs").is_file());
  assert!(!root.join("out/notes.rs").exists());
}

// The first view with errors, in sorted order, stops the run and names its file; views before it
// are already written.
#[test]
fn first_failing_view_stops_the_run() {
  let root = setup(
    "failing",
    &[
      ("a.xaml", BORDER),
      ("b.xaml", r#"<Gauge xmlns="https://artic.rs/ui"/>"#),
      ("c.xaml", BORDER),
    ],
  );
  let error = run(&root, &root.join("views")).expect_err("b has errors");
  let message = error.to_string();
  assert!(message.contains("b.xaml") && message.contains("no type `Gauge`"), "{message}");
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
  let root = setup("absolute", &[("a.xaml", BORDER)]);
  run(&root, &root.join("views")).expect("a valid view");
  let rust = fs::read_to_string(root.join("out/a.rs")).expect("written");
  assert!(rust.starts_with("// Generated from a.xaml.\n"), "{rust}");
}
