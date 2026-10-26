//! Errors and warnings about source files, with the position they refer to.
//!
//! Every part of artic that reads a view (a language's parser, the compiler, the loader and the
//! language server) reports through these types, so problems look the same wherever they are
//! found.

mod diagnostic;
mod render;
mod source;

pub use diagnostic::{Diagnostic, Diagnostics, Label, Severity};
pub use source::{LineCol, SourceFile, SourceId, SourceMap, Span};
