//! [`Engine`], the [`TextEngine`] artic shapes text with.

use crate::candidates::Candidates;
use crate::paragraph;
use crate::shaping::Shaping;
use artic_layout::finite;
use artic_layout::text::{FamilyName, FontBytes, GenericFamily, Paragraph, ParagraphStyle, StyledText, TextEngine, TextLimits};
use fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::FontContext;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

/// Shapes text with `parley` in fonts `fontique` finds.
///
/// It keeps its fonts, `parley`'s shaping caches, and one [`FontData`](artic_layout::text::FontData)
/// per face it has drawn from, so a face keeps one id for the engine's life. An app keeps one
/// engine and lends it to layout.
pub struct Engine {
  shaping: Shaping,
}

impl Engine {
  /// An engine that finds the system's fonts. Its default family is `system-ui`, which `fontique`
  /// maps to the desktop's interface font.
  #[must_use]
  pub fn system() -> Engine {
    let fonts = FontContext {
      collection: Collection::new(CollectionOptions::default()),
      source_cache: SourceCache::new_shared(),
    };
    Engine::new(fonts, FamilyName::Generic(GenericFamily::SystemUi))
  }

  /// An engine that knows no font, so every paragraph has no lines: for layout with no text in it.
  #[must_use]
  pub fn without_fonts() -> Engine {
    Engine::with_fonts(Vec::<FontBytes>::new())
  }

  /// An engine that knows only `fonts`, with every generic family pointing at them, in order.
  ///
  /// Its default family is the first font's, so a family it doesn't know lands on that font too.
  /// Bytes that aren't a font are skipped; with no font at all, every paragraph has no lines.
  #[must_use]
  pub fn with_fonts(fonts: impl IntoIterator<Item = impl Into<FontBytes>>) -> Engine {
    let mut collection = Collection::new(CollectionOptions {
      shared: false,
      system_fonts: false,
    });
    let mut families = Vec::new();
    for bytes in fonts {
      let bytes: FontBytes = bytes.into();
      for (family, _) in collection.register_fonts(Blob::new(Arc::clone(bytes.shared())), None) {
        if !families.contains(&family) {
          families.push(family);
        }
      }
    }
    for &generic in fontique::GenericFamily::all() {
      collection.set_generic_families(generic, families.iter().copied());
    }
    let default_family = families
      .first()
      .and_then(|&family| collection.family_name(family))
      .map_or(FamilyName::Generic(GenericFamily::SystemUi), FamilyName::named);
    let fonts = FontContext {
      collection,
      source_cache: SourceCache::default(),
    };
    Engine::new(fonts, default_family)
  }

  fn new(fonts: FontContext, default_family: FamilyName) -> Engine {
    Engine {
      shaping: Shaping::new(fonts, default_family),
    }
  }
}

impl TextEngine for Engine {
  fn paragraph(&mut self, text: &StyledText, paragraph_style: &ParagraphStyle, limits: TextLimits) -> Paragraph {
    let styles = text.styles();
    let sized = text.base_style().is_some_and(|base| {
      std::iter::once(base)
        .chain(text.spans().map(|(_, index)| index))
        .all(|index| finite::positive(styles[index].size).is_some())
    });
    if !sized || !self.shaping.has_fonts() {
      return Paragraph::default();
    }
    let kept = self.shaping.lay_out(text, paragraph_style, limits);
    let draft = self.shaping.draft(text.text(), kept);
    let mut candidates = Candidates::new(&mut self.shaping, paragraph_style.line_height, draft.right_to_left);
    paragraph::paragraph(text, draft, paragraph_style, limits.usable_width(), &mut candidates)
  }
}

impl Debug for Engine {
  /// Shows the default family and how many faces the engine has drawn from; `parley`'s contexts
  /// have nothing worth printing.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("Engine")
      .field("default_family", self.shaping.default_family())
      .field("faces", &self.shaping.face_count())
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_layout::text::{FontFamily, TextStyle};
  use pretty_assertions::assert_eq;

  fn engine() -> Engine {
    Engine::with_fonts([artic_test_fonts::REGULAR])
  }

  // A candidate keeps every cluster but the direction mark in front of it, and its cluster starts
  // are offsets in the candidate's own text.
  #[test]
  fn candidates_drop_the_direction_mark() {
    let mut engine = engine();
    let text = |text: &str| StyledText::plain(text, TextStyle::new(FontFamily::default(), 16.0));

    for right_to_left in [false, true] {
      let mut candidates = Candidates::new(&mut engine.shaping, None, right_to_left);
      let line = candidates.line(&text("abc"), 0);
      let glyphs = line.runs.iter().map(|run| run.glyph_count()).sum::<usize>();

      assert_eq!(glyphs, 3, "{right_to_left}");
      assert_eq!(candidates.clusters(&text("ab c"), 0).starts, [0, 1, 2, 3]);
      let words = candidates.clusters(&text("ab c"), 0).words;
      assert!(words.contains(&3) && !words.contains(&1), "{words:?}");
    }
  }
}
