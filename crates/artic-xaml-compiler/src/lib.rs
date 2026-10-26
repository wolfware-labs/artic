//! Build-time compiler that turns XAML views into Rust.
//!
//! [`compile_str`] reads one document into an `artic_view` document, checks it with
//! `artic_view::check`, and writes the checked view as the module body that builds it: a `View`
//! struct that holds the root and one handle per `x:Name`, and a `build` function that creates
//! each element with its builder, children first. Every problem in the document is reported in
//! one pass. [`compile_dir`] does it for every view of a directory, from a build script. [`Xaml`]
//! is the language the loader and tools read XAML with.
//!
//! ```
//! use artic_registry::Registry;
//! use artic_diagnostics::SourceMap;
//!
//! let mut sources = SourceMap::new();
//! let file = sources.add("view.xaml", r#"<Border xmlns="https://artic.rs/ui"/>"#);
//! let registry = Registry::from_link_time().expect("no duplicate types");
//! let rust = artic_xaml_compiler::compile_str(&sources, file, &registry, &artic::xaml::namespaces())
//!   .expect("a valid view");
//! assert!(rust.contains("::artic::controls::Border::new()"));
//! ```

mod dir;
mod emit;
mod lower;
mod read;
mod resolve;
mod unsupported;

pub use dir::{CompileError, compile_dir, compile_into};
pub use read::Xaml;

use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap};
use artic_registry::Registry;
use artic_xaml::namespaces::NamespaceMap;

/// Compiles one document to the Rust module body for it.
///
/// The file's path in `sources` is written into the output's header and doc comments, so pass a
/// path relative to the crate, such as `src/views/login.xaml`.
///
/// # Errors
///
/// Returns the parser's errors if the document doesn't parse. Otherwise returns every problem
/// found, in source order: every unknown type or property, every literal its type refuses and
/// every construct the compiler doesn't support yet.
pub fn compile_str(
  sources: &SourceMap,
  file: SourceId,
  registry: &Registry,
  namespaces: &NamespaceMap,
) -> Result<String, Diagnostics> {
  let (document, mut diagnostics) = read::read(sources, file, registry, namespaces);
  let Some(document) = document else {
    return Err(diagnostics);
  };
  let (view, checked) = artic_view::check(&document);
  let (view, lowered) = lower::lower(&view, namespaces);
  diagnostics.extend(checked);
  diagnostics.extend(lowered);
  let diagnostics = sorted(diagnostics);
  match view {
    Some(view) if !diagnostics.has_errors() => Ok(emit::emit(&view, sources.get(file).path())),
    _ => Err(diagnostics),
  }
}

/// `diagnostics` by where each starts, keeping the order they were found in for one position.
fn sorted(diagnostics: Diagnostics) -> Diagnostics {
  let mut list: Vec<Diagnostic> = diagnostics.into_iter().collect();
  list.sort_by_key(|diagnostic| diagnostic.span.start);
  let mut sorted = Diagnostics::new();
  sorted.extend(list);
  sorted
}
