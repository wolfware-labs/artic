use crate::{ContentInfo, UiType};
use artic_core::Object;
use artic_core::tree::{Handle, NodeId, UiTree};

/// A node waiting to be built, with the children to link once it is: what the generated `.child`
/// and `.children` return.
///
/// [`build`](WithChildren::build) links the children through the type's content collection, the
/// path a UI language takes, so the collection's own rules apply, such as one child for a `Child`
/// field.
#[derive(Debug)]
pub struct WithChildren<T, I> {
  node: T,
  children: I,
}

impl<T, I> WithChildren<T, I> {
  /// Holds `node` and the `children` to link when it is built.
  pub const fn new(node: T, children: I) -> Self {
    Self { node, children }
  }
}

impl<T: UiType + Object, I: IntoIterator<Item: Into<NodeId>>> WithChildren<T, I> {
  /// Adds the node to `tree`, links the children to it in order, and returns its handle.
  ///
  /// # Panics
  ///
  /// When a child isn't in the tree or is listed twice, or when the type's content isn't a
  /// collection of child elements. The checks run before the node is allocated, so a failed build
  /// leaves the tree as it was.
  #[must_use = "the handle is needed to link or reach the node"]
  pub fn build(self, tree: &mut UiTree) -> Handle<T> {
    let name = T::TYPE_INFO.name;
    let Some(ContentInfo::Collection { add_child, .. }) = T::TYPE_INFO.content else {
      panic!("`{name}` takes no child elements");
    };
    let children: Vec<NodeId> = self.children.into_iter().map(Into::into).collect();
    for (index, child) in children.iter().enumerate() {
      assert!(tree.contains(*child), "a child of the `{name}` being built isn't in the tree");
      assert!(
        !children[..index].contains(child),
        "a child of the `{name}` being built is listed twice"
      );
    }
    let node = tree.alloc_typed(self.node);
    for child in children {
      add_child(tree, node.id(), child).expect("a node just allocated has no ancestors or children to refuse a checked child");
    }
    node
  }
}
