mod app;

fn main() {}

#[cfg(test)]
mod tests {
  use super::app::HelloApp;
  use artic::UiTree;
  use artic::controls::{Border, StackPanel, TextBlock};
  use artic::core::tree::TreeKind;
  use artic::element::Surface;

  // The app is one window titled "Hello", holding a border around a panel of the greeting's lines.
  #[test]
  fn hello_view_tree_shape() {
    let mut tree = UiTree::new();
    let app = HelloApp::create(&mut tree);
    let children = |tree: &UiTree, node| tree.children(TreeKind::Logical, node).collect::<Vec<_>>();

    let surfaces = children(&tree, app.id());
    assert_eq!(surfaces.len(), 1);
    let surface = tree.get_as::<Surface>(surfaces[0]).expect("a surface");
    assert_eq!(surface.title.get(), "Hello");
    let [border] = children(&tree, surfaces[0])[..] else {
      panic!("the surface holds one child");
    };
    assert!(tree.get_as::<Border>(border).is_some());
    let [panel] = children(&tree, border)[..] else {
      panic!("the border holds one child");
    };
    assert!(tree.get_as::<StackPanel>(panel).is_some());
    let lines: Vec<String> = children(&tree, panel)
      .into_iter()
      .map(|node| tree.get_as::<TextBlock>(node).expect("a text block").text.get())
      .collect();
    assert_eq!(lines, ["Hello from artic", "A view written in XAML, with its code in Rust"]);
  }
}
