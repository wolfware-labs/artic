//! A broken `parley` layout turned into a [`Paragraph`].
//!
//! The lines are placed here, not with `parley`'s `align`, which aligns against the break width
//! rather than the widest kept line and measures trailing whitespace wrongly in right-to-left
//! runs. Each line moves by its free width, the paragraph's width minus its own, times its
//! alignment's factor; a justified line is first stretched to the width limit, sharing what it
//! lacks of it among its spaces. A line is the visual span of its clusters, except the whitespace that ends its text: that
//! whitespace hangs, taking no width and drawing nothing. In a left-to-right paragraph every line's
//! span starts at 0, and in a right-to-left one every line's span ends at the paragraph's width,
//! whatever direction the line's own text runs: the paragraph's direction is its first strong
//! character's.

use crate::faces::Faces;
use crate::lines::Kept;
use artic_layout::geometry::Size;
use artic_layout::text::{FontData, Glyph, GlyphRun, Line, Paragraph, ParagraphStyle, Synthesis, TextAlignment};
use parley::{BreakReason, Layout};
use std::sync::Arc;

pub(crate) fn paragraph(
  text: &str,
  layout: &Layout<()>,
  kept: Kept,
  style: &ParagraphStyle,
  max_width: Option<f32>,
  faces: &mut Faces,
) -> Paragraph {
  let shaped = layout
    .lines()
    .take(kept.lines)
    .map(|line| shape_line(text, &line, faces))
    .collect::<Vec<_>>();
  let height = shaped.iter().map(|line| line.height).sum::<f32>();
  if height <= 0.0 {
    return Paragraph::default();
  }
  let last = shaped.len() - 1;
  let stretched = shaped
    .iter()
    .enumerate()
    .map(|(index, line)| {
      style.alignment == TextAlignment::Justify
        && index != last
        && line.soft_break
        && line.gaps > 0
        && max_width.is_some_and(|max| line.width() <= max)
    })
    .collect::<Vec<_>>();
  let widest = shaped.iter().map(ShapedLine::width).fold(0.0, f32::max);
  let stretch_to = max_width.filter(|_| stretched.contains(&true));
  let width = stretch_to.map_or(widest, |max| max.max(widest));
  let right_to_left = layout.is_rtl();
  let limited_width = stretched.contains(&true) || shaped.iter().any(|line| line.soft_break);
  let factor = style.alignment.factor(right_to_left);
  let mut top = 0.0;
  let mut lines = Vec::with_capacity(shaped.len());
  let mut runs = Vec::new();
  for (line, stretch) in shaped.into_iter().zip(stretched) {
    let (line_width, gap) = match stretch_to {
      Some(max) if stretch => (max, (max - line.width()) / line.gaps as f32),
      _ => (line.width(), 0.0),
    };
    let shift = (width - line_width) * factor - line.start;
    lines.push(Line {
      top,
      height: line.height,
      baseline: line.baseline,
      width: line_width,
    });
    top += line.height;
    runs.extend(line.runs.into_iter().map(|run| run.place(shift, gap)));
  }
  Paragraph::new(Size::new(width, height), lines, runs)
    .with_right_to_left(right_to_left)
    .with_limited(limited_width, kept.limited_height)
}

struct ShapedLine {
  height: f32,
  soft_break: bool,
  gaps: u32,
  baseline: f32,
  start: f32,
  end: f32,
  runs: Vec<ShapedRun>,
}

impl ShapedLine {
  fn width(&self) -> f32 {
    self.end - self.start
  }
}

struct ShapedRun {
  font: FontData,
  size: f32,
  coords: Arc<[i16]>,
  synthesis: Synthesis,
  glyphs: Vec<(Glyph, u32)>,
}

impl ShapedRun {
  fn place(self, shift: f32, gap: f32) -> GlyphRun {
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

fn shape_line(text: &str, line: &parley::Line<'_, ()>, faces: &mut Faces) -> ShapedLine {
  let end = line.text_range().end.min(text.len());
  let start = line.text_range().start.min(end);
  let hung = start + text[start..end].trim_end().len();
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
      let kept = cluster.text_range().start < hung;
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
      runs.push(ShapedRun {
        font: faces.font(run.font()),
        size: run.font_size(),
        coords: run.normalized_coords().into(),
        synthesis: Synthesis {
          embolden: synthesis.embolden(),
          skew: synthesis.skew().unwrap_or(0.0),
        },
        glyphs,
      });
    }
  }
  let (start, end) = extent.unwrap_or((0.0, 0.0));
  ShapedLine {
    height: metrics.line_height,
    soft_break: matches!(line.break_reason(), BreakReason::Regular | BreakReason::Emergency),
    gaps,
    baseline: metrics.baseline,
    start,
    end,
    runs,
  }
}
