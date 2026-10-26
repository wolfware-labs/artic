//! What a UI language gives artic: a reader from its files to documents.

use crate::Document;
use artic_diagnostics::{Diagnostics, SourceId, SourceMap};
use artic_registry::Registry;

/// A UI language: what reads its files into documents.
///
/// It takes `&self` and returns owned values, so a tool can pick a `&dyn Language` by a file's
/// extension.
pub trait Language {
  /// The extension of its files, such as `ui`.
  fn extension(&self) -> &str;

  /// Reads `file` into a document, looking each type up in `registry`. Returns `None` when the
  /// file can't be read as a document, with at least one diagnostic saying why. The diagnostics
  /// are for problems that don't depend on where the checker looks; the others are the
  /// document's [`Problem`](crate::document::Problem)s.
  fn read(&self, sources: &SourceMap, file: SourceId, registry: &Registry) -> (Option<Document>, Diagnostics);

  /// The crates its configuration names, which generated code references so the app links them.
  /// A crate a view reaches only by its Rust path is linked by the app itself.
  fn crates(&self) -> Vec<&str>;
}
