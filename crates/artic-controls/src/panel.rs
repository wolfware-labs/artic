use artic_core::property::Property;
use artic_element::RenderCx;
use artic_layout::geometry::Rect;
use artic_macros::UiProperties;
use artic_render::{Brush, SceneBuilder};

/// What every panel has: a background drawn under its children. A panel flattens it and calls
/// [`render`](Panel::render) from its own `render`.
#[derive(Debug, UiProperties)]
pub struct Panel {
  /// The brush the panel's bounds are filled with, if any.
  #[ui(affects = render)]
  pub background: Property<Option<Brush>>,
}

impl Panel {
  /// Fills the element's bounds with `Background`, if it's set.
  pub fn render(&self, cx: &RenderCx<'_>, scene: &mut SceneBuilder) {
    if let Some(brush) = self.background.get() {
      let size = cx.size();
      scene.fill(Rect::new(0.0, 0.0, size.width, size.height), brush);
    }
  }
}
