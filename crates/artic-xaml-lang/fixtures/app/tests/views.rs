// The compiled card builds the tree its markup describes, its handles reach the named nodes, and
// a control crate the map names keeps its types in the registry though no view uses it.

use artic::layout::Thickness;
use artic::prelude::UiTree;
use artic_core::tree::TreeKind;
use artic_registry::Registry;
use artic_xaml_compiler_fixture_app::email;
use artic_xaml_compiler_fixture_app::views::card;
use artic_xaml_compiler_fixture_app::views::login::LoginView;

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

// `create` puts the code type at the root of the tree, with the view's root content as its only
// child, the root's attributes set on it, and its own fields at their defaults.
#[test]
fn build_returns_code_type_root() {
  let mut tree = UiTree::new();

  let login = LoginView::create(&mut tree);

  let children = tree.children(TreeKind::Logical, login.id()).collect::<Vec<_>>();
  assert_eq!(tree.parent(TreeKind::Logical, login.id()), None);
  assert_eq!(children.len(), 1);
  assert_eq!(login.get(&tree).layout.margin.get(), Thickness::uniform(16.0));
  assert_eq!(login.get(&tree).attempts, 0);
}

// The code type's handles are the panel's children, in document order, with the markup's text:
// one reached from another module through its `pub(crate)` field, one through the code type's
// own module.
#[test]
fn code_type_handles_are_the_named_nodes() {
  let mut tree = UiTree::new();
  let login = LoginView::create(&mut tree);
  let view = login.get(&tree);
  let (email, status) = (email(view), view.status());

  let panel = tree
    .children(TreeKind::Logical, login.id())
    .next()
    .expect("the root's content");
  let named = tree.children(TreeKind::Logical, panel).collect::<Vec<_>>();
  assert_eq!(named, [email.id(), status.id()]);
  assert_eq!(email.get(&tree).text.get(), "Email");
  assert_eq!(status.get(&tree).text.get(), "Signed out");
}

// The registry knows the code type by the path that defines it, and can't create it from markup.
#[test]
fn code_type_has_no_new() {
  let registry = Registry::from_link_time().expect("no duplicate types");

  let info = registry
    .type_by_rust_path("artic_xaml_compiler_fixture_app::views::login::LoginView")
    .expect("registered");

  assert!(info.new.is_none());
}
