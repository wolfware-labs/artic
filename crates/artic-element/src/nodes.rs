use crate::{ElementTypes, RenderCx};
use artic_core::tree::{NodeId, UiTree};
use artic_layout::LayoutManager;
use artic_layout::geometry::Transform;
use artic_render::{Layers, Nodes, SceneBuilder};

/// Composition of laid-out elements: each node's element draws at the size layout arranged it at,
/// with what its layout kept, and its layers carry its layout offset after its own transform. A
/// node layout never arranged isn't drawn.
#[derive(Debug, Clone, Copy)]
pub struct ElementNodes<'a> {
  /// Finds each node's element.
  pub types: &'a ElementTypes,
  /// The run that sized and placed the nodes.
  pub layout: &'a LayoutManager,
}

impl Nodes for ElementNodes<'_> {
  fn layers(&self, tree: &UiTree, node: NodeId) -> Layers {
    let Some(rect) = self.layout.rect(node) else {
      return Layers {
        visible: false,
        ..Layers::default()
      };
    };
    let mut layers = tree
      .get(node)
      .and_then(|object| self.types.element(object))
      .map_or_else(Layers::default, |element| element.layers(rect.size()));
    let offset = (rect.x != 0.0 || rect.y != 0.0).then(|| Transform::translate(rect.x, rect.y));
    layers.transform = match (layers.transform, offset) {
      (Some(own), Some(offset)) => Some(own.then(offset)),
      (own, offset) => own.or(offset),
    };
    layers
  }

  fn record(&mut self, tree: &UiTree, node: NodeId, builder: &mut SceneBuilder) {
    let Some(rect) = self.layout.rect(node) else {
      return;
    };
    if let Some(element) = tree.get(node).and_then(|object| self.types.element(object)) {
      element.render(&mut RenderCx::new(tree, node, rect.size(), self.layout.result(node)), builder);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{AccessCx, Element, RenderCx};
  use artic_core::Object;
  use artic_core::property::PropertyVisitor;
  use artic_core::tree::TreeKind;
  use artic_layout::geometry::{Rect, Size};
  use artic_layout::{ArrangeCx, Layoutable, MeasureCx, PixelGrid};
  use pretty_assertions::assert_eq;

  /// A 10 by 10 leaf that fills its arranged size, with `transform` as its own layer.
  #[derive(Debug)]
  struct Spot {
    transform: Option<Transform>,
  }

  impl Object for Spot {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  impl Layoutable for Spot {
    fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
      Size::new(10.0, 10.0)
    }

    fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
  }

  impl Element for Spot {
    fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
      let size = cx.size();
      scene.fill(
        Rect::new(0.0, 0.0, size.width, size.height),
        artic_render::Color::rgb(0, 0, 0),
      );
    }

    fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
      accesskit::Node::new(accesskit::Role::GenericContainer)
    }

    fn layers(&self, _size: Size) -> artic_render::Layers {
      artic_render::Layers {
        transform: self.transform,
        ..artic_render::Layers::default()
      }
    }
  }

  crate::element_type!(Spot);

  /// A parent that places its one child at `at`, at the child's desired size.
  #[derive(Debug)]
  struct Place {
    at: (f32, f32),
  }

  impl Object for Place {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  impl Layoutable for Place {
    fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
      for child in cx.children() {
        cx.measure(child, available);
      }
      available
    }

    fn arrange(&self, cx: &mut ArrangeCx<'_>, _rect: Rect) {
      for child in cx.children() {
        let desired = cx.desired(child);
        cx.arrange(child, Rect::new(self.at.0, self.at.1, desired.width, desired.height));
      }
    }
  }

  impl Element for Place {
    fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

    fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
      accesskit::Node::new(accesskit::Role::GenericContainer)
    }
  }

  crate::element_type!(Place);

  /// A 10 by 10 leaf whose measure keeps "shaped" as its layout result, and which draws only when
  /// render reads it back.
  #[derive(Debug)]
  struct Shaped;

  impl Object for Shaped {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  impl Layoutable for Shaped {
    fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
      cx.set_measured("shaped");
      Size::new(10.0, 10.0)
    }

    fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
  }

  impl Element for Shaped {
    fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
      if cx.layout_result::<&str>() == Some(&"shaped") {
        scene.fill(Rect::new(0.0, 0.0, 1.0, 1.0), artic_render::Color::rgb(0, 0, 0));
      }
    }

    fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
      accesskit::Node::new(accesskit::Role::GenericContainer)
    }
  }

  crate::element_type!(Shaped);

  /// A `Place` at `at` holding a `Spot` with `transform`, laid out in 100 by 100, and a second
  /// `Spot` layout never reached: the tree, the placed spot, the unreached one and the layout.
  fn laid_out(at: (f32, f32), transform: Option<Transform>) -> (UiTree, NodeId, NodeId, LayoutManager) {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(Place { at }));
    let spot = tree.alloc(Box::new(Spot { transform }));
    let unreached = tree.alloc(Box::new(Spot { transform: None }));
    tree.add_child(TreeKind::Visual, root, spot);
    let mut layout = LayoutManager::default();
    let grid = PixelGrid { scale: 1.0, snap: false };
    let types = ElementTypes::from_link_time();
    layout.run(
      &mut tree,
      &types,
      root,
      Size::new(100.0, 100.0),
      grid,
      &mut artic_text::Engine::with_fonts([]),
    );
    (tree, spot, unreached, layout)
  }

  // A node layout never arranged isn't drawn.
  #[test]
  fn an_unarranged_node_is_invisible() {
    let (tree, _, unreached, layout) = laid_out((0.0, 0.0), None);
    let types = ElementTypes::from_link_time();

    let layers = ElementNodes {
      types: &types,
      layout: &layout,
    }
    .layers(&tree, unreached);

    assert!(!layers.visible);
  }

  // The element's own transform applies first, then the offset layout placed it at.
  #[test]
  fn layers_put_the_own_transform_before_the_offset() {
    let own = Transform::scale(2.0, 2.0);
    let (tree, spot, _, layout) = laid_out((5.0, 7.0), Some(own));
    let types = ElementTypes::from_link_time();

    let layers = ElementNodes {
      types: &types,
      layout: &layout,
    }
    .layers(&tree, spot);

    assert_eq!(layers.transform, Some(own.then(Transform::translate(5.0, 7.0))));
  }

  // At the origin there is no offset, so only the element's own transform is left.
  #[test]
  fn a_node_at_the_origin_keeps_its_own_transform_only() {
    let own = Transform::scale(2.0, 2.0);
    let (tree, spot, _, layout) = laid_out((0.0, 0.0), Some(own));
    let types = ElementTypes::from_link_time();

    let layers = ElementNodes {
      types: &types,
      layout: &layout,
    }
    .layers(&tree, spot);

    assert_eq!(layers.transform, Some(own));
  }

  // A node layout never arranged records nothing, even when called directly.
  #[test]
  fn an_unarranged_node_records_nothing() {
    let (tree, _, unreached, layout) = laid_out((0.0, 0.0), None);
    let types = ElementTypes::from_link_time();
    let mut builder = SceneBuilder::default();

    ElementNodes {
      types: &types,
      layout: &layout,
    }
    .record(&tree, unreached, &mut builder);

    assert_eq!(format!("{builder:?}"), format!("{:?}", SceneBuilder::default()));
  }

  // An element draws at the size layout arranged it at.
  #[test]
  fn record_draws_at_the_arranged_size() {
    let (tree, spot, _, layout) = laid_out((5.0, 7.0), None);
    let types = ElementTypes::from_link_time();
    let mut builder = SceneBuilder::default();

    ElementNodes {
      types: &types,
      layout: &layout,
    }
    .record(&tree, spot, &mut builder);

    let mut expected = SceneBuilder::default();
    expected.fill(Rect::new(0.0, 0.0, 10.0, 10.0), artic_render::Color::rgb(0, 0, 0));
    assert_eq!(format!("{builder:?}"), format!("{expected:?}"));
  }

  // An element draws with what its layout kept, such as a shaped paragraph.
  #[test]
  fn record_passes_the_layout_result() {
    let mut tree = UiTree::new();
    let node = tree.alloc(Box::new(Shaped));
    let mut layout = LayoutManager::default();
    let types = ElementTypes::from_link_time();
    let grid = PixelGrid { scale: 1.0, snap: false };
    layout.run(
      &mut tree,
      &types,
      node,
      Size::new(10.0, 10.0),
      grid,
      &mut artic_text::Engine::with_fonts([]),
    );
    let mut builder = SceneBuilder::default();

    ElementNodes {
      types: &types,
      layout: &layout,
    }
    .record(&tree, node, &mut builder);

    assert_ne!(format!("{builder:?}"), format!("{:?}", SceneBuilder::default()));
  }
}
