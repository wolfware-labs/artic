use artic_layout::text::{
  FontFamily, FontStyle, FontWeight, GenericFamily, Line, Paragraph, ParagraphStyle, TextAlignment, TextEngine, TextLimits,
  TextStyle, TextTrimming, TextWrapping,
};
use artic_test_fonts::{BOLD, REGULAR};
use artic_text::Engine;
use pretty_assertions::assert_eq;
use std::num::NonZeroU32;
use std::sync::Arc;

const LINE: f32 = 19.84;

fn engine() -> Engine {
  Engine::with_fonts([REGULAR, BOLD])
}

fn style() -> TextStyle {
  TextStyle::new("Atkinson Hyperlegible".parse().unwrap(), 16.0)
}

fn shape(engine: &mut Engine, text: &str, style: &TextStyle) -> Paragraph {
  engine.paragraph(text, style, &ParagraphStyle::default(), TextLimits::NONE)
}

fn laid_out(engine: &mut Engine, text: &str, paragraph: &ParagraphStyle, limits: TextLimits) -> Paragraph {
  engine.paragraph(text, &style(), paragraph, limits)
}

fn wrapping(wrapping: TextWrapping) -> ParagraphStyle {
  ParagraphStyle {
    wrapping,
    ..ParagraphStyle::default()
  }
}

fn assert_close(actual: f32, expected: f32) {
  assert!((actual - expected).abs() <= 0.5, "{actual} isn't within 0.5 of {expected}");
}

fn face(paragraph: &Paragraph) -> &[u8] {
  (**paragraph.runs()[0].font().bytes()).as_ref()
}

// The test font sets "Hello, world" at 16 px in one line of its own size.
#[test]
fn paragraph_size_with_test_font() {
  let paragraph = shape(&mut engine(), "Hello, world", &style());

  assert_close(paragraph.size().width, 81.49);
  assert_close(paragraph.size().height, LINE);
  assert_eq!(paragraph.lines().len(), 1);
}

// Empty text has no width and no glyphs, and the height of one line, as an empty WPF TextBlock.
#[test]
fn empty_text_has_one_line_height() {
  let paragraph = shape(&mut engine(), "", &style());

  assert_eq!(paragraph.size().width, 0.0);
  assert_close(paragraph.size().height, LINE);
  assert_eq!(paragraph.lines().len(), 1);
  assert!(paragraph.is_empty());
}

// A size that isn't finite or isn't above 0 gives an empty paragraph of size 0.
#[test]
fn bad_sizes_give_an_empty_paragraph() {
  for size in [0.0, -1.0, f32::NAN, f32::INFINITY] {
    let style = TextStyle { size, ..style() };

    assert_eq!(shape(&mut engine(), "Hello", &style), Paragraph::default());
  }
}

// An engine without fonts gives an empty paragraph of size 0.
#[test]
fn no_font_gives_an_empty_paragraph() {
  let paragraph = shape(&mut Engine::without_fonts(), "Hello", &style());

  assert_eq!(paragraph, Paragraph::default());
}

// Bytes that aren't a font are skipped, and the first font that loads is the default family.
#[test]
fn unreadable_font_bytes_are_skipped() {
  let mut engine = Engine::with_fonts([&b"not a font"[..], REGULAR]);
  let nope = TextStyle::new("Nope".parse().unwrap(), 16.0);

  assert!(std::ptr::eq(face(&shape(&mut engine, "Hello", &nope)), REGULAR));
}

// A family that isn't installed gives way to the next one in the list.
#[test]
fn missing_family_falls_back() {
  let mut engine = engine();
  let listed = TextStyle::new("Nope, 'Atkinson Hyperlegible'".parse().unwrap(), 16.0);

  assert_eq!(shape(&mut engine, "Hello", &listed), shape(&mut engine, "Hello", &style()));
}

// Font bytes come as the caller holds them: borrowed, owned or already shared.
#[test]
fn with_fonts_takes_any_bytes() {
  let owned = Engine::with_fonts([REGULAR.to_vec()]);
  let shared = Engine::with_fonts([Arc::<[u8]>::from(REGULAR)]);

  for mut engine in [owned, shared] {
    assert_eq!(shape(&mut engine, "a", &style()).runs()[0].glyphs().len(), 1);
  }
}

// With only its own fonts, an unknown name and every generic family land on the first font.
#[test]
fn with_fonts_falls_back_to_its_first_font() {
  let mut engine = engine();

  for family in ["Nope", "serif", "monospace", "system-ui"] {
    let style = TextStyle::new(family.parse().unwrap(), 16.0);

    assert!(std::ptr::eq(face(&shape(&mut engine, "Hello", &style)), REGULAR), "{family}");
  }
}

// Bold selects the family's bold face, with nothing synthesized.
#[test]
fn bold_selects_a_heavier_face() {
  let style = TextStyle {
    weight: FontWeight::BOLD,
    ..style()
  };
  let paragraph = shape(&mut engine(), "Hello", &style);

  assert!(std::ptr::eq(face(&paragraph), BOLD));
  assert!(!paragraph.runs()[0].synthesis().embolden);
}

// Bold asked of a family with only a regular face thickens the regular one.
#[test]
fn missing_bold_is_synthesized() {
  let style = TextStyle {
    weight: FontWeight::BOLD,
    ..style()
  };
  let paragraph = shape(&mut Engine::with_fonts([REGULAR]), "Hello", &style);

  assert!(std::ptr::eq(face(&paragraph), REGULAR));
  assert!(paragraph.runs()[0].synthesis().embolden);
}

// Italic and oblique, asked of a family without an italic face, slant the upright one.
#[test]
fn missing_italic_is_synthesized() {
  for slant in [FontStyle::Italic, FontStyle::Oblique] {
    let style = TextStyle { style: slant, ..style() };
    let paragraph = shape(&mut engine(), "Hello", &style);

    assert!(paragraph.runs()[0].synthesis().skew > 0.0, "{slant:?}");
  }
}

// One face keeps one id across paragraphs, and another face has another id.
#[test]
fn same_font_keeps_its_id() {
  let mut engine = engine();
  let bold = TextStyle {
    weight: FontWeight::BOLD,
    ..style()
  };

  let first = shape(&mut engine, "Hello", &style());
  let second = shape(&mut engine, "world", &style());
  let heavy = shape(&mut engine, "Hello", &bold);

  assert_eq!(first.runs()[0].font().id(), second.runs()[0].font().id());
  assert_ne!(first.runs()[0].font().id(), heavy.runs()[0].font().id());
}

// A line height puts each line that far below the previous one.
#[test]
fn line_height_sets_line_spacing() {
  let style = ParagraphStyle {
    line_height: Some(30.0),
    ..ParagraphStyle::default()
  };
  let paragraph = laid_out(&mut engine(), "a\nb", &style, TextLimits::NONE);

  assert_eq!(paragraph.lines().len(), 2);
  assert_close(paragraph.lines()[1].top, 30.0);
  assert_close(paragraph.size().height, 60.0);
}

// A line height that isn't finite or isn't above 0 uses the font's own spacing.
#[test]
fn unusable_line_height_uses_the_font_spacing() {
  let mut engine = engine();
  let expected = shape(&mut engine, "a\nb", &style());

  for line_height in [0.0, -1.0, f32::NAN, f32::INFINITY] {
    let style = ParagraphStyle {
      line_height: Some(line_height),
      ..ParagraphStyle::default()
    };

    assert_eq!(
      laid_out(&mut engine, "a\nb", &style, TextLimits::NONE),
      expected,
      "{line_height}"
    );
  }
}

// Text ending in a newline keeps the empty line after it, and its height counts.
#[test]
fn trailing_newline_keeps_its_empty_line() {
  let paragraph = shape(&mut engine(), "a\n", &style());

  assert_eq!(paragraph.lines().len(), 2);
  assert_close(paragraph.size().height, 2.0 * LINE);
}

// The system engine maps system-ui to an installed font. A machine with no fonts skips the test.
#[test]
fn system_engine_resolves_system_ui() {
  let paragraph = shape(&mut Engine::system(), "Hello", &TextStyle::new(FontFamily::default(), 16.0));

  if paragraph.is_empty() {
    eprintln!("skipped: no system fonts installed");
    return;
  }
  assert!(paragraph.size().width > 0.0);
  assert!(paragraph.size().height > 0.0);
}

// A system font that fontique's file cache dropped, after 200 paragraphs in another family,
// comes back with its first id. A machine with no fonts skips the test.
#[test]
fn system_font_keeps_its_id() {
  let mut engine = Engine::system();
  let ui = TextStyle::new(FontFamily::default(), 16.0);
  let mono = TextStyle::new(FontFamily::from(GenericFamily::Monospace), 16.0);

  let first = shape(&mut engine, "Hello", &ui);
  if first.is_empty() {
    eprintln!("skipped: no system fonts installed");
    return;
  }
  for _ in 0..200 {
    shape(&mut engine, "Hello", &mono);
  }
  let again = shape(&mut engine, "Hello", &ui);

  assert_eq!(again.runs()[0].font().id(), first.runs()[0].font().id());
}

// Debug shows the default family and how many faces the engine has drawn from.
#[test]
fn engine_debug_shows_default_family_and_faces() {
  let mut engine = Engine::with_fonts([REGULAR]);
  shape(&mut engine, "Hello", &style());

  assert_eq!(
    format!("{engine:?}"),
    r#"Engine { default_family: Named("Atkinson Hyperlegible"), faces: 1 }"#
  );
}

const FOX: &str = "The quick brown fox jumps over the lazy dog";
const HEBREW: &str = "שלום עולם שלום עולם שלום";

fn wrapped(engine: &mut Engine, text: &str, max_width: f32) -> Paragraph {
  laid_out(
    engine,
    text,
    &wrapping(TextWrapping::WrapWithOverflow),
    TextLimits::width(max_width),
  )
}

fn glyph_xs(paragraph: &Paragraph, line: &Line) -> Vec<f32> {
  paragraph
    .runs()
    .iter()
    .flat_map(|run| run.glyphs())
    .filter(|glyph| (glyph.y - line.baseline).abs() < 0.01)
    .map(|glyph| glyph.x)
    .collect()
}

// Lines break to fit the width: four lines at 100 px, two at 200, none wider than the width.
#[test]
fn line_breaking_respects_max_width() {
  let mut engine = engine();

  for (max_width, count) in [(100.0, 4), (200.0, 2)] {
    let paragraph = wrapped(&mut engine, FOX, max_width);

    assert_eq!(paragraph.lines().len(), count, "{max_width}");
    assert!(paragraph.lines().iter().all(|line| line.width <= max_width));
  }
  assert_close(wrapped(&mut engine, FOX, 100.0).size().width, 84.61);
}

// Only the first max_lines lines are kept, and the size is theirs: the widest kept line by their
// heights.
#[test]
fn max_lines_caps_the_paragraph() {
  let style = ParagraphStyle {
    max_lines: NonZeroU32::new(2),
    ..wrapping(TextWrapping::WrapWithOverflow)
  };
  let paragraph = laid_out(&mut engine(), FOX, &style, TextLimits::width(100.0));

  assert_eq!(paragraph.lines().len(), 2);
  assert_close(paragraph.size().width, 68.67);
  assert_close(paragraph.size().height, 2.0 * LINE);
}

// A max_lines above the line count keeps every line.
#[test]
fn max_lines_above_the_line_count_keeps_every_line() {
  let mut engine = engine();
  let style = ParagraphStyle {
    max_lines: NonZeroU32::new(10),
    ..wrapping(TextWrapping::WrapWithOverflow)
  };

  assert_eq!(
    laid_out(&mut engine, FOX, &style, TextLimits::width(100.0)),
    wrapped(&mut engine, FOX, 100.0)
  );
}

// A max_width that is NaN or infinite doesn't wrap.
#[test]
fn unusable_max_width_does_not_wrap() {
  let mut engine = engine();
  let expected = shape(&mut engine, FOX, &style());

  for max_width in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
    assert_eq!(wrapped(&mut engine, FOX, max_width), expected, "{max_width}");
  }
  assert_eq!(expected.lines().len(), 1);
}

// A negative max_width is 0: the text breaks at every opportunity.
#[test]
fn negative_max_width_breaks_everywhere() {
  let mut engine = engine();
  let paragraph = wrapped(&mut engine, "one two three four five", -5.0);

  assert_eq!(paragraph, wrapped(&mut engine, "one two three four five", 0.0));
  assert_eq!(paragraph.lines().len(), 5);
}

// Each line starts where the previous one ends, its baseline lies inside it, and the lines'
// heights add up to the paragraph's. Left-to-right lines start at 0.
#[test]
fn lines_stack_top_to_bottom() {
  let paragraph = wrapped(&mut engine(), FOX, 100.0);
  let lines = paragraph.lines();

  assert_eq!(lines[0].top, 0.0);
  for pair in lines.windows(2) {
    assert_close(pair[1].top, pair[0].top + pair[0].height);
  }
  for line in lines {
    assert!(line.baseline > line.top && line.baseline <= line.top + line.height);
    assert_close(glyph_xs(&paragraph, line).into_iter().fold(f32::MAX, f32::min), 0.0);
  }
  assert_close(paragraph.size().height, lines.iter().map(|line| line.height).sum());
}

// Right-to-left lines end at the paragraph's right edge, the widest line's width, not at
// max_width: a shorter last line starts the difference in from the left.
#[test]
fn right_to_left_text_stays_inside_its_size() {
  let paragraph = wrapped(&mut engine(), HEBREW, 100.0);
  let width = paragraph.size().width;

  assert_eq!(paragraph.lines().len(), 3);
  assert_close(width, 71.94);
  for line in paragraph.lines() {
    let xs = glyph_xs(&paragraph, line);

    assert_close(xs.iter().copied().fold(f32::MAX, f32::min), width - line.width);
    assert!(xs.iter().all(|&x| x <= width), "{xs:?}");
  }
}

// A word wider than max_width makes its line wider than the break width, and its glyphs still
// stay inside the paragraph.
#[test]
fn overflowing_word_stays_inside_its_size() {
  let paragraph = wrapped(&mut engine(), HEBREW, 0.0);

  for line in paragraph.lines() {
    assert!(
      glyph_xs(&paragraph, line)
        .iter()
        .all(|&x| x >= -0.01 && x <= paragraph.size().width)
    );
  }
}

fn assert_inside(paragraph: &Paragraph) {
  let width = paragraph.size().width;
  for glyph in paragraph.runs().iter().flat_map(|run| run.glyphs()) {
    assert!(
      glyph.x >= -0.01 && glyph.x <= width + 0.01,
      "glyph at {} outside 0..{width}",
      glyph.x
    );
  }
}

fn glyph_count(paragraph: &Paragraph) -> usize {
  paragraph.runs().iter().map(|run| run.glyphs().len()).sum()
}

// A space at the logical start of right-to-left text sits at its right end, and no glyph lands
// outside the paragraph.
#[test]
fn right_to_left_leading_space_stays_inside() {
  assert_inside(&shape(&mut engine(), " שלום", &style()));
  assert_inside(&shape(&mut engine(), "  שלום", &style()));
}

// Latin words wrapped inside right-to-left text keep every glyph inside the paragraph.
#[test]
fn latin_wrapped_in_right_to_left_text_stays_inside() {
  assert_inside(&wrapped(&mut engine(), "שלום Hello world עולם שלום", 60.0));
}

// Whitespace at the end of a line takes no width and draws no glyphs, in either direction.
#[test]
fn trailing_whitespace_is_hung() {
  let mut engine = engine();

  for (text, trimmed) in [("abc ", "abc"), ("שלום ", "שלום")] {
    let paragraph = shape(&mut engine, text, &style());
    let expected = shape(&mut engine, trimmed, &style());

    assert_eq!(paragraph.size(), expected.size(), "{text:?}");
    assert_eq!(glyph_count(&paragraph), glyph_count(&expected), "{text:?}");
  }
}

// Spaces at the start of a left-to-right line keep their width, so indentation stays.
#[test]
fn leading_spaces_keep_their_width() {
  let mut engine = engine();
  let indented = shape(&mut engine, "  abc", &style());
  let plain = shape(&mut engine, "abc", &style());

  assert_close(indented.size().width, plain.size().width + 8.96);
}

// The empty line after a final newline has no width.
#[test]
fn empty_last_line_has_no_width() {
  let paragraph = shape(&mut engine(), "abc\n", &style());

  assert_eq!(paragraph.lines()[1].width, 0.0);
}

// A line height below the font's own still stacks the lines from 0, each baseline inside its line.
#[test]
fn tight_line_height_stacks_lines_from_zero() {
  let style = ParagraphStyle {
    line_height: Some(16.0),
    ..ParagraphStyle::default()
  };
  let paragraph = laid_out(&mut engine(), "a\nb", &style, TextLimits::NONE);
  let lines = paragraph.lines();

  assert_eq!([lines[0].top, lines[1].top], [0.0, 16.0]);
  for line in lines {
    assert!(
      line.baseline > line.top && line.baseline <= line.top + line.height,
      "{line:?}"
    );
  }
}

// Thai has no spaces between words, so it wraps only where the complex-scripts feature's
// dictionaries find word boundaries.
#[cfg(feature = "complex-scripts")]
#[test]
fn thai_text_wraps_between_words() {
  let paragraph = wrapped(&mut engine(), "ภาษาไทยเป็นภาษาที่สวยงาม", 30.0);

  assert!(paragraph.lines().len() > 1);
}

// Every line is placed by the paragraph's direction, which its first strong character sets, not
// by its own text: a Hebrew line in a left-to-right paragraph starts at 0, and a Latin line in a
// right-to-left one ends at the paragraph's right edge.
#[test]
fn lines_follow_the_paragraph_direction() {
  let mut engine = engine();
  let left_to_right = shape(&mut engine, "Hello world\nשלום", &style());
  let right_to_left = shape(&mut engine, "שלום עולם שלום\nabc", &style());

  let hebrew = &left_to_right.lines()[1];
  assert_close(glyph_xs(&left_to_right, hebrew).into_iter().fold(f32::MAX, f32::min), 0.0);
  let latin = &right_to_left.lines()[1];
  assert_close(
    glyph_xs(&right_to_left, latin).into_iter().fold(f32::MAX, f32::min),
    right_to_left.size().width - latin.width,
  );
}

const LONG_WORD: &str = "Supercalifragilisticexpialidocious";

// NoWrap breaks only at hard breaks: one line wider than the width it was given.
#[test]
fn no_wrap_keeps_one_line() {
  let paragraph = laid_out(&mut engine(), FOX, &wrapping(TextWrapping::NoWrap), TextLimits::width(100.0));

  assert_eq!(paragraph.lines().len(), 1);
  assert!(paragraph.size().width > 100.0);
}

// Wrap breaks inside a word wider than the width, so no line sticks out.
#[test]
fn wrap_breaks_a_long_word() {
  let paragraph = laid_out(
    &mut engine(),
    LONG_WORD,
    &wrapping(TextWrapping::Wrap),
    TextLimits::width(100.0),
  );

  assert!(paragraph.lines().len() > 1);
  assert!(
    paragraph.lines().iter().all(|line| line.width <= 100.0),
    "{:?}",
    paragraph.lines()
  );
}

// WrapWithOverflow breaks only between words, so a long word stays whole and sticks out.
#[test]
fn wrap_with_overflow_keeps_a_long_word() {
  let paragraph = wrapped(&mut engine(), LONG_WORD, 100.0);

  assert_eq!(paragraph.lines().len(), 1);
  assert!(paragraph.size().width > 100.0);
}

// MaxLines applies to hard-broken lines too.
#[test]
fn max_lines_with_no_wrap() {
  let style = ParagraphStyle {
    max_lines: NonZeroU32::new(2),
    ..ParagraphStyle::default()
  };

  assert_eq!(
    laid_out(&mut engine(), "a\nb\nc", &style, TextLimits::width(100.0))
      .lines()
      .len(),
    2
  );
}

// A negative width is 0, so Wrap puts each cluster on its own line.
#[test]
fn negative_max_width_breaks_every_cluster_under_wrap() {
  let paragraph = laid_out(&mut engine(), "abc", &wrapping(TextWrapping::Wrap), TextLimits::width(-5.0));

  assert_eq!(paragraph.lines().len(), 3);
}

fn limits(max_width: Option<f32>, max_height: Option<f32>) -> TextLimits {
  TextLimits { max_width, max_height }
}

fn overflowing() -> ParagraphStyle {
  wrapping(TextWrapping::WrapWithOverflow)
}

// A height of two and a half lines keeps two, and the paragraph is their height.
#[test]
fn max_height_drops_lines_that_do_not_fit() {
  let paragraph = laid_out(&mut engine(), FOX, &overflowing(), limits(Some(100.0), Some(2.5 * LINE)));

  assert_eq!(paragraph.lines().len(), 2);
  assert_close(paragraph.size().height, 2.0 * LINE);
}

// A height below one line still keeps the first line.
#[test]
fn max_height_keeps_the_first_line() {
  let paragraph = laid_out(&mut engine(), FOX, &overflowing(), limits(Some(100.0), Some(5.0)));

  assert_eq!(paragraph.lines().len(), 1);
}

// A negative height keeps one line.
#[test]
fn negative_max_height_keeps_one_line() {
  let paragraph = laid_out(&mut engine(), FOX, &overflowing(), limits(Some(100.0), Some(-10.0)));

  assert_eq!(paragraph.lines().len(), 1);
}

// A height that is NaN or infinite is no limit.
#[test]
fn unusable_max_height_is_no_limit() {
  let mut engine = engine();
  let expected = wrapped(&mut engine, FOX, 100.0);

  for max_height in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
    let paragraph = laid_out(&mut engine, FOX, &overflowing(), limits(Some(100.0), Some(max_height)));

    assert_eq!(paragraph, expected, "{max_height}");
  }
}

// A height exactly at a line's bottom keeps that line.
#[test]
fn max_height_at_a_line_bottom_keeps_that_line() {
  let mut engine = engine();
  let second = wrapped(&mut engine, FOX, 100.0).lines()[1];

  let paragraph = laid_out(
    &mut engine,
    FOX,
    &overflowing(),
    limits(Some(100.0), Some(second.top + second.height)),
  );

  assert_eq!(paragraph.lines().len(), 2);
}

// MaxLines and the height together keep the fewer lines.
#[test]
fn max_lines_and_max_height_keep_the_fewer() {
  let mut engine = engine();
  let three = ParagraphStyle {
    max_lines: NonZeroU32::new(3),
    ..overflowing()
  };
  let one = ParagraphStyle {
    max_lines: NonZeroU32::new(1),
    ..overflowing()
  };

  assert_eq!(
    laid_out(&mut engine, FOX, &three, limits(Some(100.0), Some(2.5 * LINE)))
      .lines()
      .len(),
    2
  );
  assert_eq!(
    laid_out(&mut engine, FOX, &one, limits(Some(100.0), Some(2.5 * LINE)))
      .lines()
      .len(),
    1
  );
}

// The paragraph reports the direction its first strong character gives it.
#[test]
fn paragraph_reports_its_direction() {
  let mut engine = engine();

  assert!(!shape(&mut engine, FOX, &style()).right_to_left());
  assert!(shape(&mut engine, HEBREW, &style()).right_to_left());
}

// Lines that broke to fit the width mark the paragraph as limited by it.
#[test]
fn wrapped_paragraph_is_limited_by_width() {
  let paragraph = wrapped(&mut engine(), FOX, 100.0);

  assert!(paragraph.limited_width());
  assert!(!paragraph.limited_height());
}

// Text that breaks only at hard breaks isn't limited, with or without a width.
#[test]
fn unwrapped_paragraph_is_not_limited() {
  let mut engine = engine();

  for paragraph in [
    shape(&mut engine, FOX, &style()),
    laid_out(&mut engine, "a\nb", &overflowing(), TextLimits::width(100.0)),
    laid_out(&mut engine, FOX, &ParagraphStyle::default(), TextLimits::width(100.0)),
  ] {
    assert!(!paragraph.limited_width(), "{paragraph:?}");
    assert!(!paragraph.limited_height(), "{paragraph:?}");
  }
}

// Lines dropped for the height mark the paragraph as limited by it; a height they all fit doesn't.
#[test]
fn dropped_lines_limit_the_height() {
  let mut engine = engine();

  assert!(laid_out(&mut engine, FOX, &overflowing(), limits(Some(100.0), Some(2.5 * LINE))).limited_height());
  assert!(!laid_out(&mut engine, FOX, &overflowing(), limits(Some(100.0), Some(10.0 * LINE))).limited_height());
}

// A line cut only because MaxLines dropped the ones after it isn't limited by the width.
#[test]
fn clamped_line_that_fits_is_not_limited_by_width() {
  let style = ParagraphStyle {
    max_lines: NonZeroU32::new(1),
    ..overflowing()
  };

  assert!(!laid_out(&mut engine(), "a\nb\nc", &style, TextLimits::width(100.0)).limited_width());
  let trimmed = ParagraphStyle {
    trimming: TextTrimming::CharacterEllipsis,
    ..style
  };
  assert!(!laid_out(&mut engine(), "a\nb\nc", &trimmed, TextLimits::width(100.0)).limited_width());
}

fn aligned(alignment: TextAlignment) -> ParagraphStyle {
  ParagraphStyle {
    alignment,
    ..overflowing()
  }
}

fn left_edge(paragraph: &Paragraph, line: &Line) -> f32 {
  glyph_xs(paragraph, line).into_iter().fold(f32::MAX, f32::min)
}

fn right_glyph(paragraph: &Paragraph, line: &Line) -> f32 {
  glyph_xs(paragraph, line).into_iter().fold(f32::MIN, f32::max)
}

// Center puts each line's free width half before it, End all of it, and in right-to-left text
// End is the left edge.
#[test]
fn center_and_end_place_lines() {
  let mut engine = engine();
  let center = laid_out(&mut engine, FOX, &aligned(TextAlignment::Center), TextLimits::width(100.0));
  let end = laid_out(&mut engine, FOX, &aligned(TextAlignment::End), TextLimits::width(100.0));
  let hebrew_end = laid_out(&mut engine, HEBREW, &aligned(TextAlignment::End), TextLimits::width(100.0));

  for line in center.lines() {
    assert_close(left_edge(&center, line), (center.size().width - line.width) / 2.0);
  }
  for line in end.lines() {
    assert_close(left_edge(&end, line), end.size().width - line.width);
  }
  for line in hebrew_end.lines() {
    assert_close(left_edge(&hebrew_end, line), 0.0);
  }
}

// Left and Right ignore the direction: Left starts Hebrew lines at 0, Right ends Latin lines at
// the paragraph's right edge.
#[test]
fn left_and_right_ignore_direction() {
  let mut engine = engine();
  let left = laid_out(&mut engine, HEBREW, &aligned(TextAlignment::Left), TextLimits::width(100.0));
  let right = laid_out(&mut engine, FOX, &aligned(TextAlignment::Right), TextLimits::width(100.0));

  for line in left.lines() {
    assert_close(left_edge(&left, line), 0.0);
  }
  for line in right.lines() {
    assert_close(left_edge(&right, line), right.size().width - line.width);
  }
}

// Justify stretches every line but the last to the paragraph's width, and leaves the last at the
// start.
#[test]
fn justify_fills_every_line_but_the_last() {
  let paragraph = laid_out(&mut engine(), FOX, &aligned(TextAlignment::Justify), TextLimits::width(100.0));
  let (last, rest) = paragraph.lines().split_last().unwrap();

  for line in rest {
    assert_eq!(line.width, paragraph.size().width);
    assert_close(left_edge(&paragraph, line), 0.0);
  }
  assert!(last.width < paragraph.size().width);
  assert_close(left_edge(&paragraph, last), 0.0);
}

// A line that ends at a hard break isn't stretched.
#[test]
fn justify_skips_hard_breaks_and_trimmed_lines() {
  let text = "The quick\nbrown fox jumps over the lazy dog";
  let mut engine = engine();
  let justified = laid_out(&mut engine, text, &aligned(TextAlignment::Justify), TextLimits::width(100.0));
  let start = laid_out(&mut engine, text, &overflowing(), TextLimits::width(100.0));

  assert_eq!(justified.lines()[0].width, start.lines()[0].width);
  assert!(justified.lines()[0].width < justified.size().width);
  assert!(justified.lines()[1].width == justified.size().width);
  let joined = "Super\u{a0}cali\u{a0}fragilistic\u{a0}expiali two three four five";
  let cut = ParagraphStyle {
    alignment: TextAlignment::Justify,
    trimming: TextTrimming::CharacterEllipsis,
    ..overflowing()
  };
  let paragraph = laid_out(&mut engine, joined, &cut, TextLimits::width(100.0));
  assert!(line_ids(&paragraph, &paragraph.lines()[0]).contains(&ellipsis(&mut engine)));
  assert!(paragraph.lines()[0].width < 100.0);
}

// A line without spaces, such as a piece of a word broken by Wrap, isn't stretched.
#[test]
fn justify_skips_a_line_without_spaces() {
  let text = "Supercalifragilisticexpialidocious and a few more words here";
  let justify = ParagraphStyle {
    alignment: TextAlignment::Justify,
    ..wrapping(TextWrapping::Wrap)
  };
  let mut engine = engine();
  let justified = laid_out(&mut engine, text, &justify, TextLimits::width(100.0));
  let start = laid_out(&mut engine, text, &wrapping(TextWrapping::Wrap), TextLimits::width(100.0));

  assert_eq!(justified.size().width, 100.0);
  assert_eq!(justified.lines()[0].width, start.lines()[0].width);
  assert_eq!(
    glyph_xs(&justified, &justified.lines()[0]),
    glyph_xs(&start, &start.lines()[0])
  );
}

// The space hanging at a stretched line's end takes no share: the line's last glyph moves by the
// whole free width, so the line ends at the paragraph's edge.
#[test]
fn justify_ignores_hanging_whitespace() {
  let mut engine = engine();
  let justified = laid_out(&mut engine, FOX, &aligned(TextAlignment::Justify), TextLimits::width(100.0));
  let start = laid_out(&mut engine, FOX, &overflowing(), TextLimits::width(100.0));
  let (natural, stretched) = (start.lines()[0], justified.lines()[0]);

  assert_close(
    right_glyph(&justified, &stretched),
    right_glyph(&start, &natural) + (justified.size().width - natural.width),
  );
}

// A paragraph with a stretched line is as wide as max_width, and limited by it.
#[test]
fn justified_paragraph_takes_max_width() {
  let paragraph = laid_out(&mut engine(), FOX, &aligned(TextAlignment::Justify), TextLimits::width(100.0));

  assert_eq!(paragraph.size().width, 100.0);
  assert!(paragraph.limited_width());
}

// A line wider than max_width isn't stretched, even when another line makes the paragraph wider
// still. No-break spaces keep the first line's words together.
#[test]
fn overflowing_line_is_not_justified() {
  let text = "Super\u{a0}cali\u{a0}fragi Supercalifragilisticexpialidociousness end";
  let mut engine = engine();
  let justified = laid_out(&mut engine, text, &aligned(TextAlignment::Justify), TextLimits::width(60.0));
  let start = laid_out(&mut engine, text, &overflowing(), TextLimits::width(60.0));

  assert!(start.lines()[0].width > 60.0);
  assert!(start.lines()[0].width < start.size().width);
  assert_eq!(
    glyph_xs(&justified, &justified.lines()[0]),
    glyph_xs(&start, &start.lines()[0])
  );
}

// Right-to-left justified lines span the paragraph from 0 to its width.
#[test]
fn justify_fills_right_to_left_lines() {
  let paragraph = laid_out(
    &mut engine(),
    HEBREW,
    &aligned(TextAlignment::Justify),
    TextLimits::width(100.0),
  );
  let (_, rest) = paragraph.lines().split_last().unwrap();

  assert_eq!(paragraph.size().width, 100.0);
  for line in rest {
    assert_close(left_edge(&paragraph, line), 0.0);
    assert!(right_glyph(&paragraph, line) <= paragraph.size().width);
  }
  assert_inside(&paragraph);
}

// With no width limit there's nothing to stretch to: the paragraph keeps its widest line's width.
#[test]
fn justify_without_a_width_limit_stretches_nothing() {
  let mut engine = engine();
  let justified = laid_out(
    &mut engine,
    "The quick\nbrown fox",
    &aligned(TextAlignment::Justify),
    TextLimits::NONE,
  );

  assert_eq!(
    justified,
    laid_out(&mut engine, "The quick\nbrown fox", &overflowing(), TextLimits::NONE)
  );
}

// Beside a word wider than max_width, justified lines stretch to max_width, not to the word: only
// the word overflows.
#[test]
fn justified_lines_stop_at_max_width() {
  let text = "Supercalifragilisticexpialidociousness one two three four five six seven";
  let paragraph = laid_out(&mut engine(), text, &aligned(TextAlignment::Justify), TextLimits::width(60.0));
  let (word, rest) = paragraph.lines().split_first().unwrap();

  assert!(word.width > 60.0);
  assert_eq!(paragraph.size().width, word.width);
  assert!(rest.iter().any(|line| line.width == 60.0), "{rest:?}");
  for line in rest {
    assert!(line.width <= 60.0, "{line:?}");
    assert!(right_glyph(&paragraph, line) < 60.0, "{line:?}");
  }
}

fn trimming(trimming: TextTrimming) -> ParagraphStyle {
  ParagraphStyle {
    trimming,
    ..ParagraphStyle::default()
  }
}

fn trimmed(engine: &mut Engine, text: &str, mode: TextTrimming, max_width: f32) -> Paragraph {
  laid_out(engine, text, &trimming(mode), TextLimits::width(max_width))
}

fn ellipsis(engine: &mut Engine) -> u32 {
  shape(engine, "\u{2026}", &style()).runs()[0].glyphs()[0].id
}

fn ids(paragraph: &Paragraph) -> Vec<u32> {
  let mut glyphs = paragraph
    .runs()
    .iter()
    .flat_map(|run| run.glyphs())
    .copied()
    .collect::<Vec<_>>();
  glyphs.sort_by(|a, b| (a.y, a.x).partial_cmp(&(b.y, b.x)).unwrap());
  glyphs.into_iter().map(|glyph| glyph.id).collect()
}

fn line_ids(paragraph: &Paragraph, line: &Line) -> Vec<u32> {
  let mut glyphs = paragraph
    .runs()
    .iter()
    .flat_map(|run| run.glyphs())
    .filter(|glyph| (glyph.y - line.baseline).abs() < 0.01)
    .copied()
    .collect::<Vec<_>>();
  glyphs.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
  glyphs.into_iter().map(|glyph| glyph.id).collect()
}

const PATH: &str = r"C:\Users\ada\projects\artic\docs\design\text-block.md";
const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz0123456789";

// CharacterEllipsis keeps the start, cut between clusters, then "…", within the width.
#[test]
fn character_ellipsis_cuts_between_clusters() {
  let mut engine = engine();
  let paragraph = trimmed(&mut engine, FOX, TextTrimming::CharacterEllipsis, 100.0);
  let shown = ids(&paragraph);

  assert_eq!(paragraph.lines().len(), 1);
  assert!(paragraph.size().width <= 100.0);
  assert_eq!(shown.last(), Some(&ellipsis(&mut engine)));
  assert!((1..FOX.len()).any(|cut| shown == ids(&shape(&mut engine, &format!("{}\u{2026}", FOX[..cut].trim_end()), &style()))));
  assert!(paragraph.limited_width());
}

// WordEllipsis cuts before a word.
#[test]
fn word_ellipsis_cuts_at_a_word_boundary() {
  let mut engine = engine();
  let shown = ids(&trimmed(&mut engine, FOX, TextTrimming::WordEllipsis, 100.0));
  let word_ends = FOX.match_indices(' ').map(|(at, _)| at);

  assert!(
    word_ends
      .into_iter()
      .any(|end| shown == ids(&shape(&mut engine, &format!("{}\u{2026}", &FOX[..end]), &style())))
  );
}

// A single word too long for the width is cut between characters.
#[test]
fn word_ellipsis_falls_back_to_characters() {
  let mut engine = engine();
  let paragraph = trimmed(&mut engine, LONG_WORD, TextTrimming::WordEllipsis, 100.0);
  let shown = ids(&paragraph);

  assert!(shown.len() > 1);
  assert_eq!(shown.last(), Some(&ellipsis(&mut engine)));
  assert!(paragraph.size().width <= 100.0);
}

// PrefixCharacterEllipsis keeps the first 8 clusters, "…", then the end.
#[test]
fn prefix_ellipsis_keeps_eight_clusters_and_the_end() {
  let mut engine = engine();
  let paragraph = trimmed(&mut engine, LETTERS, TextTrimming::PrefixCharacterEllipsis, 120.0);
  let shown = ids(&paragraph);

  assert_eq!(shown[8], ellipsis(&mut engine));
  assert!(paragraph.size().width <= 120.0);
  assert!(
    (9..LETTERS.len()).any(|cut| shown == ids(&shape(&mut engine, &format!("abcdefgh\u{2026}{}", &LETTERS[cut..]), &style())))
  );
}

// When 8 clusters and "…" don't fit, PrefixCharacterEllipsis cuts as CharacterEllipsis does.
#[test]
fn prefix_ellipsis_falls_back_to_characters() {
  let mut engine = engine();
  let paragraph = trimmed(&mut engine, LETTERS, TextTrimming::PrefixCharacterEllipsis, 40.0);

  assert_eq!(ids(&paragraph).last(), Some(&ellipsis(&mut engine)));
  assert!(paragraph.size().width <= 40.0);
}

// LeadingCharacterEllipsis puts "…" first and keeps the end.
#[test]
fn leading_ellipsis_keeps_the_end() {
  let mut engine = engine();
  let shown = ids(&trimmed(&mut engine, FOX, TextTrimming::LeadingCharacterEllipsis, 100.0));

  assert_eq!(shown.first(), Some(&ellipsis(&mut engine)));
  assert!(
    (1..FOX.len()).any(|cut| shown == ids(&shape(&mut engine, &format!("\u{2026}{}", FOX[cut..].trim_start()), &style())))
  );
}

// PathSegmentEllipsis keeps the first and last segments and replaces middle ones with "…".
#[test]
fn path_ellipsis_drops_middle_segments() {
  let mut engine = engine();
  let paragraph = trimmed(&mut engine, PATH, TextTrimming::PathSegmentEllipsis, 200.0);
  let shown = ids(&paragraph);
  let head = ids(&shape(&mut engine, r"C:\", &style()));
  let tail = ids(&shape(&mut engine, r"\text-block.md", &style()));

  assert!(paragraph.size().width <= 200.0);
  assert!(shown.starts_with(&head) && shown.ends_with(&tail), "{shown:?}");
  assert_eq!(shown.iter().filter(|&&id| id == ellipsis(&mut engine)).count(), 1);
}

// When even the first and last segments don't fit, PathSegmentEllipsis cuts as
// LeadingCharacterEllipsis does.
#[test]
fn path_ellipsis_falls_back_to_leading() {
  let mut engine = engine();
  let paragraph = trimmed(&mut engine, PATH, TextTrimming::PathSegmentEllipsis, 60.0);

  assert_eq!(ids(&paragraph).first(), Some(&ellipsis(&mut engine)));
  assert!(paragraph.size().width <= 60.0);
}

// A clamped line's text is its own: nothing after its hard break shows.
#[test]
fn trimming_stops_at_a_hard_break() {
  let mut engine = engine();
  let clamp = ParagraphStyle {
    max_lines: NonZeroU32::new(1),
    ..trimming(TextTrimming::CharacterEllipsis)
  };
  let paragraph = laid_out(&mut engine, "The quick brown fox\njumps", &clamp, TextLimits::width(1000.0));

  assert_eq!(
    ids(&paragraph),
    ids(&shape(&mut engine, "The quick brown fox\u{2026}", &style()))
  );
}

// U+2028 LINE SEPARATOR is a hard break too.
#[test]
fn trimming_stops_at_a_line_separator() {
  let mut engine = engine();
  let clamp = ParagraphStyle {
    max_lines: NonZeroU32::new(1),
    ..trimming(TextTrimming::CharacterEllipsis)
  };
  let paragraph = laid_out(&mut engine, "abc\u{2028}def", &clamp, TextLimits::width(1000.0));

  assert_eq!(ids(&paragraph), ids(&shape(&mut engine, "abc\u{2026}", &style())));
}

// A line that fits but is followed by dropped lines still gets "…".
#[test]
fn clamped_line_that_fits_still_gets_an_ellipsis() {
  let mut engine = engine();
  let clamp = ParagraphStyle {
    max_lines: NonZeroU32::new(1),
    ..trimming(TextTrimming::CharacterEllipsis)
  };

  assert_eq!(
    ids(&laid_out(&mut engine, "a\nb\nc", &clamp, TextLimits::width(100.0))),
    ids(&shape(&mut engine, "a\u{2026}", &style()))
  );
}

// Text that ends in a line break leaves only whitespace after its last kept line: no "…".
#[test]
fn trailing_line_break_is_not_trimmed() {
  let mut engine = engine();
  let clamp = ParagraphStyle {
    max_lines: NonZeroU32::new(1),
    ..trimming(TextTrimming::CharacterEllipsis)
  };

  assert!(!ids(&laid_out(&mut engine, "abc\n", &clamp, TextLimits::width(100.0))).contains(&ellipsis(&mut engine)));
}

// With room for nothing, CharacterEllipsis and LeadingCharacterEllipsis draw "…" alone,
// overhanging.
#[test]
fn ellipsis_alone_when_nothing_fits() {
  let mut engine = engine();
  let alone = vec![ellipsis(&mut engine)];

  for mode in [TextTrimming::CharacterEllipsis, TextTrimming::LeadingCharacterEllipsis] {
    assert_eq!(ids(&trimmed(&mut engine, FOX, mode, 1.0)), alone, "{mode:?}");
  }
}

// TextTrimming::None draws the line whole.
#[test]
fn no_trimming_draws_no_ellipsis() {
  let mut engine = engine();

  assert_eq!(
    trimmed(&mut engine, FOX, TextTrimming::None, 100.0),
    shape(&mut engine, FOX, &style())
  );
}

// A trimmed line keeps the top, height and baseline of the line it replaces.
#[test]
fn trimmed_line_keeps_its_metrics() {
  let mut engine = engine();
  let clamped = ParagraphStyle {
    max_lines: NonZeroU32::new(2),
    ..wrapping(TextWrapping::WrapWithOverflow)
  };
  let trimmed = ParagraphStyle {
    trimming: TextTrimming::CharacterEllipsis,
    ..clamped
  };
  let plain = laid_out(&mut engine, FOX, &clamped, TextLimits::width(100.0));
  let cut = laid_out(&mut engine, FOX, &trimmed, TextLimits::width(100.0));

  for (before, after) in plain.lines().iter().zip(cut.lines()) {
    assert_eq!(
      (before.top, before.height, before.baseline),
      (after.top, after.height, after.baseline)
    );
  }
}

// A paragraph's size and placement describe the trimmed line: its width is the cut line's, and
// no glyph lies past it.
#[test]
fn trimmed_size_matches_what_is_drawn() {
  let paragraph = trimmed(&mut engine(), FOX, TextTrimming::CharacterEllipsis, 100.0);

  assert_eq!(paragraph.size().width, paragraph.lines()[0].width);
  assert_inside(&paragraph);
}

// In a right-to-left paragraph a Latin line is trimmed right to left: "…" on its left.
#[test]
fn trimmed_latin_line_in_rtl_paragraph() {
  let mut engine = engine();
  let paragraph = trimmed(
    &mut engine,
    "שלום\nHello world and more words",
    TextTrimming::CharacterEllipsis,
    60.0,
  );
  let latin = paragraph.lines()[1];

  assert_eq!(line_ids(&paragraph, &latin).first(), Some(&ellipsis(&mut engine)));
}

// Trimmed right-to-left text keeps every glyph inside its size, "…" on the left.
#[test]
fn trimmed_right_to_left_text_stays_inside_its_size() {
  let mut engine = engine();
  let paragraph = trimmed(&mut engine, HEBREW, TextTrimming::CharacterEllipsis, 60.0);

  assert!(paragraph.size().width <= 60.0);
  assert_inside(&paragraph);
  assert_eq!(ids(&paragraph).first(), Some(&ellipsis(&mut engine)));
}

// A cut never separates a base letter from its combining mark.
#[test]
fn combining_mark_stays_with_its_base() {
  let mut engine = engine();
  let text = "e\u{301}".repeat(12);
  let shown = ids(&trimmed(&mut engine, &text, TextTrimming::CharacterEllipsis, 40.0));

  assert!((0..=12).any(|kept| shown == ids(&shape(&mut engine, &format!("{}\u{2026}", "e\u{301}".repeat(kept)), &style()))));
}

// Center places a trimmed line by its cut width.
#[test]
fn trimmed_line_is_aligned_by_its_new_width() {
  let clamp = ParagraphStyle {
    alignment: TextAlignment::Center,
    ..trimming(TextTrimming::CharacterEllipsis)
  };
  let paragraph = laid_out(&mut engine(), &format!("short\n{FOX}"), &clamp, TextLimits::width(100.0));

  for line in paragraph.lines() {
    assert_close(left_edge(&paragraph, line), (paragraph.size().width - line.width) / 2.0);
  }
}

// A NaN width is no width limit, but MaxLines still trims the last kept line.
#[test]
fn unusable_max_width_still_trims_a_clamped_line() {
  let mut engine = engine();
  let clamp = ParagraphStyle {
    max_lines: NonZeroU32::new(1),
    ..trimming(TextTrimming::CharacterEllipsis)
  };

  assert_eq!(
    ids(&laid_out(&mut engine, "abc\ndef", &clamp, TextLimits::width(f32::NAN))),
    ids(&shape(&mut engine, "abc\u{2026}", &style()))
  );
}

// Lines dropped by the height trim the last kept line, as MaxLines does.
#[test]
fn dropped_by_height_trims_the_last_line() {
  let mut engine = engine();
  let clamp = ParagraphStyle {
    trimming: TextTrimming::CharacterEllipsis,
    ..wrapping(TextWrapping::WrapWithOverflow)
  };
  let paragraph = laid_out(
    &mut engine,
    FOX,
    &clamp,
    TextLimits {
      max_width: Some(100.0),
      max_height: Some(2.5 * LINE),
    },
  );
  let last = paragraph.lines()[1];

  assert_eq!(paragraph.lines().len(), 2);
  assert_eq!(line_ids(&paragraph, &last).last(), Some(&ellipsis(&mut engine)));
}

// Empty text with trimming in a tiny box draws nothing.
#[test]
fn trimming_empty_text_draws_nothing() {
  let clamp = trimming(TextTrimming::CharacterEllipsis);
  let paragraph = laid_out(
    &mut engine(),
    "",
    &clamp,
    TextLimits {
      max_width: Some(1.0),
      max_height: Some(-1.0),
    },
  );

  assert!(paragraph.is_empty());
}

// A line cut because the lines after it were dropped keeps its whole text and gains "…", in every
// mode, and isn't limited by the width.
#[test]
fn clamped_line_keeps_its_whole_text_in_every_mode() {
  let mut engine = engine();
  let expected = ids(&shape(&mut engine, "abcdefghijk\u{2026}", &style()));

  for mode in [
    TextTrimming::CharacterEllipsis,
    TextTrimming::WordEllipsis,
    TextTrimming::PrefixCharacterEllipsis,
    TextTrimming::LeadingCharacterEllipsis,
    TextTrimming::PathSegmentEllipsis,
  ] {
    let clamp = ParagraphStyle {
      max_lines: NonZeroU32::new(1),
      ..trimming(mode)
    };
    let paragraph = laid_out(&mut engine, "abcdefghijk\nmore", &clamp, TextLimits::width(1000.0));

    assert_eq!(ids(&paragraph), expected, "{mode:?}");
    assert!(!paragraph.limited_width(), "{mode:?}");
  }
}

// A clamped line never shows text from the lines dropped after it, even with room for it.
#[test]
fn clamped_line_never_shows_dropped_text() {
  let mut engine = engine();
  let room = shape(&mut engine, "ab cd\u{2026}", &style()).size().width;
  let clamp = ParagraphStyle {
    max_lines: NonZeroU32::new(1),
    trimming: TextTrimming::CharacterEllipsis,
    ..overflowing()
  };
  let paragraph = laid_out(&mut engine, "ab cdefghij", &clamp, TextLimits::width(room));

  assert_eq!(ids(&paragraph), ids(&shape(&mut engine, "ab\u{2026}", &style())));
}
