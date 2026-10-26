//! XAML seen through the `Language` trait.

use artic_diagnostics::SourceMap;
use artic_registry::Registry;
use artic_view::Language;
use artic_xaml_compiler::Xaml;

// XAML reads a file into a document whose root type is looked up, and names its extension and
// the crates its URIs stand for.
#[test]
fn xaml_reads_a_document() {
  let xaml = Xaml::new(artic::xaml::namespaces());
  let mut sources = SourceMap::new();
  let file = sources.add("view.xaml", r#"<Border xmlns="https://artic.rs/ui" Padding="4"/>"#);
  let registry = Registry::from_link_time().expect("no duplicate types");

  let (document, diagnostics) = xaml.read(&sources, file, &registry);

  assert!(diagnostics.is_empty(), "{diagnostics}");
  let root = document.expect("a document").root;
  assert_eq!(root.ty.expect("Border is found").info.name, "Border");
  assert_eq!(root.members.len(), 1);
  assert_eq!(xaml.extension(), "xaml");
  assert!(xaml.crates().contains(&"artic_controls"), "{:?}", xaml.crates());
}

// A file that doesn't parse gives no document, with the parser's errors.
#[test]
fn xaml_that_does_not_parse_gives_no_document() {
  let xaml = Xaml::new(artic::xaml::namespaces());
  let mut sources = SourceMap::new();
  let file = sources.add("view.xaml", "<Border");
  let registry = Registry::from_link_time().expect("no duplicate types");

  let (document, diagnostics) = xaml.read(&sources, file, &registry);

  assert!(document.is_none());
  assert!(diagnostics.has_errors());
}
