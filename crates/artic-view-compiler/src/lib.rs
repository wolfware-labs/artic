//! Compiles the views of any UI language to Rust.
//!
//! A [`Language`] reads a file into a document. [`compile_str`] checks it with
//! `artic_view::check` and writes the checked view as the module body that builds it: a
//! `CODE_TYPE` constant, a `View` struct with one handle per named element, and the root unless
//! the view has a code type, and a `build` function that creates each element with its builder,
//! children first, and the root as the code type when it has one. Every problem in the document is
//! reported in one pass. [`compile_dir`] does it for every view of a directory, from a build
//! script. [`CratePaths`] says how generated code reaches each crate.
//!
//! # Errors in `cargo build`
//!
//! A view's problems are reported at the view's path in the crate, as rustc reports its own, with
//! the line under them: `src/views/login.ui:12:9: error: ...`. [`compile_dir`] gathers the
//! problems of every view, so one build shows them all, and a build script ends with
//! [`CompileError::exit`], which prints them on stderr, adds Cargo's one-line summary, and fails
//! the build.
//!
//! # A rustc error in a compiled view
//!
//! When the views compile but rustc refuses the code they make, such as a code type that doesn't
//! fit its view, rustc points into the generated file, `$OUT_DIR/views/src/views/login.rs`. Each
//! element's statements there follow a comment naming where the element is written:
//!
//! ```text
//! // src/views/login.ui:12:9
//! let __e3 = ::artic::controls::Border::new().padding(4.0_f32).build(tree);
//! ```
//!
//! Read up from the line rustc names to the nearest such comment, and open the view at that line
//! and column. Cargo prints `$OUT_DIR` in full, under `target/<profile>/build/<package>-<hash>/out`.

mod dir;
mod emit;
mod lower;
mod paths;

pub use dir::{CompileError, compile_dir, compile_into};
pub use paths::CratePaths;

use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap};
use artic_registry::Registry;
use artic_view::Language;

/// Compiles one document of `language` to the Rust module body for it.
///
/// The file's path in `sources` is written into the output's header and doc comments, with its
/// control characters and bidirectional controls escaped, so pass a path relative to the crate,
/// such as `src/views/login.ui`.
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
  let (view, lowered) = lower::lower(&view, paths, &language.crates());
  diagnostics.extend(checked);
  diagnostics.extend(lowered);
  let diagnostics = sorted(diagnostics);
  match view {
    Some(view) if !diagnostics.has_errors() => Ok(emit::emit(&view, sources.get(file))),
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
