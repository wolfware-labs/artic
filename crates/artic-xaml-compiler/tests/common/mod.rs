//! Compiling a document in a test, against the registry the test binary links.

use artic::view::CratePaths;
use artic::xaml::NamespaceMap;
use artic_diagnostics::SourceMap;
use artic_registry::Registry;
use artic_xaml_compiler::Xaml;

/// A URI standing for the test crate, whose own `#[derive(UiType)]` types it names.
pub const TEST_UI: &str = "https://test.example/ui";

/// Compiles `xaml` as `view.xaml` with artic's map and paths and [`TEST_UI`]: the Rust, or the
/// rendered diagnostics.
pub fn compile(xaml: &str) -> Result<String, String> {
  compile_with(xaml, "view.xaml", |_, _| {})
}

/// Compiles `xaml` as `path`, with the map and paths `edit` changes after artic's and [`TEST_UI`]
/// are in them.
pub fn compile_with(xaml: &str, path: &str, edit: impl FnOnce(&mut NamespaceMap, &mut CratePaths)) -> Result<String, String> {
  let mut namespaces = artic::xaml::namespaces();
  namespaces.alias(TEST_UI, &[env!("CARGO_CRATE_NAME")]);
  let mut paths = artic::view::crate_paths();
  edit(&mut namespaces, &mut paths);
  compile_as(&Xaml::new(namespaces), xaml, path, &paths)
}

/// Compiles `xaml` as `path` with `language` and `paths`: the Rust, or the rendered diagnostics.
pub fn compile_as(language: &Xaml, xaml: &str, path: &str, paths: &CratePaths) -> Result<String, String> {
  let mut sources = SourceMap::new();
  let file = sources.add(path, xaml);
  let registry = Registry::from_link_time().expect("no duplicate types");
  artic_view_compiler::compile_str(&sources, file, language, &registry, paths).map_err(|errors| errors.render(&sources))
}
