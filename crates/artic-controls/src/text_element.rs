use artic_layout::text::{FontFamily, FontStyle, FontWeight};
use artic_macros::UiType;
use artic_render::{Brush, Color};

/// The font and brush text is drawn with, as attached properties any node can hold. Each
/// inherits, so text follows the nearest ancestor that sets them: a panel can set the font of
/// everything inside it.
#[derive(Debug, UiType)]
#[ui(attached(name = "FontFamily", ty = FontFamily, affects = layout, target = any, inherits))]
#[ui(attached(name = "FontSize", ty = f32, default = 14.0, affects = layout, target = any, inherits))]
#[ui(attached(name = "FontWeight", ty = FontWeight, affects = layout, target = any, inherits))]
#[ui(attached(name = "FontStyle", ty = FontStyle, affects = layout, target = any, inherits))]
#[ui(attached(
  name = "Foreground",
  ty = Brush,
  default = Brush::from(Color::rgb(0, 0, 0)),
  affects = render,
  target = any,
  inherits
))]
pub struct TextElement;
