// `compile_dir` as a build script calls it, with `OUT_DIR` set. Setting it is sound only while no
// other thread reads the environment, so this test runs without the harness, on the main thread
// alone.

mod common;

use artic_view_compiler::{CompileError, CratePaths, compile_dir};
use common::Named;
use std::fs;
use std::path::Path;

// A directory with no views replaces `$OUT_DIR/views/` with nothing, and a directory whose path
// holds a newline is refused before anything is printed for Cargo. Windows can't name a directory
// so.
fn main() {
  let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compile_dir");
  let _ = fs::remove_dir_all(&root);
  let out = root.join("out");
  fs::create_dir_all(out.join("views/stale")).expect("a writable temp dir");
  let views = root.join("views");
  fs::create_dir_all(&views).expect("a writable temp dir");
  // SAFETY: no other thread has started, so nothing reads the environment meanwhile.
  unsafe { std::env::set_var("OUT_DIR", &out) };

  compile_dir(&views, &Named, &CratePaths::new()).expect("an empty directory compiles");
  assert!(!out.join("views").exists(), "the earlier output is removed");

  if cfg!(not(windows)) {
    let bad = root.join("views\nx");
    fs::create_dir_all(&bad).expect("a writable temp dir");
    let error = compile_dir(&bad, &Named, &CratePaths::new()).expect_err("a newline in the directory");
    assert!(
      matches!(error, CompileError::ControlCharacter { ref path } if path.ends_with("views\nx")),
      "{error}"
    );
  }
}
