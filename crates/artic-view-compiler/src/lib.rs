//! Compiles the views of any UI language to Rust.
//!
//! A [`Language`] reads a file into a document. [`compile_str`] checks it with
//! `artic_view::check` and writes the checked view as the module body that builds it: a `View`
//! struct that holds the root and one handle per named element, and a `build` function that
//! creates each element with its builder, children first. Every problem in the document is
//! reported in one pass. [`compile_dir`] does it for every view of a directory, from a build
//! script. [`CratePaths`] says how generated code reaches each crate.

mod dir;
mod emit;
mod lower;
mod paths;

pub use dir::{CompileError, compile_dir, compile_into};
pub use paths::CratePaths;

use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap};
use artic_registry::Registry;
use artic_view::{CheckedView, Language};
use std::path::Path;

/// Compiles one document of `language` to the Rust module body for it.
///
/// The file's path in `sources` is written into the output's header and doc comments, so pass a
/// path relative to the crate, such as `src/views/login.ui`.
///
/// # Errors
///
/// Returns the language's errors if it can't read the file as a document. Otherwise returns every
/// problem found, in source order: every unknown type or property, every literal its type refuses
/// and every construct the compiler doesn't support yet.
pub fn compile_str(
  sources: &SourceMap,
  file: SourceId,
  language: &(impl Language + ?Sized),
  registry: &Registry,
  paths: &CratePaths,
) -> Result<String, Diagnostics> {
  let (document, mut diagnostics) = language.read(sources, file, registry);
  let Some(document) = document else {
    return Err(diagnostics);
  };
  let (view, checked) = artic_view::check(&document);
  let (rust, emitted) = emit(&view, sources.get(file).path(), paths, &language.crates());
  diagnostics.extend(checked);
  diagnostics.extend(emitted);
  let diagnostics = sorted(diagnostics);
  match rust {
    Some(rust) if !diagnostics.has_errors() => Ok(rust),
    _ => Err(diagnostics),
  }
}

/// Writes `view`, compiled from the file at `path`, as a module body, with the problems writing it
/// finds: a literal its type refuses, or a path that isn't a Rust path. `crates` are the
/// language's, which the body references so the app links them, with every crate `paths` names.
/// The body is `None` when one of those problems stops it, or the view has no root.
pub fn emit(view: &CheckedView, path: &Path, paths: &CratePaths, crates: &[&str]) -> (Option<String>, Diagnostics) {
  let (lowered, diagnostics) = lower::lower(view, paths, crates);
  let rust = lowered
    .filter(|_| !diagnostics.has_errors())
    .map(|view| emit::emit(&view, path));
  (rust, diagnostics)
}

/// `diagnostics` by where each starts, keeping the order they were found in for one position.
fn sorted(diagnostics: Diagnostics) -> Diagnostics {
  let mut list: Vec<Diagnostic> = diagnostics.into_iter().collect();
  list.sort_by_key(|diagnostic| diagnostic.span.start);
  let mut sorted = Diagnostics::new();
  sorted.extend(list);
  sorted
}
