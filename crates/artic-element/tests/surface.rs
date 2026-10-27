// Application, Surface and the Desktop settings: what a surface says to accessibility, how a
// setting on the application reaches it, and how it lays out its content.

use artic_core::property::Affects;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{AccessCx, Application, Chrome, Desktop, ElementTypes, SizeToContent, Surface};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{LayoutManager, PixelGrid};
use artic_text::Engine;
use pretty_assertions::assert_eq;

/// An application holding one surface, and the surface.
fn application(tree: &mut UiTree) -> (NodeId, NodeId) {
  let application = tree.alloc(Box::new(Application::default()));
  let surface = tree.alloc(Box::new(Surface::default()));
  for kind in [TreeKind::Logical, TreeKind::Visual] {
    tree.add_child(kind, application, surface);
  }
  (application, surface)
}

// A Surface is a window to accessibility, named by its Title, and renamed when the Title changes.
#[test]
fn surface_accesskit_node() {
  let mut tree = UiTree::new();
  let (_, surface) = application(&mut tree);
  let types = ElementTypes::from_link_time();
  let title = tree.get_as::<Surface>(surface).expect("a surface").title;
  title.set("Notes".to_owned());
  let describe = |tree: &UiTree| {
    let element = types
      .element(tree.get(surface).expect("live"))
      .expect("Surface is an element");
    element.accessibility(&AccessCx::new(tree, surface, Rect::new(0.0, 0.0, 100.0, 50.0)))
  };

  let first = describe(&tree);
  title.set("Inspector".to_owned());
  let renamed = describe(&tree);

  assert_eq!(first.role(), accesskit::Role::Window);
  assert_eq!(first.label(), Some("Notes"));
  assert_eq!(renamed.label(), Some("Inspector"));
}

// A surface is open until something closes it, and Title and IsOpen mark it for reconciling.
#[test]
fn surface_is_open_by_default_and_marks_surface_work() {
  let mut tree = UiTree::new();
  let (_, surface) = application(&mut tree);
  let node = tree.get_as::<Surface>(surface).expect("a surface");
  let (title, is_open) = (node.title, node.is_open);
  assert!(is_open.get());
  tree.clear_dirty(surface, Affects::LAYOUT | Affects::RENDER);

  title.set("Notes".to_owned());

  assert!(tree.dirty(surface).contains(Affects::SURFACE));
  assert!(tree.dirty(surface).contains(Affects::RENDER));
}

// Chrome set on the Application reaches each Surface through inheritance, and a Surface's own
// value wins.
#[test]
fn desktop_chrome_inherits_from_application() {
  let mut tree = UiTree::new();
  let (application, surface) = application(&mut tree);

  Desktop::set_chrome(&mut tree, application, Chrome::None);
  assert_eq!(Desktop::chrome(&tree, surface), Chrome::None);
  assert_eq!(Desktop::size_to_content(&tree, surface), SizeToContent::Manual);

  Desktop::set_chrome(&mut tree, surface, Chrome::System);
  assert_eq!(Desktop::chrome(&tree, surface), Chrome::System);
  assert_eq!(Desktop::width(&tree, surface), None);
}

// SizeToContent says which axes content sizes.
#[test]
fn size_to_content_axes() {
  let axes = |value: SizeToContent| (value.width(), value.height());

  assert_eq!(axes(SizeToContent::Manual), (false, false));
  assert_eq!(axes(SizeToContent::Width), (true, false));
  assert_eq!(axes(SizeToContent::Height), (false, true));
  assert_eq!(axes(SizeToContent::WidthAndHeight), (true, true));
  assert_eq!("widthandheight".parse(), Ok(SizeToContent::WidthAndHeight));
}

// A Surface fills its content into its whole box and wants its content's size.
#[test]
fn surface_lays_out_its_content() {
  let mut tree = UiTree::new();
  let (_, surface) = application(&mut tree);
  let content = tree.alloc(Box::new(Surface::default()));
  tree.add_child(TreeKind::Visual, surface, content);
  let mut layout = LayoutManager::default();

  layout.run(
    &mut tree,
    &ElementTypes::from_link_time(),
    surface,
    Size::new(200.0, 100.0),
    PixelGrid { scale: 1.0, snap: false },
    &mut Engine::without_fonts(),
  );

  assert_eq!(layout.rect(content), Some(Rect::new(0.0, 0.0, 200.0, 100.0)));
}

// An application takes its surfaces as content, each linked in both trees.
#[test]
fn application_builder_takes_surfaces() {
  let mut tree = UiTree::new();
  let main = Surface::new().title("Main").build(&mut tree);
  let tools = Surface::new().title("Tools").build(&mut tree);

  let application = Application::new().children([main, tools]).build(&mut tree);

  for kind in [TreeKind::Logical, TreeKind::Visual] {
    assert_eq!(
      tree.children(kind, application.id()).collect::<Vec<NodeId>>(),
      [main.id(), tools.id()]
    );
  }
}

// A surface takes one child as content, linked in both trees.
#[test]
fn surface_builder_takes_one_child() {
  let mut tree = UiTree::new();
  let content = Surface::new().build(&mut tree);

  let surface = Surface::new().child(content).build(&mut tree);

  for kind in [TreeKind::Logical, TreeKind::Visual] {
    assert_eq!(tree.children(kind, surface.id()).collect::<Vec<NodeId>>(), [content.id()]);
  }
}
