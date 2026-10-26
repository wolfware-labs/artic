//! The view every UI language reads its files into, and the checks it goes through.
//!
//! A language's front end reads a file into a [`Document`]: its elements with their types looked
//! up in the registry, and their members as written. [`check`] finds each member on its type and
//! returns a [`CheckedView`], which a back end builds: the compiler writes it as Rust, and the
//! runtime loader will build it into a tree. A language implements [`Language`] to reach both.

mod check;
pub mod document;
mod language;
mod names;
pub mod unsupported;

pub use check::{CheckedContent, CheckedElement, CheckedView, check};
pub use document::Document;
pub use language::Language;
pub use names::field_name;
