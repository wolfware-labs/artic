//! Breaking a layout into the lines a [`ParagraphStyle`] and [`TextLimits`] allow.

use artic_layout::text::{ParagraphStyle, TextLimits};
use parley::{Layout, YieldData};

pub(crate) fn usable_width(limits: TextLimits) -> Option<f32> {
  limits.max_width.filter(|width| width.is_finite()).map(|width| width.max(0.0))
}

pub(crate) fn break_lines(layout: &mut Layout<()>, paragraph: &ParagraphStyle, limits: TextLimits) {
  let max_width = usable_width(limits).unwrap_or(f32::MAX);
  let mut breaker = layout.break_lines();
  breaker.state_mut().set_layout_max_advance(max_width);
  breaker.state_mut().set_line_max_advance(max_width);
  let mut kept = 0;
  while paragraph.max_lines.is_none_or(|max| kept < max.get()) {
    match breaker.break_next() {
      Some(YieldData::LineBreak(_)) => kept += 1,
      Some(_) => {}
      None => break,
    }
  }
  breaker.finish();
}
