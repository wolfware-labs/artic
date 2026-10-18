//! Breaking a layout into the lines [`TextLimits`] allow.

use artic_layout::text::TextLimits;
use parley::{Layout, YieldData};

pub(crate) fn break_lines(layout: &mut Layout<()>, limits: TextLimits) {
  let max_width = match limits.max_width {
    Some(width) if width.is_finite() => width.max(0.0),
    _ => f32::MAX,
  };
  let mut breaker = layout.break_lines();
  breaker.state_mut().set_layout_max_advance(max_width);
  breaker.state_mut().set_line_max_advance(max_width);
  let mut kept = 0;
  while limits.max_lines.is_none_or(|max| kept < max.get()) {
    match breaker.break_next() {
      Some(YieldData::LineBreak(_)) => kept += 1,
      Some(_) => {}
      None => break,
    }
  }
  breaker.finish();
}
