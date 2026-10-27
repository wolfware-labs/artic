// The compiled card builds the tree its markup describes, its handles reach the named nodes, and
// a control crate the map names keeps its types in the registry though no view uses it.

use artic::layout::Thickness;
use artic::prelude::UiTree;
use artic_core::tree::TreeKind;
use artic_registry::Registry;
use artic_xaml_compiler_fixture_app::views::card;

// The view builds a border around a panel of two texts, with the markup's values.
#[test]
fn card_builds_its_tree() {
  let mut tree = UiTree::new();
  let view = card::build(&mut tree);
  assert_eq!(view.root.get(&tree).padding.get(), Thickness::uniform(8.0));
  assert_eq!(view.list.get(&tree).spacing.get(), 6.0);
  assert_eq!(view.title.get(&tree).text.get(), "Welcome");
  assert_eq!(view.title.get(&tree).font_size.get(), 18.0);
  assert_eq!(view.body.get(&tree).text.get(), "Signed in");
}

// The handles are the root's child and that child's children, in document order.
#[test]
fn handles_are_the_named_nodes() {
  let mut tree = UiTree::new();
  let view = card::build(&mut tree);
  let children = |node| tree.children(TreeKind::Logical, node).collect::<Vec<_>>();
  assert_eq!(children(view.root.id()), [view.list.id()]);
  assert_eq!(children(view.list.id()), [view.title.id(), view.body.id()]);
}

// The control crate the map names is linked through the generated `use` line alone.
#[test]
fn named_crate_is_linked() {
  let registry = Registry::from_link_time().expect("no duplicate types");
  assert!(
    registry
      .type_by_rust_path("artic_xaml_compiler_fixture_controls::Gauge")
      .is_some()
  );
}
