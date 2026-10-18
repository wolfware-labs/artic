//! Shaping one string alone, on one line, in a forced direction: how trimming measures and draws
//! its candidates.
//!
//! `parley` finds a layout's direction from its text, so a candidate from a right-to-left
//! paragraph that starts with Latin letters would come out left to right, with its ellipsis on the
//! wrong side. A U+200F RIGHT-TO-LEFT MARK or U+200E LEFT-TO-RIGHT MARK in front forces the
//! paragraph's direction; its cluster is then left out of the line.

use crate::faces::Faces;
use crate::shaped::{ShapedLine, shape_line};
use crate::trim::Clusters;
use crate::{lines, properties};
use artic_layout::text::{FamilyName, ParagraphStyle, TextLimits, TextStyle};
use parley::{FontContext, Layout, LayoutContext};

const MARK_LEN: usize = '\u{200E}'.len_utf8();

pub(crate) struct Shaper<'a> {
  pub(crate) fonts: &'a mut FontContext,
  pub(crate) layouts: &'a mut LayoutContext<()>,
  pub(crate) layout: &'a mut Layout<()>,
  pub(crate) faces: &'a mut Faces,
  pub(crate) style: &'a TextStyle,
  pub(crate) line_height: Option<f32>,
  pub(crate) default_family: &'a FamilyName,
}

impl Shaper<'_> {
  pub(crate) fn line(&mut self, text: &str, right_to_left: bool) -> ShapedLine {
    let marked = self.lay_out(text, right_to_left);
    match self.layout.lines().next() {
      Some(line) => shape_line(&marked, &line, MARK_LEN, self.faces),
      None => ShapedLine::default(),
    }
  }

  pub(crate) fn clusters(&mut self, text: &str, right_to_left: bool) -> Clusters {
    self.lay_out(text, right_to_left);
    let mut clusters = Clusters::default();
    for line in self.layout.lines() {
      for run in line.runs() {
        for cluster in run.clusters() {
          let Some(start) = cluster.text_range().start.checked_sub(MARK_LEN) else {
            continue;
          };
          clusters.starts.push(start);
          if cluster.is_word_boundary() {
            clusters.words.push(start);
          }
        }
      }
    }
    clusters.starts.sort_unstable();
    clusters.words.sort_unstable();
    clusters
  }

  fn lay_out(&mut self, text: &str, right_to_left: bool) -> String {
    let mark = if right_to_left { '\u{200F}' } else { '\u{200E}' };
    let marked = format!("{mark}{text}");
    let one_line = ParagraphStyle {
      line_height: self.line_height,
      ..ParagraphStyle::default()
    };
    let mut builder = self.layouts.ranged_builder(self.fonts, &marked, 1.0, false);
    properties::push(&mut builder, self.style, &one_line, self.default_family);
    builder.build_into(self.layout, &marked);
    lines::break_lines(self.layout, &one_line, TextLimits::NONE);
    marked
  }
}
