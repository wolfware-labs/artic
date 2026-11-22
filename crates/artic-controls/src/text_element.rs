use artic_core::property::Property;
use artic_layout::text::{FontFamily, FontStyle, FontWeight, TextStyle};
use artic_macros::{UiProperties, UiType};
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

/// The font and brush a type that shows text sets on itself. Each property shares its key with
/// [`TextElement`]'s attached property of the same name, so it inherits from any ancestor that
/// sets either, and passes its value down the same way.
#[derive(Debug, UiProperties)]
pub struct TextFormat {
  /// The families to find the font in, in order: [`TextElement::FONT_FAMILY`].
  #[ui(key = TextElement::FONT_FAMILY)]
  pub font_family: Property<FontFamily>,
  /// The font size, in logical pixels: [`TextElement::FONT_SIZE`].
  #[ui(key = TextElement::FONT_SIZE)]
  pub font_size: Property<f32>,
  /// The font weight: [`TextElement::FONT_WEIGHT`].
  #[ui(key = TextElement::FONT_WEIGHT)]
  pub font_weight: Property<FontWeight>,
  /// Upright or slanted: [`TextElement::FONT_STYLE`].
  #[ui(key = TextElement::FONT_STYLE)]
  pub font_style: Property<FontStyle>,
  /// The brush the text is painted with: [`TextElement::FOREGROUND`].
  #[ui(key = TextElement::FOREGROUND)]
  pub foreground: Property<Brush>,
}

impl TextFormat {
  /// The style text is shaped in: the font properties' values.
  #[must_use]
  pub fn style(&self) -> TextStyle {
    TextStyle {
      family: self.font_family.get(),
      size: self.font_size.get(),
      weight: self.font_weight.get(),
      style: self.font_style.get(),
    }
  }
}
