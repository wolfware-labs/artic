//! XAML as a UI language: [`Xaml`] reads XAML files into view documents.
//!
//! `Xaml` finds each type through the URIs its `NamespaceMap` declares or a `using:` module, and
//! reads each member as written: attributes, property elements, `x:Name` and content. The
//! compiler in `artic-view-compiler` takes it from there, as it does for any UI language.
//!
//! ```
//! use artic_diagnostics::SourceMap;
//! use artic_registry::Registry;
//! use artic_xaml_lang::Xaml;
//!
//! let mut sources = SourceMap::new();
//! let file = sources.add("view.xaml", r#"<Border xmlns="https://artic.rs/ui"/>"#);
//! let registry = Registry::from_link_time().expect("no duplicate types");
//! let xaml = Xaml::new(artic::xaml::namespaces());
//! let rust = artic_view_compiler::compile_str(&sources, file, &xaml, &registry, &artic::view::crate_paths())
//!   .expect("a valid view");
//! assert!(rust.contains("::artic::controls::Border::new()"));
//! ```

mod read;
mod resolve;
mod unsupported;

pub use read::Xaml;
