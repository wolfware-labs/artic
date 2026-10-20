//! Compiles the views under `src/views` with artic's map and the fixture control crate's URI.

use artic_xaml_compiler_fixture_controls as _;

fn main() {
  let mut namespaces = artic::xaml::namespaces();
  namespaces.alias("https://fixture.example/controls", &["artic_xaml_compiler_fixture_controls"]);
  artic::xaml::compile_dir("src/views", &namespaces).unwrap_or_else(|error| panic!("{error}"));
}
