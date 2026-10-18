//! [`Engine`], the [`TextEngine`] artic shapes text with.

use crate::faces::Faces;
use crate::{lines, paragraph, properties};
use artic_layout::text::{FamilyName, GenericFamily, Paragraph, TextEngine, TextLimits, TextStyle};
use fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{FontContext, Layout, LayoutContext};
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

/// Shapes text with `parley` in fonts `fontique` finds.
///
/// It keeps its fonts, `parley`'s shaping caches, and one [`FontData`](artic_layout::text::FontData)
/// per face it has drawn from, so a face keeps one id for the engine's life. An app keeps one
/// engine and lends it to layout.
pub struct Engine {
  fonts: FontContext,
  layouts: LayoutContext<()>,
  layout: Layout<()>,
  faces: Faces,
  default_family: FamilyName,
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

  /// An engine that knows only `fonts`, with every generic family pointing at them, in order.
  ///
  /// Its default family is the first font's, so a family it doesn't know lands on that font too.
  /// Bytes that aren't a font are skipped; with no font at all, every paragraph is empty.
  #[must_use]
  pub fn with_fonts(fonts: impl IntoIterator<Item = Arc<dyn AsRef<[u8]> + Send + Sync>>) -> Engine {
    let mut collection = Collection::new(CollectionOptions {
      shared: false,
      system_fonts: false,
    });
    let mut families = Vec::new();
    for bytes in fonts {
      for (family, _) in collection.register_fonts(Blob::new(bytes), None) {
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
      fonts,
      layouts: LayoutContext::new(),
      layout: Layout::new(),
      faces: Faces::default(),
      default_family,
    }
  }
}

impl TextEngine for Engine {
  fn paragraph(&mut self, text: &str, style: &TextStyle, limits: TextLimits) -> Paragraph {
    if !(style.size.is_finite() && style.size > 0.0) {
      return Paragraph::default();
    }
    let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, false);
    properties::push(&mut builder, style, &self.default_family);
    builder.build_into(&mut self.layout, text);
    lines::break_lines(&mut self.layout, limits);
    paragraph::paragraph(text, &self.layout, &mut self.faces)
  }
}

impl Debug for Engine {
  /// Shows the default family and how many faces the engine has drawn from; `parley`'s contexts
  /// have nothing worth printing.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("Engine")
      .field("default_family", &self.default_family)
      .field("faces", &self.faces.len())
      .finish()
  }
}
