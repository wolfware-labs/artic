//! Compiling a document in a test, against the registry the test binary links.

use artic::xaml::NamespaceMap;
use artic_diagnostics::SourceMap;
use artic_registry::Registry;

/// A URI standing for the test crate, whose own `#[derive(UiType)]` types it names.
pub const TEST_UI: &str = "https://test.example/ui";

/// Compiles `xaml` as `view.xaml` with artic's map and [`TEST_UI`]: the Rust, or the rendered
/// diagnostics.
pub fn compile(xaml: &str) -> Result<String, String> {
  compile_with(xaml, "view.xaml", |_| {})
}

/// Compiles `xaml` as `path`, with the map `edit` changes after artic's and [`TEST_UI`] are in it.
pub fn compile_with(xaml: &str, path: &str, edit: impl FnOnce(&mut NamespaceMap)) -> Result<String, String> {
  let mut namespaces = artic::xaml::namespaces();
  namespaces.alias(TEST_UI, &[env!("CARGO_CRATE_NAME")]);
  edit(&mut namespaces);
  let mut sources = SourceMap::new();
  let file = sources.add(path, xaml);
  let registry = Registry::from_link_time().expect("no duplicate types");
  artic_xaml_compiler::compile_str(&sources, file, &registry, &namespaces).map_err(|errors| errors.render(&sources))
}
