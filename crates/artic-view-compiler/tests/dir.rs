// `compile_dir`'s file work: every view under a directory compiled into a mirrored layout,
// sorted, with the errors of every view reported together.

mod common;

use artic_view_compiler::{CompileError, CratePaths, compile_dir, compile_into};
use common::{Named, Silent, registry};
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

// Every view is compiled: the errors of each failing view are reported together, at its path,
// and the views without errors are still written.
#[test]
fn every_failing_view_is_reported() {
  let root = setup(
    "failing",
    &[("a.view", PANEL), ("b.view", "Gauge"), ("c.view", PANEL), ("d.view", "Knob")],
  );
  let error = run(&root, &root.join("views")).expect_err("b and d have errors");
  let message = error.to_string();
  assert!(message.contains("b.view:1:1: error: no type `Gauge`"), "{message}");
  assert!(message.contains("d.view:1:1: error: no type `Knob`"), "{message}");
  assert!(message.find("b.view") < message.find("d.view"), "{message}");
  assert_eq!(error.summary(), "2 errors in 2 views");
  assert!(root.join("out/a.rs").is_file());
  assert!(root.join("out/c.rs").is_file());
}

// The summary counts in the singular too, and is the message itself for an error that isn't a
// view's.
#[test]
fn summary_counts_errors_and_views() {
  let root = setup("summary", &[("a.view", "Gauge")]);
  let error = run(&root, &root.join("views")).expect_err("a has an error");
  assert_eq!(error.summary(), "1 error in 1 view");
  let missing = run(&root, &root.join("absent")).expect_err("no directory");
  assert_eq!(missing.summary(), missing.to_string());
}

// A directory that doesn't exist is an I/O error naming it, and Debug, which build scripts print,
// adds the cause.
#[test]
fn missing_dir_is_an_io_error() {
  let root = setup("missing", &[]);
  let error = run(&root, &root.join("views")).expect_err("no directory");
  assert!(
    matches!(error, CompileError::Io { ref path, .. } if path.ends_with("views")),
    "{error}"
  );
  assert!(error.to_string().ends_with("views couldn't be read or written"), "{error}");
  let cause = std::error::Error::source(&error).expect("the io error");
  assert_eq!(format!("{error:?}"), format!("{error}: {cause}"));
}

// A view whose name holds a control character is refused before the output is removed or written,
// since the name would start a line of its own in Cargo's `rerun-if-changed` output. Windows can't
// name a file so.
#[cfg(not(windows))]
#[test]
fn control_character_in_a_name_is_refused() {
  let name = "a\nfn x() {}\n.view";
  let root = setup("control_character", &[("b.view", PANEL)]);
  run(&root, &root.join("views")).expect("a valid view");
  fs::write(root.join("views").join(name), PANEL).expect("writable");
  let error = run(&root, &root.join("views")).expect_err("a newline in a name");
  assert!(
    matches!(error, CompileError::ControlCharacter { ref path } if path.ends_with(name)),
    "{error}"
  );
  assert!(error.to_string().contains(r"a\nfn x() {}\n.view"), "{error}");
  assert!(root.join("out/b.rs").is_file(), "the earlier output is kept");
}

// A control character in a folder under the directory, or in the directory itself, is refused
// the same way.
#[cfg(not(windows))]
#[test]
fn control_character_in_a_folder_is_refused() {
  let root = setup("control_folder", &[("a\nb/c.view", PANEL)]);
  let error = run(&root, &root.join("views")).expect_err("a newline in a folder");
  assert!(
    matches!(error, CompileError::ControlCharacter { ref path } if path.ends_with("a\nb/c.view")),
    "{error}"
  );
  let dir = root.join("views\nx");
  fs::create_dir_all(&dir).expect("a writable temp dir");
  fs::write(dir.join("c.view"), PANEL).expect("a writable temp dir");
  let error = run(&root, &dir).expect_err("a newline in the directory");
  assert!(
    matches!(error, CompileError::ControlCharacter { ref path } if *path == dir),
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

// A directory reached through `..` names each view relative to it, so its output stays in the
// output directory.
#[test]
fn parent_relative_dir_stays_in_the_output() {
  let root = setup("parent_relative", &[("a.view", PANEL)]);
  let cwd = std::env::current_dir().expect("a current directory");
  let views = root.join("views");
  let shared = cwd
    .ancestors()
    .find(|ancestor| views.starts_with(ancestor))
    .expect("an ancestor both share");
  let dir = cwd
    .strip_prefix(shared)
    .expect("below it")
    .components()
    .map(|_| Path::new(".."))
    .collect::<PathBuf>()
    .join(views.strip_prefix(shared).expect("below it"));
  assert!(dir.starts_with(".."), "{}", dir.display());

  run(&root, &dir).expect("a valid view");

  assert!(root.join("out/a.rs").is_file());
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

// Outside a build script there is no `OUT_DIR` to write to, and `compile_dir` says so.
#[test]
fn compile_dir_needs_out_dir() {
  assert!(
    std::env::var_os("OUT_DIR").is_none(),
    "Cargo sets OUT_DIR only for build scripts"
  );

  let result = compile_dir("views", &Named, &CratePaths::new());

  assert!(matches!(result, Err(CompileError::NoOutDir)), "{result:?}");
}

/// Set in the child that `exit_reports_and_fails_the_build_script` runs, which then calls `exit`.
const EXIT_CHILD: &str = "ARTIC_EXIT_CHILD";

// `exit` ends the process with code 1, the rendered problems on stderr and Cargo's one-line summary
// on stdout. The test runs itself again as a child, which exits; the harness prints the test's name
// on the line the child's first output continues, so the summary ends a line rather than starts one.
#[test]
fn exit_reports_and_fails_the_build_script() {
  if std::env::var_os(EXIT_CHILD).is_some() {
    let root = setup("exit", &[("a.view", "Gauge")]);
    run(&root, &root.join("views")).expect_err("a has an error").exit();
  }
  let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
    .args([
      "exit_reports_and_fails_the_build_script",
      "--exact",
      "--nocapture",
      "--test-threads=1",
    ])
    .env(EXIT_CHILD, "1")
    .output()
    .expect("the test binary runs");
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert_eq!(output.status.code(), Some(1), "{stdout}\n{stderr}");
  assert!(stderr.contains("a.view:1:1: error: no type `Gauge`"), "{stderr}");
  assert!(
    stdout.lines().any(|line| line.ends_with("cargo::error=1 error in 1 view")),
    "{stdout}"
  );
}

// A view its language fails to read without reporting why still fails the build, at the view,
// rather than passing and leaving its output missing.
#[test]
fn a_view_read_with_no_error_still_fails() {
  let root = setup("silent", &[("a.view", PANEL)]);
  let error = compile_into(
    &root.join("views"),
    &root.join("out"),
    &Silent,
    &registry(),
    &CratePaths::new(),
  )
  .expect_err("a view wasn't compiled");
  let message = error.to_string();
  assert!(
    message.contains("a.view:1:1: error: this view wasn't compiled, and its language reported no error"),
    "{message}"
  );
  assert_eq!(error.summary(), "1 error in 1 view");
}
