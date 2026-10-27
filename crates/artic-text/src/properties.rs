//! What a [`TextStyle`] and a [`ParagraphStyle`] become on a `parley` builder.

use artic_layout::finite;
use artic_layout::text::{
  FamilyName, FontFamily, FontStyle, FontWeight, GenericFamily, ParagraphStyle, TextStyle, TextWrapping,
};
use parley::{FontFamilyName, LineHeight, OverflowWrap, RangedBuilder, StyleProperty, TextWrapMode};
use std::borrow::Cow;

pub(crate) fn push(
  builder: &mut RangedBuilder<'_, ()>,
  style: &TextStyle,
  paragraph: &ParagraphStyle,
  default_family: &FamilyName,
) {
  builder.push_default(StyleProperty::FontFamily(font_stack(&style.family, default_family)));
  builder.push_default(StyleProperty::FontSize(style.size));
  builder.push_default(StyleProperty::FontWeight(weight(style.weight)));
  builder.push_default(StyleProperty::FontStyle(font_style(style.style)));
  builder.push_default(StyleProperty::LineHeight(line_height(paragraph.line_height)));
  let (mode, overflow) = wrap(paragraph.wrapping);
  builder.push_default(StyleProperty::TextWrapMode(mode));
  builder.push_default(StyleProperty::OverflowWrap(overflow));
}

fn font_stack(family: &FontFamily, default_family: &FamilyName) -> parley::FontFamily<'static> {
  let names = family
    .names()
    .iter()
    .chain([default_family])
    .map(family_name)
    .collect::<Vec<_>>();
  parley::FontFamily::List(Cow::Owned(names))
}

fn family_name(name: &FamilyName) -> FontFamilyName<'static> {
  match name {
    FamilyName::Named(name) => FontFamilyName::Named(Cow::Owned(name.to_string())),
    FamilyName::Generic(generic) => FontFamilyName::Generic(generic_family(*generic)),
  }
}

fn generic_family(generic: GenericFamily) -> parley::GenericFamily {
  match generic {
    GenericFamily::Serif => parley::GenericFamily::Serif,
    GenericFamily::SansSerif => parley::GenericFamily::SansSerif,
    GenericFamily::Monospace => parley::GenericFamily::Monospace,
    GenericFamily::Cursive => parley::GenericFamily::Cursive,
    GenericFamily::Fantasy => parley::GenericFamily::Fantasy,
    GenericFamily::SystemUi => parley::GenericFamily::SystemUi,
    GenericFamily::Emoji => parley::GenericFamily::Emoji,
    GenericFamily::Math => parley::GenericFamily::Math,
  }
}

fn weight(weight: FontWeight) -> parley::FontWeight {
  parley::FontWeight::new(f32::from(weight.value()))
}

fn font_style(style: FontStyle) -> parley::FontStyle {
  match style {
    FontStyle::Normal => parley::FontStyle::Normal,
    FontStyle::Italic => parley::FontStyle::Italic,
    FontStyle::Oblique => parley::FontStyle::Oblique(None),
  }
}

fn wrap(wrapping: TextWrapping) -> (TextWrapMode, OverflowWrap) {
  match wrapping {
    TextWrapping::NoWrap => (TextWrapMode::NoWrap, OverflowWrap::Normal),
    TextWrapping::Wrap => (TextWrapMode::Wrap, OverflowWrap::Anywhere),
    TextWrapping::WrapWithOverflow => (TextWrapMode::Wrap, OverflowWrap::Normal),
  }
}

fn line_height(height: Option<f32>) -> LineHeight {
  match height.and_then(finite::positive) {
    Some(height) => LineHeight::Absolute(height),
    None => LineHeight::MetricsRelative(1.0),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // The style's families come first, then the engine's default family.
  #[test]
  fn font_stack_ends_with_the_default_family() {
    let family: FontFamily = "Inter, monospace".parse().unwrap();

    let stack = font_stack(&family, &FamilyName::named("Fallback"));

    assert_eq!(
      stack,
      parley::FontFamily::List(Cow::Owned(vec![
        FontFamilyName::Named(Cow::Owned("Inter".into())),
        FontFamilyName::Generic(parley::GenericFamily::Monospace),
        FontFamilyName::Named(Cow::Owned("Fallback".into())),
      ]))
    );
  }

  // Each of artic's generic families becomes parley's family of the same name.
  #[test]
  fn every_generic_family_maps() {
    let pairs = [
      (GenericFamily::Serif, parley::GenericFamily::Serif),
      (GenericFamily::SansSerif, parley::GenericFamily::SansSerif),
      (GenericFamily::Monospace, parley::GenericFamily::Monospace),
      (GenericFamily::Cursive, parley::GenericFamily::Cursive),
      (GenericFamily::Fantasy, parley::GenericFamily::Fantasy),
      (GenericFamily::SystemUi, parley::GenericFamily::SystemUi),
      (GenericFamily::Emoji, parley::GenericFamily::Emoji),
      (GenericFamily::Math, parley::GenericFamily::Math),
    ];

    for (generic, expected) in pairs {
      assert_eq!(generic_family(generic), expected);
    }
  }

  // Weight carries over as a number, and oblique is the upright face slanted by parley's default.
  #[test]
  fn weight_and_style_map() {
    assert_eq!(weight(FontWeight::SEMI_BOLD), parley::FontWeight::new(600.0));
    assert_eq!(font_style(FontStyle::Normal), parley::FontStyle::Normal);
    assert_eq!(font_style(FontStyle::Italic), parley::FontStyle::Italic);
    assert_eq!(font_style(FontStyle::Oblique), parley::FontStyle::Oblique(None));
  }

  // Each wrapping mode becomes parley's wrap mode and overflow rule.
  #[test]
  fn wrapping_maps_to_parley() {
    assert_eq!(wrap(TextWrapping::NoWrap), (TextWrapMode::NoWrap, OverflowWrap::Normal));
    assert_eq!(wrap(TextWrapping::Wrap), (TextWrapMode::Wrap, OverflowWrap::Anywhere));
    assert_eq!(
      wrap(TextWrapping::WrapWithOverflow),
      (TextWrapMode::Wrap, OverflowWrap::Normal)
    );
  }

  // A line height above 0 is an absolute distance; none, or one that isn't finite or above 0, is
  // the font's own spacing.
  #[test]
  fn line_height_is_absolute_or_the_font_metrics() {
    assert_eq!(line_height(Some(30.0)), LineHeight::Absolute(30.0));
    for height in [None, Some(0.0), Some(-1.0), Some(f32::NAN), Some(f32::INFINITY)] {
      assert_eq!(line_height(height), LineHeight::MetricsRelative(1.0));
    }
  }
}
