//! Compiles the views under `src/views` with artic's map and the fixture control crates' URIs.

use artic_xaml_compiler_fixture_controls as _;
use artic_xaml_compiler_fixture_docking as _;

fn main() {
  let mut namespaces = artic::xaml::namespaces();
  namespaces.alias("https://fixture.example/controls", &["artic_xaml_compiler_fixture_controls"]);
  namespaces.alias("https://fixture.example/docking", &["artic_xaml_compiler_fixture_docking"]);
  artic::xaml::compile_dir("src/views", &namespaces).unwrap_or_else(|error| error.exit());
}
