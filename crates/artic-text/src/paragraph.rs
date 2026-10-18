//! A broken `parley` layout turned into a [`Paragraph`].
//!
//! The lines are placed here, not with `parley`'s `align`, which aligns against the break width
//! rather than the widest kept line and measures trailing whitespace wrongly in right-to-left
//! runs. Lines that don't fit are trimmed first, when the style asks, so the paragraph's size
//! and the lines' placement describe what is drawn. Each line moves by its free width, the paragraph's width minus its own, times its
//! alignment's factor; a justified line is first stretched to the width limit, sharing what it
//! lacks of it among its spaces. A line is the visual span of its clusters, except the whitespace
//! that ends its text: that whitespace hangs, taking no width and drawing nothing. In a left-to-right paragraph every line's
//! span starts at 0, and in a right-to-left one every line's span ends at the paragraph's width,
//! whatever direction the line's own text runs: the paragraph's direction is its first strong
//! character's.

use crate::lines::Kept;
use crate::shaped::{ShapedLine, shape_line};
use crate::shaper::Shaper;
use crate::trim;
use artic_layout::geometry::Size;
use artic_layout::text::{Line, Paragraph, ParagraphStyle, TextAlignment, TextTrimming};
use parley::Layout;

pub(crate) fn paragraph(
  text: &str,
  layout: &Layout<()>,
  kept: Kept,
  style: &ParagraphStyle,
  max_width: Option<f32>,
  shaper: &mut Shaper<'_>,
) -> Paragraph {
  let mut shaped = layout
    .lines()
    .take(kept.lines)
    .map(|line| shape_line(text, &line, 0, shaper.faces))
    .collect::<Vec<_>>();
  let height = shaped.iter().map(|line| line.height).sum::<f32>();
  if height <= 0.0 {
    return Paragraph::default();
  }
  let right_to_left = layout.is_rtl();
  let lost = trim(text, &mut shaped, style.trimming, max_width, right_to_left, shaper);
  let last = shaped.len() - 1;
  let stretched = shaped
    .iter()
    .enumerate()
    .map(|(index, line)| {
      style.alignment == TextAlignment::Justify
        && index != last
        && !line.trimmed
        && line.soft_break
        && line.gaps > 0
        && max_width.is_some_and(|max| line.width() <= max)
    })
    .collect::<Vec<_>>();
  let widest = shaped.iter().map(ShapedLine::width).fold(0.0, f32::max);
  let stretch_to = max_width.filter(|_| stretched.contains(&true));
  let width = stretch_to.map_or(widest, |max| max.max(widest));
  let limited_width = lost || stretched.contains(&true) || shaped.iter().any(|line| line.soft_break);
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

fn trim(
  text: &str,
  lines: &mut [ShapedLine],
  mode: TextTrimming,
  max_width: Option<f32>,
  right_to_left: bool,
  shaper: &mut Shaper<'_>,
) -> bool {
  if mode == TextTrimming::None {
    return false;
  }
  let last = lines.len() - 1;
  let mut lost = false;
  for (index, line) in lines.iter_mut().enumerate() {
    let clamped = index == last && !text[line.text.end..].trim().is_empty();
    let wide = max_width.is_some_and(|max| line.width() > max);
    if !(clamped || wide) {
      continue;
    }
    let end = if index == last {
      hard_break_after(text, line.text.start)
    } else {
      line.hung
    };
    let own = line.hung - line.text.start;
    let limit = max_width.unwrap_or(f32::INFINITY);
    if let Some(trimmed) = trim::trim_line(shaper, &text[line.text.start..end], own, mode, limit, right_to_left) {
      lost |= trimmed.lost;
      *line = std::mem::take(line).replaced_by(trimmed.line);
    }
  }
  lost
}

fn hard_break_after(text: &str, from: usize) -> usize {
  text[from..]
    .find(['\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}'])
    .map_or(text.len(), |at| from + at)
}
