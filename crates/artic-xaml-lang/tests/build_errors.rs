//! A real `cargo build` of an app whose views have errors: the build fails, and its output shows
//! every error of every view, rendered at its path in the crate, then Cargo's one-line summary.

mod common;

use common::app::App;
use std::fs;

// Two broken views, one with two errors, all reported in one build at `path:line:col`, with the
// source line and a caret under it, and Cargo's summary after them.
#[test]
fn every_view_error_reaches_cargo_build() {
  let app = App::new(
    "build-errors",
    "",
    "fn main() {\n  artic::xaml::compile_dir(\"src/views\", &artic::xaml::namespaces()).unwrap_or_else(|error| error.exit());\n}\n",
    "src/views",
  );
  fs::write(app.dir.join("src/main.rs"), "fn main() {}\n").expect("the app's main");
  fs::write(
    app.dir.join("src/views/login.xaml"),
    "<StackPanel xmlns=\"https://artic.rs/ui\">\n  <Border Foo=\"1\"/>\n  <TextBlock Bar=\"2\"/>\n</StackPanel>\n",
  )
  .expect("a view");
  fs::write(
    app.dir.join("src/views/settings.xaml"),
    "<Border xmlns=\"https://artic.rs/ui\" Padding=\"wide\"/>\n",
  )
  .expect("a view");

  let output = app.cargo(&["build", "--quiet", "--color=never"]);
  let stderr = app.stderr(&output);

  assert!(!output.status.success(), "{stderr}");
  for expected in [
    "src/views/login.xaml:2:11: error: `Border` has no property `Foo`",
    "2 |   <Border Foo=\"1\"/>",
    "src/views/login.xaml:3:14: error: `TextBlock` has no property `Bar`",
    "src/views/settings.xaml:1:46: error: invalid Thickness 'wide'",
    "error: build-errors@0.0.0: 3 errors in 2 views",
  ] {
    assert!(stderr.contains(expected), "missing {expected:?} in:\n{stderr}");
  }
  assert!(!stderr.contains("panicked"), "{stderr}");
}
