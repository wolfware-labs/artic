//! Trimming's candidates: each one shaped alone, on one line, in the paragraph's direction.
//!
//! `parley` finds a layout's direction from its text, so a candidate from a right-to-left
//! paragraph that starts with Latin letters would come out left to right, with its ellipsis on the
//! wrong side. A U+200F RIGHT-TO-LEFT MARK or U+200E LEFT-TO-RIGHT MARK in front forces the
//! paragraph's direction; its cluster, in the style of the candidate's first span (or the line's
//! base style when it has no text), is then left out
//! of the line and of the cluster offsets.

use crate::draft::DraftLine;
use crate::shaping::Shaping;
use crate::trim::Clusters;
use artic_layout::text::{ParagraphStyle, StyledText, TextLimits};

const MARK_LEN: usize = '\u{200E}'.len_utf8();

pub(crate) struct Candidates<'a> {
  shaping: &'a mut Shaping,
  one_line: ParagraphStyle,
  mark: &'static str,
}

impl<'a> Candidates<'a> {
  pub(crate) fn new(shaping: &'a mut Shaping, line_height: Option<f32>, right_to_left: bool) -> Candidates<'a> {
    Candidates {
      shaping,
      one_line: ParagraphStyle {
        line_height,
        ..ParagraphStyle::default()
      },
      mark: if right_to_left { "\u{200F}" } else { "\u{200E}" },
    }
  }

  pub(crate) fn line(&mut self, text: &StyledText, base: usize) -> DraftLine {
    let marked = self.lay_out(text, base);
    self.shaping.first_line(marked.text(), MARK_LEN)
  }

  pub(crate) fn clusters(&mut self, text: &StyledText, base: usize) -> Clusters {
    self.lay_out(text, base);
    self.shaping.clusters(MARK_LEN)
  }

  fn lay_out(&mut self, text: &StyledText, base: usize) -> StyledText {
    let mut marked = text.clone();
    let style = text.spans().next().map_or(base, |(_, style)| style);
    marked.replace_range(0..0, self.mark, style);
    self.shaping.lay_out(&marked, &self.one_line, TextLimits::NONE);
    marked
  }
}
