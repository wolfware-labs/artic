//! The traits an element implements: [`Element`] for the frame pipeline, [`ElementImpl`] for the
//! control author, and [`HasState`] for the state structs a control embeds.

use crate::context::{AccessCx, EventCx, Handled, HitTestCx, HitTestResult, RenderCx};
use artic_core::Object;
use artic_core::input::InputEvent;
use artic_layout::geometry::{Point, Size};
use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
use artic_render::{Layers, SceneBuilder, Shape};

/// A node the frame pipeline lays out, draws, hit-tests, sends input to and describes to
/// assistive technology.
///
/// [`Layoutable`] brings `measure` and `arrange`. Controls don't implement this trait by hand:
/// `#[element]` writes it around their [`ElementImpl`], with the shared work of the property
/// groups.
pub trait Element: Object + Layoutable {
  /// Records the element's own drawing, in its own coordinates, without its descendants.
  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder);

  /// Whether `point`, in the element's own coordinates, hits it. By default, inside its arranged
  /// bounds.
  fn hit_test(&self, cx: &HitTestCx<'_>, point: Point) -> HitTestResult {
    cx.bounds_test(point)
  }

  /// Handles one input event. By default, nothing is handled.
  fn on_event(&mut self, cx: &mut EventCx<'_>, event: &InputEvent) -> Handled {
    let _ = (cx, event);
    Handled::No
  }

  /// The element's accessibility node: its role, name, value, state, bounds and actions.
  fn accessibility(&self, cx: &AccessCx<'_>) -> accesskit::Node;

  /// The element's layers in composition, for its arranged `size`: transform, clip and opacity.
  /// By default, none.
  fn layers(&self, size: Size) -> Layers {
    let _ = size;
    Layers::default()
  }
}

/// What a control writes: only what is special about it. Every method has a default.
///
/// `#[element]` calls these from its [`Element`] implementation, after the shared work of the
/// property groups: margin, size limits and alignment around `measure_override` and
/// `arrange_override`, from [`Layout`](crate::Layout); visibility, opacity and
/// transform around the rest. A control with nothing special writes
/// `impl ElementImpl for Spacer {}`.
pub trait ElementImpl {
  /// The content's size within `available`, which already excludes the margin and is held within
  /// the element's limits. By default, nothing.
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    let _ = (cx, available);
    Size::ZERO
  }

  /// Places the children within `size`, in the element's own coordinates. By default, nothing.
  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) {
    let _ = (cx, size);
  }

  /// Records the control's own drawing. By default, nothing.
  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
    let _ = (cx, scene);
  }

  /// Handles one input event. Not called while the element is disabled. By default, nothing is
  /// handled.
  fn on_event(&mut self, cx: &mut EventCx<'_>, event: &InputEvent) -> Handled {
    let _ = (cx, event);
    Handled::No
  }

  /// Adds the control's role, name, value and actions to the default node built from the property
  /// groups. By default, nothing.
  fn accessibility(&self, cx: &AccessCx<'_>, node: &mut accesskit::Node) {
    let _ = (cx, node);
  }

  /// Clips the element's descendants, in its own coordinates, for its arranged `size`; the
  /// element's own drawing isn't clipped. By default, no clip.
  fn clip(&self, size: Size) -> Option<Shape> {
    let _ = size;
    None
  }
}

/// Gives a control's state struct of type `S` to the traits built on it.
///
/// `#[element]` implements it for each field marked `#[element(state)]`, and a blanket
/// impl gives the matching trait, such as `HasContent` for `HasState<ContentState>`. A struct has
/// at most one field per state type.
pub trait HasState<S> {
  /// The state.
  fn state(&self) -> &S;

  /// The state, to change it.
  fn state_mut(&mut self) -> &mut S;
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_core::property::PropertyVisitor;
  use artic_core::tree::UiTree;
  use pretty_assertions::assert_eq;

  /// A control with nothing special, to check `ElementImpl`'s defaults.
  struct Spacer;
  impl ElementImpl for Spacer {}

  /// A leaf element written by hand, using every default `Element` has.
  #[derive(Debug)]
  struct Leaf;

  impl Object for Leaf {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  impl Layoutable for Leaf {
    fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
      Size::new(10.0, 10.0)
    }

    fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: artic_layout::geometry::Rect) {}
  }

  impl Element for Leaf {
    fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

    fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
      accesskit::Node::new(accesskit::Role::GenericContainer)
    }
  }

  // Element's defaults: no input handled, no layers.
  #[test]
  fn element_defaults_handle_nothing_and_add_no_layers() {
    let mut leaf = Leaf;
    let node = UiTree::new().alloc(Box::new(Leaf));

    assert_eq!(leaf.on_event(&mut EventCx::new(node), &InputEvent::default()), Handled::No);
    assert_eq!(leaf.layers(Size::new(10.0, 10.0)), Layers::default());
  }

  // ElementImpl's defaults: no input handled, no clip. measure_override's default needs a
  // MeasureCx, which only a layout pass makes, so the integration test covers it.
  #[test]
  fn element_impl_defaults_do_nothing() {
    let node = UiTree::new().alloc(Box::new(Leaf));
    let mut spacer = Spacer;

    assert_eq!(spacer.on_event(&mut EventCx::new(node), &InputEvent::default()), Handled::No);
    assert_eq!(spacer.clip(Size::new(10.0, 10.0)), None);
  }
}
