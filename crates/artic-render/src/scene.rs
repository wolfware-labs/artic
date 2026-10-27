use crate::{BoxShadow, Brush, Shape, Stroke};
use artic_core::tree::NodeId;
use artic_layout::geometry::{Point, RoundedRect, Transform};
use artic_layout::text::Paragraph;
use std::sync::Arc;

/// Something that changes how what's drawn inside it appears, until the matching pop.
///
/// Every renderer must support every variant, so new variants are breaking changes on purpose: a
/// renderer stops compiling instead of silently skipping what it doesn't draw.
#[derive(Debug, Clone, PartialEq)]
pub enum Layer {
  /// Maps what's drawn inside, in its own coordinates, to the coordinates outside.
  Transform(Transform),
  /// Only draws inside the shape, in the coordinates the clip is pushed in. A shape that
  /// [draws nothing](Shape::drawable) hides everything inside.
  Clip(Shape),
  /// Draws everything inside as one group, then blends it at this opacity: 0 is clear, 1 opaque.
  /// Any value is kept, and made [drawable](drawable_opacity) when drawn.
  Opacity(f32),
}

/// A layer's opacity as it's drawn: clamped to `0..=1`, NaN counting as `1`.
///
/// A broken opacity shouldn't hide a whole subtree, so NaN is opaque here, unlike a brush's
/// opacity, where NaN paints nothing. Scenes carry a layer's opacity as it was given; every
/// renderer applies this to [`Layer::Opacity`] when it draws the layer, so hand-built scenes draw
/// by the same rule.
#[must_use]
pub fn drawable_opacity(opacity: f32) -> f32 {
  if opacity.is_nan() { 1.0 } else { opacity.clamp(0.0, 1.0) }
}

/// One recorded drawing command of a [`SceneFragment`].
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
  /// Starts a layer, ended by the matching [`Command::Pop`].
  Push(Layer),
  /// Ends the innermost layer.
  Pop,
  /// Fills `shape` with `brush`.
  Fill {
    /// The area to fill. The brush's relative points and radii resolve against its
    /// [bounds](Shape::bounds).
    shape: Shape,
    /// The paint to fill it with.
    brush: Brush,
  },
  /// Strokes the outline of `shape` with `brush`.
  Stroke {
    /// The outline to stroke. An open figure stays open. The brush's relative points and radii
    /// resolve against its [bounds](Shape::bounds), without the stroke's width.
    shape: Shape,
    /// The paint to stroke it with.
    brush: Brush,
    /// The stroke's width, joins and caps.
    stroke: Stroke,
  },
  /// Draws the shadow `rect` casts, only outside `rect`. It's drawn where it's recorded, so a
  /// shadow recorded before what casts it shows behind it.
  BoxShadow {
    /// The rounded rectangle that casts the shadow.
    rect: RoundedRect,
    /// How the shadow is grown, moved, blurred and colored.
    shadow: BoxShadow,
  },
  /// Draws `paragraph`'s glyphs with its top-left corner at `origin`, painted with `brush`.
  Text {
    /// The shaped text.
    paragraph: Paragraph,
    /// Where the paragraph's top-left corner goes, in the node's coordinates.
    origin: Point,
    /// The paint. Its relative points and radii resolve against the paragraph's box,
    /// `Rect::new(origin, paragraph.size())`, so a gradient runs across the whole text.
    brush: Brush,
  },
}

/// One node's recorded drawing, in the node's own coordinates.
///
/// Its layers are balanced: every [`Command::Push`] has its [`Command::Pop`]. A renderer may cache
/// its own encoding of a fragment under its node and generation, since a fragment never changes;
/// recording the node again makes a new fragment with a higher generation.
#[derive(Debug, PartialEq)]
pub struct SceneFragment {
  node: NodeId,
  generation: u64,
  commands: Vec<Command>,
}

impl SceneFragment {
  pub(crate) fn new(node: NodeId, generation: u64, commands: Vec<Command>) -> SceneFragment {
    SceneFragment {
      node,
      generation,
      commands,
    }
  }

  /// The node that recorded it.
  #[must_use]
  pub fn node(&self) -> NodeId {
    self.node
  }

  /// Increases each time any node is recorded again, so `(node, generation)` identifies this
  /// recording.
  #[must_use]
  pub fn generation(&self) -> u64 {
    self.generation
  }

  /// The recorded commands, in order.
  #[must_use]
  pub fn commands(&self) -> &[Command] {
    &self.commands
  }
}

/// One item of a [`Scene`]: a node's layer, the end of one, or a node's fragment.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneItem {
  /// Starts a node's layer, which also applies to its visual descendants.
  Push(Layer),
  /// Ends the innermost layer.
  Pop,
  /// A node's drawing, shared with the composition's cache rather than copied.
  Fragment(Arc<SceneFragment>),
}

/// One frame: the fragments of a visual subtree in drawing order, each wrapped in its nodes'
/// layers.
///
/// A renderer reads it from start to end. It's `Send`, so it can move to a render thread.
#[derive(Debug, Default, PartialEq)]
pub struct Scene {
  pub(crate) items: Vec<SceneItem>,
}

impl Scene {
  /// The items in drawing order. Layers are balanced.
  #[must_use]
  pub fn items(&self) -> &[SceneItem] {
    &self.items
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::SceneBuilder;
  use static_assertions::assert_impl_all;

  assert_impl_all!(Scene: Send);
  assert_impl_all!(SceneFragment: Send, Sync);

  // A recorded scene is one fragment holding what the closure drew, with no layers around it.
  #[test]
  fn record_makes_one_fragment() {
    let rect = artic_layout::geometry::Rect::new(0.0, 0.0, 2.0, 1.0);
    let red = crate::Color::rgb(255, 0, 0);

    let scene = Scene::record(|builder| builder.fill(rect, red));

    let mut expected = SceneBuilder::default();
    expected.fill(rect, red);
    let [SceneItem::Fragment(fragment)] = scene.items() else {
      panic!("one fragment, got {:?}", scene.items());
    };
    assert_eq!(fragment.commands(), expected.take());
  }
}
