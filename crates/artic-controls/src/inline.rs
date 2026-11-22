//! Inline runs: the pieces of a [`TextBlock`](crate::TextBlock)'s text, each in its own style, as
//! in `Hello <Bold>world</Bold>`.
//!
//! An inline is a node in the logical tree only: it takes no space and draws nothing, and the
//! text block above it shapes and paints its text. Its font and brush inherit down the logical
//! tree, so a [`Run`] inside [`Bold`] inside [`Italic`] is bold and italic with no merge rule, and
//! a value the run sets itself wins.

use crate::TextFormat;
use artic_core::property::{Precedence, Property};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_layout::text::{FontStyle, FontWeight, StyledText};
use artic_macros::UiType;
use artic_registry::{OverrideDefaults, TypeInfo, UiCollection, UiType, ValueError};

/// Marks the inlines a type holds, as a UI language writes them: a [`Run`], [`Bold`] or
/// [`Italic`] becomes the type's logical child, drawn by the text block above it, so a change to
/// one reaches that block. The field itself holds nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct Inlines;

impl Inlines {
  /// The runs under `owner`, in reading order: its inlines, with the runs inside each [`Bold`]
  /// or [`Italic`] in their place.
  pub fn runs(tree: &UiTree, owner: NodeId) -> impl Iterator<Item = &Run> {
    tree
      .descendants(TreeKind::Logical, owner)
      .filter_map(|node| tree.get_as::<Run>(node))
  }

  /// The text of the runs under `owner`, each piece in its run's style, or `None` when `owner`
  /// holds no inlines.
  #[must_use]
  pub fn styled_text(tree: &UiTree, owner: NodeId) -> Option<StyledText> {
    tree.children(TreeKind::Logical, owner).next()?;
    let mut text = StyledText::new();
    for run in Self::runs(tree, owner) {
      run.text.with(|piece| text.push(piece, run.format.style()));
    }
    Some(text)
  }
}

impl UiCollection for Inlines {
  fn takes() -> Option<&'static [&'static TypeInfo]> {
    static INLINES: [&TypeInfo; 3] = [Run::TYPE_INFO, Bold::TYPE_INFO, Italic::TYPE_INFO];
    Some(&INLINES)
  }

  fn text_wrapper() -> Option<&'static TypeInfo> {
    Some(Run::TYPE_INFO)
  }

  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError> {
    field(tree, owner).ok_or(ValueError::WrongNode)?;
    tree
      .link(TreeKind::Logical, owner, None, child)
      .map_err(ValueError::InvalidChild)?;
    tree.set_drawn_by_parent(child, true);
    Ok(())
  }
}

/// A piece of text in its own font and brush, which it inherits from the inline or text block
/// it's in unless it sets them.
#[derive(Debug, UiType)]
#[ui(whitespace = "significant")]
pub struct Run {
  /// The text.
  #[ui(content, affects = layout)]
  pub text: Property<String>,
  /// The font and brush.
  #[ui(flatten)]
  pub format: TextFormat,
}

/// Inlines in a bold weight, unless they or this set another.
#[derive(Debug, UiType)]
#[ui(whitespace = "significant", override_defaults)]
pub struct Bold {
  /// The inlines.
  #[ui(content)]
  pub inlines: Inlines,
  /// The font and brush; the weight is bold.
  #[ui(flatten)]
  pub format: TextFormat,
}

impl OverrideDefaults for Bold {
  fn override_defaults(&self) {
    self.format.font_weight.set_at(Precedence::TypeValue, FontWeight::BOLD);
  }
}

/// Inlines in italics, unless they or this set another style.
#[derive(Debug, UiType)]
#[ui(whitespace = "significant", override_defaults)]
pub struct Italic {
  /// The inlines.
  #[ui(content)]
  pub inlines: Inlines,
  /// The font and brush; the style is italic.
  #[ui(flatten)]
  pub format: TextFormat,
}

impl OverrideDefaults for Italic {
  fn override_defaults(&self) {
    self.format.font_style.set_at(Precedence::TypeValue, FontStyle::Italic);
  }
}
