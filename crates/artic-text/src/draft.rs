//! A draft of the paragraph: the kept `parley` lines turned into glyph runs, before they're
//! trimmed, sized and placed. Reading a broken layout into a draft is the one place a layout's
//! lines are read: what comes after works on the draft, so the layout is free to shape trimming's
//! candidates.
//!
//! A line keeps the clusters that start from `from` up to the whitespace that ends its text,
//! which hangs. Each glyph remembers how many kept spaces come before it in visual order, so
//! justification can move it later without shaping again.

use crate::faces::Faces;
use crate::lines::Kept;
use artic_layout::text::{FontData, Glyph, GlyphRun, Synthesis};
use parley::{BreakReason, Layout};
use std::ops::Range;
use std::sync::Arc;

pub(crate) struct Draft {
  pub(crate) lines: Vec<DraftLine>,
  pub(crate) right_to_left: bool,
  pub(crate) limited_height: bool,
}

impl Draft {
  pub(crate) fn read(text: &str, layout: &Layout<()>, kept: Kept, faces: &mut Faces) -> Draft {
    Draft {
      lines: layout
        .lines()
        .take(kept.lines)
        .map(|line| DraftLine::read(text, &line, 0, faces))
        .collect(),
      right_to_left: layout.is_rtl(),
      limited_height: kept.limited_height,
    }
  }
}

#[derive(Default)]
pub(crate) struct DraftLine {
  pub(crate) text: Range<usize>,
  pub(crate) hung: usize,
  pub(crate) height: f32,
  pub(crate) soft_break: bool,
  pub(crate) gaps: u32,
  pub(crate) baseline: f32,
  pub(crate) start: f32,
  pub(crate) end: f32,
  pub(crate) runs: Vec<DraftRun>,
}

impl DraftLine {
  pub(crate) fn read(text: &str, line: &parley::Line<'_, ()>, from: usize, faces: &mut Faces) -> DraftLine {
    let end = line.text_range().end.min(text.len());
    let start = line.text_range().start.min(end);
    let hung = start + text[start..end].trim_end().len();
    let range = start..end;
    let metrics = line.metrics();
    let mut pen = 0.0;
    let mut extent: Option<(f32, f32)> = None;
    let mut spaces = 0;
    let mut gaps = 0;
    let mut runs = Vec::new();
    for run in line.runs() {
      let mut glyphs = Vec::new();
      for cluster in run.visual_clusters() {
        let left = pen;
        let kept = (from..hung).contains(&cluster.text_range().start);
        for glyph in cluster.glyphs() {
          if kept {
            glyphs.push((
              Glyph {
                id: glyph.id,
                x: pen + glyph.x,
                y: metrics.baseline + glyph.y,
              },
              spaces,
            ));
          }
          pen += glyph.advance;
        }
        if kept {
          extent = Some(extent.map_or((left, pen), |(start, end)| (start.min(left), end.max(pen))));
          gaps = spaces;
          if cluster.is_space_or_nbsp() {
            spaces += 1;
          }
        }
      }
      if !glyphs.is_empty() {
        let synthesis = run.synthesis();
        runs.push(DraftRun {
          font: faces.font(run.font()),
          size: run.font_size(),
          coords: run.normalized_coords().iter().map(|coord| coord.to_bits()).collect(),
          synthesis: Synthesis {
            embolden: synthesis.embolden(),
            skew: synthesis.skew().unwrap_or(0.0),
          },
          glyphs,
        });
      }
    }
    let (start, end) = extent.unwrap_or((0.0, 0.0));
    DraftLine {
      text: range,
      hung,
      height: metrics.line_height,
      soft_break: matches!(line.break_reason(), BreakReason::Regular | BreakReason::Emergency),
      gaps,
      baseline: metrics.baseline,
      start,
      end,
      runs,
    }
  }

  pub(crate) fn width(&self) -> f32 {
    self.end - self.start
  }

  pub(crate) fn replaced_by(self, mut trimmed: DraftLine) -> DraftLine {
    let lift = self.baseline - trimmed.baseline;
    for run in &mut trimmed.runs {
      for (glyph, _) in &mut run.glyphs {
        glyph.y += lift;
      }
    }
    DraftLine {
      gaps: 0,
      start: trimmed.start,
      end: trimmed.end,
      runs: trimmed.runs,
      ..self
    }
  }
}

pub(crate) struct DraftRun {
  font: FontData,
  size: f32,
  coords: Arc<[i16]>,
  synthesis: Synthesis,
  glyphs: Vec<(Glyph, u32)>,
}

impl DraftRun {
  pub(crate) fn place(self, shift: f32, gap: f32) -> GlyphRun {
    let glyphs = self
      .glyphs
      .into_iter()
      .map(|(glyph, spaces)| Glyph {
        x: glyph.x + shift + gap * spaces as f32,
        ..glyph
      })
      .collect::<Vec<_>>();
    GlyphRun::new(self.font, self.size, self.coords, self.synthesis, glyphs)
  }
}

#[cfg(test)]
impl DraftRun {
  pub(crate) fn glyph_count(&self) -> usize {
    self.glyphs.len()
  }
}
