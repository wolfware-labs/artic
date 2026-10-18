use crate::TextElement;
use artic_core::property::Property;
use artic_element::{AccessCx, ElementImpl, RenderCx};
use artic_layout::geometry::{Point, Size};
use artic_layout::text::{
  FontFamily, FontStyle, FontWeight, Paragraph, ParagraphStyle, TextAlignment, TextEngine, TextLimits, TextStyle, TextTrimming,
  TextWrapping,
};
use artic_layout::MeasureCx;
use artic_macros::element;
use artic_render::{Brush, SceneBuilder};
use std::num::NonZeroU32;

/// Shows a string: shaped, wrapped to its width, cut with "…" when it runs out of room, aligned,
/// and painted with a brush. Its font and brush come from the nearest ancestor that sets
/// [`TextElement`]'s properties when it doesn't set them itself.
#[element]
#[derive(Debug)]
#[xaml(namespace = "https://artic.rs/ui")]
pub struct TextBlock {
  /// The text.
  #[xaml(affects = layout)]
  pub text: Property<String>,
  /// Whether and how lines break to fit the width.
  #[xaml(affects = layout)]
  pub text_wrapping: Property<TextWrapping>,
  /// How a line that doesn't fit, or that comes before lines dropped for `MaxLines` or the height,
  /// is cut.
  #[xaml(affects = layout)]
  pub text_trimming: Property<TextTrimming>,
  /// Where each line sits in the paragraph, and the paragraph in the box.
  #[xaml(affects = layout)]
  pub text_alignment: Property<TextAlignment>,
  /// The number of lines kept, or `None` for every line.
  #[xaml(affects = layout)]
  pub max_lines: Property<Option<NonZeroU32>>,
  /// The distance from one line's top to the next, or `None` for the font's own spacing.
  #[xaml(affects = layout)]
  pub line_height: Property<Option<f32>>,
  /// The families to find the font in, in order: [`TextElement::FONT_FAMILY`].
  #[xaml(key = TextElement::FONT_FAMILY)]
  pub font_family: Property<FontFamily>,
  /// The font size, in logical pixels: [`TextElement::FONT_SIZE`].
  #[xaml(key = TextElement::FONT_SIZE)]
  pub font_size: Property<f32>,
  /// The font weight: [`TextElement::FONT_WEIGHT`].
  #[xaml(key = TextElement::FONT_WEIGHT)]
  pub font_weight: Property<FontWeight>,
  /// Upright or slanted: [`TextElement::FONT_STYLE`].
  #[xaml(key = TextElement::FONT_STYLE)]
  pub font_style: Property<FontStyle>,
  /// The brush the text is painted with: [`TextElement::FOREGROUND`].
  #[xaml(key = TextElement::FOREGROUND)]
  pub foreground: Property<Brush>,
}

/// A paragraph and the limits it was shaped with, kept as the layout result.
#[derive(Debug, Clone, PartialEq)]
struct Shaped {
  paragraph: Paragraph,
  limits: TextLimits,
}

impl TextBlock {
  fn shape(&self, text: &mut dyn TextEngine, limits: TextLimits) -> Shaped {
    let style = TextStyle {
      family: self.font_family.get(),
      size: self.font_size.get(),
      weight: self.font_weight.get(),
      style: self.font_style.get(),
    };
    let paragraph = ParagraphStyle {
      line_height: self.line_height.get(),
      alignment: self.text_alignment.get(),
      wrapping: self.text_wrapping.get(),
      trimming: self.text_trimming.get(),
      max_lines: self.max_lines.get(),
    };
    Shaped {
      paragraph: text.paragraph(&self.text.get(), &style, &paragraph, limits),
      limits,
    }
  }

  fn limits(&self, size: Size) -> TextLimits {
    let trimming = self.text_trimming.get() != TextTrimming::None;
    TextLimits {
      max_width: Some(size.width),
      max_height: trimming.then_some(size.height),
    }
  }
}

impl ElementImpl for TextBlock {
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    let shaped = self.shape(cx.text(), self.limits(available));
    let size = shaped.paragraph.size();
    cx.set_measured(shaped);
    size
  }

  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
    let Some(Shaped { paragraph, .. }) = cx.layout_result::<Shaped>() else {
      return;
    };
    let factor = self.text_alignment.get().factor(paragraph.right_to_left());
    let x = (cx.size().width - paragraph.size().width) * factor;
    scene.text(paragraph, Point::new(x, 0.0), self.foreground.get());
  }

  fn accessibility(&self, _cx: &AccessCx<'_>, node: &mut accesskit::Node) {
    node.set_role(accesskit::Role::Label);
    node.set_value(self.text.get());
  }
}
