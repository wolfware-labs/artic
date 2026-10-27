//! Breaking a layout into lines as a [`ParagraphStyle`] says, and keeping the ones [`TextLimits`]'
//! height allows.

use artic_layout::text::{ParagraphStyle, TextLimits};
use parley::{Layout, YieldData};

pub(crate) struct Kept {
  pub(crate) lines: usize,
  pub(crate) limited_height: bool,
}

pub(crate) fn break_lines(layout: &mut Layout<()>, paragraph: &ParagraphStyle, limits: TextLimits) -> Kept {
  let max_width = limits.usable_width().unwrap_or(f32::MAX);
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
  keep_by_height(layout, limits.usable_height())
}

fn keep_by_height(layout: &Layout<()>, max_height: Option<f32>) -> Kept {
  let broken = layout.len();
  let Some(max_height) = max_height else {
    return Kept {
      lines: broken,
      limited_height: false,
    };
  };
  let mut bottom = 0.0;
  let mut lines = 0;
  for line in layout.lines() {
    bottom += line.metrics().line_height;
    if lines > 0 && bottom > max_height {
      break;
    }
    lines += 1;
  }
  Kept {
    lines,
    limited_height: lines < broken,
  }
}
