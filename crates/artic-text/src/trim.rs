//! Cutting a line that doesn't fit, and putting "…" where its text was dropped.
//!
//! A line's text is its own: trimming never shows another line's. Every mode drops one run of it
//! and puts the ellipsis in its place, with the whitespace on either side of it dropped too, and
//! every mode first tries dropping nothing, with the ellipsis after the text: that fits only for a
//! line cut because the lines after it were dropped, which keeps its text and gains "…". The modes differ only in which runs they try and
//! in what order: [`cut`] tries them, keeping the most text first, and returns the first that fits.
//! Widths are only nearly monotonic, because of kerning and ligatures, so the binary search steps
//! on toward keeping less while its pick doesn't fit, and back toward keeping more when nothing
//! after it fits: the result fits whenever any candidate does, and can keep a cluster or two less
//! than the best that would. [`trim_line`] shapes
//! each candidate it tries once, and draws the one it picks.

use crate::shaped::ShapedLine;
use crate::shaper::Shaper;
use artic_layout::text::TextTrimming;
use std::collections::HashMap;
use std::ops::Range;

pub(crate) const ELLIPSIS: char = '\u{2026}';

const PREFIX_CLUSTERS: usize = 8;

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Clusters {
  pub(crate) starts: Vec<usize>,
  pub(crate) words: Vec<usize>,
}

pub(crate) struct Trimmed {
  pub(crate) line: ShapedLine,
  pub(crate) lost: bool,
}

pub(crate) fn trim_line(
  shaper: &mut Shaper<'_>,
  text: &str,
  mode: TextTrimming,
  max_width: f32,
  right_to_left: bool,
) -> Option<Trimmed> {
  if mode == TextTrimming::None {
    return None;
  }
  let clusters = shaper.clusters(text, right_to_left);
  let mut shaped = HashMap::new();
  let dropped = cut(mode, text, &clusters, &mut |dropped| {
    shaped
      .entry(dropped.clone())
      .or_insert_with(|| shaper.line(&candidate(text, dropped), right_to_left))
      .width()
      <= max_width
  })?;
  let line = shaped
    .remove(&dropped)
    .unwrap_or_else(|| shaper.line(&candidate(text, &dropped), right_to_left));
  Some(Trimmed {
    line,
    lost: loses_text(text, &dropped),
  })
}

pub(crate) fn candidate(text: &str, dropped: &Range<usize>) -> String {
  format!(
    "{}{ELLIPSIS}{}",
    text[..dropped.start].trim_end(),
    text[dropped.end..].trim_start()
  )
}

pub(crate) fn loses_text(text: &str, dropped: &Range<usize>) -> bool {
  !text[dropped.clone()].trim().is_empty()
}

pub(crate) fn cut(
  mode: TextTrimming,
  text: &str,
  clusters: &Clusters,
  fits: &mut dyn FnMut(&Range<usize>) -> bool,
) -> Option<Range<usize>> {
  let all = 0..text.len();
  let whole = text.len()..text.len();
  if mode == TextTrimming::None {
    return None;
  }
  if fits(&whole) {
    return Some(whole);
  }
  Some(match mode {
    TextTrimming::None => return None,
    TextTrimming::CharacterEllipsis => trailing(text, &clusters.starts, fits).unwrap_or(all),
    TextTrimming::WordEllipsis => trailing(text, &clusters.words, fits)
      .or_else(|| trailing(text, &clusters.starts, fits))
      .unwrap_or(all),
    TextTrimming::PrefixCharacterEllipsis => prefix(text, &clusters.starts, fits)
      .or_else(|| trailing(text, &clusters.starts, fits))
      .unwrap_or(all),
    TextTrimming::LeadingCharacterEllipsis => leading(&clusters.starts, fits).unwrap_or(all),
    TextTrimming::PathSegmentEllipsis => path(text, fits).or_else(|| leading(&clusters.starts, fits)).unwrap_or(all),
  })
}

fn trailing(text: &str, starts: &[usize], fits: &mut dyn FnMut(&Range<usize>) -> bool) -> Option<Range<usize>> {
  let len = text.len();
  let tries = starts
    .iter()
    .copied()
    .filter(|&start| start > 0)
    .chain([len])
    .rev()
    .map(|start| start..len)
    .collect::<Vec<_>>();
  first_fit(&tries, fits)
}

fn leading(starts: &[usize], fits: &mut dyn FnMut(&Range<usize>) -> bool) -> Option<Range<usize>> {
  let tries = starts.iter().map(|&start| 0..start).collect::<Vec<_>>();
  first_fit(&tries, fits)
}

fn prefix(text: &str, starts: &[usize], fits: &mut dyn FnMut(&Range<usize>) -> bool) -> Option<Range<usize>> {
  let kept = *starts.get(PREFIX_CLUSTERS)?;
  let tries = starts
    .iter()
    .copied()
    .filter(|&start| start > kept)
    .chain([text.len()])
    .map(|start| kept..start)
    .collect::<Vec<_>>();
  first_fit(&tries, fits)
}

fn path(text: &str, fits: &mut dyn FnMut(&Range<usize>) -> bool) -> Option<Range<usize>> {
  let mut segments = Vec::new();
  let mut start = 0;
  for (at, character) in text.char_indices() {
    if matches!(character, '/' | '\\') {
      segments.push(start..at);
      start = at + character.len_utf8();
    }
  }
  segments.push(start..text.len());
  if segments.len() < 3 {
    return None;
  }
  let last = segments.len() - 2;
  let middle = (segments.len() / 2).clamp(1, last);
  let (mut first, mut end) = (middle, middle);
  let mut right = true;
  loop {
    let dropped = segments[first].start..segments[end].end;
    if fits(&dropped) {
      return Some(dropped);
    }
    if first == 1 && end == last {
      return None;
    }
    if (right && end < last) || first == 1 {
      end += 1;
    } else {
      first -= 1;
    }
    right = !right;
  }
}

fn first_fit(tries: &[Range<usize>], fits: &mut dyn FnMut(&Range<usize>) -> bool) -> Option<Range<usize>> {
  let pick = tries.partition_point(|dropped| !fits(dropped));
  (pick..tries.len())
    .find(|&index| fits(&tries[index]))
    .or_else(|| (0..pick).rev().find(|&index| fits(&tries[index])))
    .map(|index| tries[index].clone())
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  fn clusters(text: &str) -> Clusters {
    let starts = text.char_indices().map(|(at, _)| at).collect::<Vec<_>>();
    let words = text
      .char_indices()
      .filter(|&(at, _)| at > 0 && text[..at].ends_with(' '))
      .map(|(at, _)| at)
      .collect();
    Clusters { starts, words }
  }

  fn shown(mode: TextTrimming, text: &str, room: usize) -> String {
    let dropped = cut(mode, text, &clusters(text), &mut |dropped| {
      candidate(text, dropped).chars().count() <= room
    })
    .unwrap();
    candidate(text, &dropped)
  }

  // The ellipsis takes the dropped range's place, and the whitespace beside it goes too.
  #[test]
  fn candidate_drops_whitespace_beside_the_ellipsis() {
    assert_eq!(candidate("The quick brown", &(10..15)), "The quick…");
    assert_eq!(candidate("one two three", &(0..4)), "…two three");
    assert_eq!(candidate("abc", &(3..3)), "abc…");
    assert_eq!(candidate("abc", &(0..3)), "…");
  }

  // A line loses text when the dropped range holds something other than whitespace.
  #[test]
  fn losing_text_ignores_whitespace() {
    assert!(loses_text("abc def", &(4..7)));
    assert!(!loses_text("abc def", &(3..4)));
    assert!(!loses_text("abc", &(3..3)));
  }

  // No trimming cuts nothing.
  #[test]
  fn no_trimming_cuts_nothing() {
    assert_eq!(cut(TextTrimming::None, "abc", &clusters("abc"), &mut |_| true), None);
  }

  // Every mode first tries the whole text with "…" after it, so a line that fits, cut only for the
  // lines dropped after it, keeps all its text.
  #[test]
  fn every_mode_first_keeps_the_whole_text() {
    for mode in [
      TextTrimming::CharacterEllipsis,
      TextTrimming::WordEllipsis,
      TextTrimming::PrefixCharacterEllipsis,
      TextTrimming::LeadingCharacterEllipsis,
      TextTrimming::PathSegmentEllipsis,
    ] {
      assert_eq!(shown(mode, "abcdefghijk", 100), "abcdefghijk…", "{mode:?}");
      assert_eq!(shown(mode, "a/b/c", 100), "a/b/c…", "{mode:?}");
    }
  }

  // CharacterEllipsis keeps the longest start that fits, and "…" alone when nothing does.
  #[test]
  fn character_cut_keeps_the_longest_start() {
    assert_eq!(shown(TextTrimming::CharacterEllipsis, "abcdefghij", 5), "abcd…");
    assert_eq!(shown(TextTrimming::CharacterEllipsis, "abcdefghij", 0), "…");
  }

  // A line that fits whole, cut only for the lines dropped after it, keeps all its text.
  #[test]
  fn character_cut_keeps_text_that_fits() {
    assert_eq!(shown(TextTrimming::CharacterEllipsis, "abc", 10), "abc…");
  }

  // WordEllipsis cuts before a word, and falls back to characters when no word fits.
  #[test]
  fn word_cut_falls_at_word_starts() {
    assert_eq!(shown(TextTrimming::WordEllipsis, "one two three", 10), "one two…");
    assert_eq!(shown(TextTrimming::WordEllipsis, "abcdefghij", 5), "abcd…");
  }

  // LeadingCharacterEllipsis keeps the longest end that fits.
  #[test]
  fn leading_cut_keeps_the_longest_end() {
    assert_eq!(shown(TextTrimming::LeadingCharacterEllipsis, "abcdefghij", 5), "…ghij");
    assert_eq!(shown(TextTrimming::LeadingCharacterEllipsis, "abcdefghij", 0), "…");
  }

  // PrefixCharacterEllipsis keeps 8 clusters and the longest end that fits after them, and falls
  // back to characters when 8 clusters and "…" don't fit or the text has no more than 8.
  #[test]
  fn prefix_cut_keeps_eight_clusters_and_the_end() {
    assert_eq!(
      shown(TextTrimming::PrefixCharacterEllipsis, "abcdefghijklmnopqrst", 12),
      "abcdefgh…rst"
    );
    assert_eq!(
      shown(TextTrimming::PrefixCharacterEllipsis, "abcdefghijklmnopqrst", 6),
      "abcde…"
    );
    assert_eq!(shown(TextTrimming::PrefixCharacterEllipsis, "abcdefgh", 4), "abc…");
  }

  // PathSegmentEllipsis drops whole middle segments outward from the middle, keeping the first and
  // last, and falls back to the leading cut when that isn't enough.
  #[test]
  fn path_cut_drops_middle_segments_outward() {
    let path = r"C:\Users\ada\projects\artic\docs";

    assert_eq!(
      shown(TextTrimming::PathSegmentEllipsis, path, 28),
      r"C:\Users\ada\…\artic\docs"
    );
    assert_eq!(shown(TextTrimming::PathSegmentEllipsis, path, 22), r"C:\Users\ada\…\docs");
    assert_eq!(shown(TextTrimming::PathSegmentEllipsis, path, 10), r"C:\…\docs");
    assert_eq!(shown(TextTrimming::PathSegmentEllipsis, path, 6), r"…\docs");
  }

  // Fewer than three segments leaves nothing in the middle to drop: the leading cut takes over.
  #[test]
  fn short_path_falls_back_to_leading() {
    assert_eq!(shown(TextTrimming::PathSegmentEllipsis, "abc/defghij", 5), "…ghij");
  }

  // A fitting candidate that the binary search passes over is still found, rather than falling
  // back to "…" alone.
  #[test]
  fn search_finds_a_fit_the_binary_search_passes_over() {
    let text = "abcdefghij";
    let dropped = cut(TextTrimming::CharacterEllipsis, text, &clusters(text), &mut |dropped| {
      dropped.start == 9
    });

    assert_eq!(dropped, Some(9..10));
  }

  // When widths aren't monotonic, the binary search's pick can fail to fit; the cut steps on to
  // keep less until one fits.
  #[test]
  fn search_steps_past_a_pick_that_does_not_fit() {
    let text = "abcdefghij";
    let mut asked = Vec::new();
    let dropped = cut(TextTrimming::CharacterEllipsis, text, &clusters(text), &mut |dropped| {
      asked.push(dropped.start);
      dropped.start <= 3 || dropped.start == 6
    })
    .unwrap();

    assert!(dropped.start <= 6, "{dropped:?}");
    assert!(dropped.start == 6 || dropped.start <= 3);
    assert!(!asked.is_empty());
  }
}
