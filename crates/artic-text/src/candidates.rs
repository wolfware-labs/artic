//! Trimming's candidates: each one shaped alone, on one line, in the paragraph's direction.
//!
//! `parley` finds a layout's direction from its text, so a candidate from a right-to-left
//! paragraph that starts with Latin letters would come out left to right, with its ellipsis on the
//! wrong side. A U+200F RIGHT-TO-LEFT MARK or U+200E LEFT-TO-RIGHT MARK in front forces the
//! paragraph's direction; its cluster is then left out of the line and of the cluster offsets.

use crate::draft::DraftLine;
use crate::shaping::Shaping;
use crate::trim::Clusters;
use artic_layout::text::{ParagraphStyle, TextLimits, TextStyle};

const MARK_LEN: usize = '\u{200E}'.len_utf8();

pub(crate) struct Candidates<'a> {
  shaping: &'a mut Shaping,
  style: &'a TextStyle,
  one_line: ParagraphStyle,
  mark: char,
}

impl<'a> Candidates<'a> {
  pub(crate) fn new(
    shaping: &'a mut Shaping,
    style: &'a TextStyle,
    line_height: Option<f32>,
    right_to_left: bool,
  ) -> Candidates<'a> {
    Candidates {
      shaping,
      style,
      one_line: ParagraphStyle {
        line_height,
        ..ParagraphStyle::default()
      },
      mark: if right_to_left { '\u{200F}' } else { '\u{200E}' },
    }
  }

  pub(crate) fn line(&mut self, text: &str) -> DraftLine {
    let marked = self.lay_out(text);
    self.shaping.first_line(&marked, MARK_LEN)
  }

  pub(crate) fn clusters(&mut self, text: &str) -> Clusters {
    self.lay_out(text);
    self.shaping.clusters(MARK_LEN)
  }

  fn lay_out(&mut self, text: &str) -> String {
    let marked = format!("{}{text}", self.mark);
    self.shaping.lay_out(&marked, self.style, &self.one_line, TextLimits::NONE);
    marked
  }
}
