//! The engine's `parley` state, and the one way text is laid out with it.
//!
//! A paragraph and each of trimming's candidates go through [`Shaping::lay_out`] into the same
//! layout, one after the other: a paragraph's lines are read into a [`Draft`] before any
//! candidate is laid out.

use crate::draft::{Draft, DraftLine};
use crate::faces::Faces;
use crate::lines::{self, Kept};
use crate::properties;
use crate::trim::Clusters;
use artic_layout::text::{FamilyName, ParagraphStyle, TextLimits, TextStyle};
use parley::{FontContext, Layout, LayoutContext};

pub(crate) struct Shaping {
  fonts: FontContext,
  layouts: LayoutContext<()>,
  layout: Layout<()>,
  faces: Faces,
  default_family: FamilyName,
}

impl Shaping {
  pub(crate) fn new(fonts: FontContext, default_family: FamilyName) -> Shaping {
    Shaping {
      fonts,
      layouts: LayoutContext::new(),
      layout: Layout::new(),
      faces: Faces::default(),
      default_family,
    }
  }

  pub(crate) fn default_family(&self) -> &FamilyName {
    &self.default_family
  }

  pub(crate) fn face_count(&self) -> usize {
    self.faces.len()
  }

  pub(crate) fn lay_out(&mut self, text: &str, style: &TextStyle, paragraph: &ParagraphStyle, limits: TextLimits) -> Kept {
    let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, false);
    properties::push(&mut builder, style, paragraph, &self.default_family);
    builder.build_into(&mut self.layout, text);
    lines::break_lines(&mut self.layout, paragraph, limits)
  }

  pub(crate) fn draft(&mut self, text: &str, kept: Kept) -> Draft {
    Draft::read(text, &self.layout, kept, &mut self.faces)
  }

  pub(crate) fn first_line(&mut self, text: &str, from: usize) -> DraftLine {
    match self.layout.lines().next() {
      Some(line) => DraftLine::read(text, &line, from, &mut self.faces),
      None => DraftLine::default(),
    }
  }

  pub(crate) fn clusters(&self, from: usize) -> Clusters {
    let mut clusters = Clusters::default();
    for line in self.layout.lines() {
      for run in line.runs() {
        for cluster in run.clusters() {
          let Some(start) = cluster.text_range().start.checked_sub(from) else {
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
}
