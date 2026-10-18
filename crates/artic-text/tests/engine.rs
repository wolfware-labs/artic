use artic_layout::text::{
  FontFamily, FontStyle, FontWeight, GenericFamily, Line, Paragraph, TextEngine, TextLimits, TextStyle,
};
use artic_test_fonts::{BOLD, REGULAR};
use artic_text::Engine;
use pretty_assertions::assert_eq;
use std::num::NonZeroU32;
use std::sync::Arc;

const LINE: f32 = 19.84;

fn font(bytes: &'static [u8]) -> Arc<dyn AsRef<[u8]> + Send + Sync> {
  Arc::new(bytes)
}

fn engine() -> Engine {
  Engine::with_fonts([font(REGULAR), font(BOLD)])
}

fn style() -> TextStyle {
  TextStyle::new("Atkinson Hyperlegible".parse().unwrap(), 16.0)
}

fn shape(engine: &mut Engine, text: &str, style: &TextStyle) -> Paragraph {
  engine.paragraph(text, style, TextLimits::NONE)
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
  let paragraph = shape(&mut Engine::with_fonts([]), "Hello", &style());

  assert_eq!(paragraph, Paragraph::default());
}

// Bytes that aren't a font are skipped, and the first font that loads is the default family.
#[test]
fn unreadable_font_bytes_are_skipped() {
  let mut engine = Engine::with_fonts([font(b"not a font"), font(REGULAR)]);
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
  let paragraph = shape(&mut Engine::with_fonts([font(REGULAR)]), "Hello", &style);

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
  let style = TextStyle {
    line_height: Some(30.0),
    ..style()
  };
  let paragraph = shape(&mut engine(), "a\nb", &style);

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
    let style = TextStyle {
      line_height: Some(line_height),
      ..style()
    };

    assert_eq!(shape(&mut engine, "a\nb", &style), expected, "{line_height}");
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
  let mut engine = Engine::with_fonts([font(REGULAR)]);
  shape(&mut engine, "Hello", &style());

  assert_eq!(
    format!("{engine:?}"),
    r#"Engine { default_family: Named("Atkinson Hyperlegible"), faces: 1 }"#
  );
}

const FOX: &str = "The quick brown fox jumps over the lazy dog";
const HEBREW: &str = "שלום עולם שלום עולם שלום";

fn wrapped(engine: &mut Engine, text: &str, max_width: f32) -> Paragraph {
  engine.paragraph(text, &style(), TextLimits::wrap(max_width))
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
  let limits = TextLimits {
    max_width: Some(100.0),
    max_lines: NonZeroU32::new(2),
  };
  let paragraph = engine().paragraph(FOX, &style(), limits);

  assert_eq!(paragraph.lines().len(), 2);
  assert_close(paragraph.size().width, 68.67);
  assert_close(paragraph.size().height, 2.0 * LINE);
}

// A max_lines above the line count keeps every line.
#[test]
fn max_lines_above_the_line_count_keeps_every_line() {
  let mut engine = engine();
  let limits = TextLimits {
    max_width: Some(100.0),
    max_lines: NonZeroU32::new(10),
  };

  assert_eq!(engine.paragraph(FOX, &style(), limits), wrapped(&mut engine, FOX, 100.0));
}

// A max_width that is NaN or infinite doesn't wrap.
#[test]
fn unusable_max_width_does_not_wrap() {
  let mut engine = engine();
  let expected = engine.paragraph(FOX, &style(), TextLimits::NONE);

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
  let style = TextStyle {
    line_height: Some(16.0),
    ..style()
  };
  let paragraph = shape(&mut engine(), "a\nb", &style);
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
