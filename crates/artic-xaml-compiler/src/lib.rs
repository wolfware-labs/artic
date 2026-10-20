//! Build-time compiler that turns XAML views into Rust.
//!
//! [`compile_str`] reads one document, resolves every type and property against the registry,
//! parses every literal, and returns the module body that builds the view: a `View` struct that
//! holds the root and one handle per `x:Name`, and a `build` function that creates each element
//! with its builder, children first. Every problem in the document is reported in one pass.
//!
//! ```
//! use artic_registry::Registry;
//! use artic_xaml::diagnostics::SourceMap;
//!
//! let mut sources = SourceMap::new();
//! let file = sources.add("view.xaml", r#"<Border xmlns="https://artic.rs/ui"/>"#);
//! let registry = Registry::from_link_time().expect("no duplicate types");
//! let rust = artic_xaml_compiler::compile_str(&sources, file, &registry, &artic::xaml::namespaces())
//!   .expect("a valid view");
//! assert!(rust.contains("::artic::controls::Border::new()"));
//! ```

mod emit;
mod lower;
mod names;
mod resolve;
mod unsupported;

use artic_registry::Registry;
use artic_xaml::diagnostics::{Diagnostics, SourceId, SourceMap};
use artic_xaml::namespaces::NamespaceMap;

/// Compiles one document to the Rust module body for it.
///
/// The file's path in `sources` is written into the output's header and doc comments, so pass a
/// path relative to the crate, such as `src/views/login.xaml`.
///
/// # Errors
///
/// Returns every problem found: the parser's, then every unknown type or property, every literal
/// its type refuses and every construct the compiler doesn't support yet.
pub fn compile_str(
  sources: &SourceMap,
  file: SourceId,
  registry: &Registry,
  namespaces: &NamespaceMap,
) -> Result<String, Diagnostics> {
  let document = artic_xaml::parse(sources, file)?;
  let view = lower::lower(&document, registry, namespaces)?;
  Ok(emit::emit(&view, sources.get(file).path()))
}
