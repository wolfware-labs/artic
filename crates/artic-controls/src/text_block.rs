use crate::TextElement;
use artic_core::property::Property;
use artic_element::{AccessCx, ElementImpl, RenderCx};
use artic_layout::geometry::{Point, Size};
use artic_layout::text::{
  FontFamily, FontStyle, FontWeight, Paragraph, ParagraphStyle, TextAlignment, TextEngine, TextLimits, TextStyle, TextTrimming,
  TextWrapping,
};
use artic_layout::{ArrangeCx, MeasureCx};
use artic_macros::element;
use artic_render::{Brush, SceneBuilder};
use std::num::NonZeroU32;

/// Shows a string: shaped, wrapped to its width, cut with "…" when it runs out of room, aligned,
/// and painted with a brush. Its font and brush come from the nearest ancestor that sets
/// [`TextElement`]'s properties when it doesn't set them itself.
#[element]
#[derive(Debug)]
pub struct TextBlock {
  /// The text.
  #[ui(affects = layout)]
  pub text: Property<String>,
  /// Whether and how lines break to fit the width.
  #[ui(affects = layout)]
  pub text_wrapping: Property<TextWrapping>,
  /// How a line that doesn't fit, or that comes before lines dropped for `MaxLines` or the height,
  /// is cut.
  #[ui(affects = layout)]
  pub text_trimming: Property<TextTrimming>,
  /// Where each line sits in the paragraph, and the paragraph in the box.
  #[ui(affects = layout)]
  pub text_alignment: Property<TextAlignment>,
  /// The number of lines kept, or `None` for every line.
  #[ui(affects = layout)]
  pub max_lines: Property<Option<NonZeroU32>>,
  /// The distance from one line's top to the next, or `None` for the font's own spacing.
  #[ui(affects = layout)]
  pub line_height: Property<Option<f32>>,
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

  fn shapes_again(&self, shaped: &Shaped, size: Size, device_pixel: f32) -> bool {
    let wrapping = self.text_wrapping.get() != TextWrapping::NoWrap;
    let trimming = self.text_trimming.get() != TextTrimming::None;
    let paragraph = &shaped.paragraph;
    let beyond = |a: f32, b: f32| a - b > device_pixel;
    let apart = |a: f32, b: f32| beyond(a, b) || beyond(b, a);
    let differs = |limit: Option<f32>, side: f32, limited: bool, extent: f32| {
      let limit = limit.unwrap_or(f32::INFINITY);
      apart(side, limit) && (limited || beyond(extent, side))
    };
    let width = (wrapping || trimming)
      && differs(
        shaped.limits.usable_width(),
        size.width,
        paragraph.limited_width(),
        paragraph.size().width,
      );
    let height = trimming
      && differs(
        shaped.limits.usable_height(),
        size.height,
        paragraph.limited_height(),
        paragraph.size().height,
      );
    width || height
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

  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) {
    let device_pixel = 1.0 / cx.scale();
    let again = cx
      .arranged::<Shaped>()
      .or_else(|| cx.measured::<Shaped>())
      .is_some_and(|current| self.shapes_again(current, size, device_pixel));
    if again {
      let shaped = self.shape(cx.text(), self.limits(size));
      cx.set_arranged(shaped);
    }
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
