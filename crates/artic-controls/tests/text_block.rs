// TextBlock shows a string in the font and brush its nearest ancestor sets, wrapped, trimmed and
// aligned within the box layout gives it.

use artic_controls::{StackPanel, TextBlock, TextElement};
use artic_core::Object;
use artic_core::property::AnyProperty;
use artic_core::property::{Affects, Precedence};
use artic_core::tree::{NodeId, UiTree};
use artic_element::{AccessCx, Children, ElementTypes};
use artic_layout::geometry::{Point, Rect, Size};
use artic_layout::text::{
  FontFamily, FontStyle, FontWeight, Paragraph, ParagraphStyle, TextAlignment, TextEngine, TextLimits, TextStyle, TextTrimming,
  TextWrapping,
};
use artic_layout::{ArrangeCx, MeasureCx};
use artic_layout::{LayoutNodes, Layoutable, Orientation};
use artic_render::{Brush, Color, Command};
use artic_test::layout::{Boxes, Headless};
use artic_test::layout::{Invariants, strategy};
use proptest::prelude::*;
use std::cell::Cell;
use std::num::NonZeroU32;

// TextElement's properties start at system-ui, 14, normal weight and style, and black, and a value
// set on a panel reaches the nodes under it.
#[test]
fn text_element_defaults_and_inheritance() {
  let mut tree = UiTree::new();
  let panel = tree.alloc(Box::new(StackPanel::default()));
  let child = tree.alloc(Box::new(StackPanel::default()));
  Children::add(&mut tree, panel, child);

  assert_eq!(TextElement::font_family(&tree, child), FontFamily::default());
  assert_eq!(TextElement::font_size(&tree, child), 14.0);
  assert_eq!(TextElement::font_weight(&tree, child), FontWeight::NORMAL);
  assert_eq!(TextElement::font_style(&tree, child), FontStyle::Normal);
  assert_eq!(TextElement::foreground(&tree, child), Brush::from(Color::rgb(0, 0, 0)));

  TextElement::set_font_size(&mut tree, panel, 20.0);
  TextElement::set_foreground(&mut tree, panel, Color::rgb(200, 0, 0).into());

  assert_eq!(TextElement::font_size(&tree, child), 20.0);
  assert_eq!(TextElement::foreground(&tree, child), Brush::from(Color::rgb(200, 0, 0)));
}

/// Real elements first, then the shared test leaves.
struct Nodes(ElementTypes);

impl LayoutNodes for Nodes {
  fn layoutable<'a>(&self, tree: &'a UiTree, node: NodeId) -> Option<&'a dyn Layoutable> {
    self
      .0
      .layoutable(tree, node)
      .or_else(|| tree.get_as::<Frame>(node).map(|frame| frame as &dyn Layoutable))
      .or_else(|| Boxes.layoutable(tree, node))
  }
}

/// The window most tests lay out in.
const WINDOW: Size = Size::new(200.0, 300.0);
/// A sentence long enough to wrap and trim at test widths.
const SENTENCE: &str = "The quick brown fox jumps over the lazy dog";

/// A headless layout of real elements and the shared test leaves.
fn layout() -> Headless<Nodes> {
  Headless::new(Nodes(ElementTypes::from_link_time()))
}

/// A TextBlock showing `text`, otherwise at its defaults.
fn text_block(text: &str) -> TextBlock {
  let block = TextBlock::default();
  block.text.set(text.to_owned());
  block
}

/// The TextBlock at `node`.
fn get(layout: &mut Headless<Nodes>, node: NodeId) -> &TextBlock {
  layout.tree().get_as::<TextBlock>(node).expect("a text block")
}

/// The commands the node records.
fn drawn_commands(layout: &mut Headless<Nodes>, node: NodeId) -> Vec<Command> {
  layout.record(node, &ElementTypes::from_link_time())
}

/// The id of the ellipsis glyph in the test font at TextBlock's default size.
fn ellipsis() -> u32 {
  let style = TextStyle::new(FontFamily::default(), 14.0);
  let paragraph = artic_test::text::engine().paragraph("\u{2026}", &style, &ParagraphStyle::default(), TextLimits::NONE);
  paragraph.runs()[0].glyphs()[0].id
}

/// The paragraph's glyph ids, left to right on each line, top line first.
fn glyph_ids(paragraph: &Paragraph) -> Vec<u32> {
  let mut glyphs = paragraph
    .runs()
    .iter()
    .flat_map(|run| run.glyphs())
    .copied()
    .collect::<Vec<_>>();
  glyphs.sort_by(|a, b| (a.y, a.x).partial_cmp(&(b.y, b.x)).expect("finite positions"));
  glyphs.into_iter().map(|glyph| glyph.id).collect()
}

/// The paragraph the node draws and where.
fn drawn(layout: &mut Headless<Nodes>, node: NodeId) -> (Paragraph, Point) {
  drawn_commands(layout, node)
    .into_iter()
    .find_map(|command| match command {
      Command::Text { paragraph, origin, .. } => Some((paragraph, origin)),
      _ => None,
    })
    .expect("a text command")
}

// NoWrap keeps one line, as wide as the text, however narrow the window.
#[test]
fn textblock_nowrap_single_line() {
  let mut layout = layout();
  let root = layout.add("text", text_block(SENTENCE));

  layout.run(root, Size::new(80.0, 300.0), 1.0);

  let (paragraph, _) = drawn(&mut layout, root);
  assert_eq!(paragraph.lines().len(), 1);
  assert!(layout.manager().desired(root).expect("measured").width > 80.0);
}

// Wrap breaks lines to the available width.
#[test]
fn textblock_wraps_at_width() {
  let mut layout = layout();
  let root = layout.add("text", text_block(SENTENCE));
  get(&mut layout, root).text_wrapping.set(TextWrapping::Wrap);

  layout.run(root, Size::new(80.0, 300.0), 1.0);

  let (paragraph, _) = drawn(&mut layout, root);
  assert!(paragraph.lines().len() > 2);
  assert!(paragraph.lines().iter().all(|line| line.width <= 80.0));
}

// MaxLines keeps that many lines.
#[test]
fn textblock_max_lines() {
  let mut layout = layout();
  let root = layout.add("text", text_block(SENTENCE));
  get(&mut layout, root).text_wrapping.set(TextWrapping::Wrap);
  get(&mut layout, root).max_lines.set(NonZeroU32::new(2));

  layout.run(root, Size::new(80.0, 300.0), 1.0);

  assert_eq!(drawn(&mut layout, root).0.lines().len(), 2);
}

// Trimming cuts a line that doesn't fit, keeps it inside the width, and puts "…" where the mode
// says: last for CharacterEllipsis, first for LeadingCharacterEllipsis.
#[test]
fn textblock_trims_with_ellipsis() {
  for (mode, first) in [
    (TextTrimming::CharacterEllipsis, false),
    (TextTrimming::LeadingCharacterEllipsis, true),
  ] {
    let mut layout = layout();
    let root = layout.add("text", text_block(SENTENCE));
    get(&mut layout, root).text_trimming.set(mode);

    layout.run(root, Size::new(80.0, 300.0), 1.0);

    let (paragraph, _) = drawn(&mut layout, root);
    let glyphs = glyph_ids(&paragraph);
    let at = if first { glyphs.first() } else { glyphs.last() };
    assert!(paragraph.size().width <= 80.0, "{mode:?}");
    assert!(paragraph.limited_width(), "{mode:?}");
    assert_eq!(at, Some(&ellipsis()), "{mode:?}");
  }
}

// With trimming, the available height drops the lines that don't fit.
#[test]
fn textblock_trims_to_the_available_height() {
  let mut layout = layout();
  let root = layout.add("text", text_block(SENTENCE));
  get(&mut layout, root).text_wrapping.set(TextWrapping::Wrap);
  get(&mut layout, root).text_trimming.set(TextTrimming::CharacterEllipsis);

  layout.run(root, Size::new(80.0, 30.0), 1.0);

  let (paragraph, _) = drawn(&mut layout, root);
  assert_eq!(paragraph.lines().len(), 1);
  assert!(paragraph.limited_height());
}

// Empty text has no width and the height of one line.
#[test]
fn empty_textblock_is_one_line_high() {
  let mut layout = layout();
  let root = layout.add("text", text_block(""));

  layout.run(root, WINDOW, 1.0);

  let desired = layout.manager().desired(root).expect("measured");
  assert_eq!(desired.width, 0.0);
  assert!(desired.height > 0.0);
}

/// A StackPanel holding one TextBlock of `text` in both trees: the panel and the block.
fn in_panel(layout: &mut Headless<Nodes>, text: &str) -> (NodeId, NodeId) {
  let panel = layout.add("panel", StackPanel::default());
  let block = layout.add("text", text_block(text));
  Children::add(layout.tree(), panel, block);
  (panel, block)
}

// A panel's TextElement.FontSize reaches a TextBlock that sets none.
#[test]
fn font_size_inherited_from_parent() {
  let mut layout = layout();
  let (panel, block) = in_panel(&mut layout, "Hi");
  TextElement::set_font_size(layout.tree(), panel, 30.0);

  layout.run(panel, WINDOW, 1.0);

  assert_eq!(get(&mut layout, block).font_size.get(), 30.0);
  assert_eq!(drawn(&mut layout, block).0.runs()[0].size(), 30.0);
}

// A TextBlock's own FontSize wins over the panel's.
#[test]
fn own_font_size_wins_over_inherited() {
  let mut layout = layout();
  let (panel, block) = in_panel(&mut layout, "Hi");
  TextElement::set_font_size(layout.tree(), panel, 30.0);
  get(&mut layout, block).font_size.set(10.0);

  layout.run(panel, WINDOW, 1.0);

  assert_eq!(drawn(&mut layout, block).0.runs()[0].size(), 10.0);
}

// Clearing a TextBlock's own FontSize falls back to the inherited one.
#[test]
fn cleared_font_size_inherits_again() {
  let mut layout = layout();
  let (panel, block) = in_panel(&mut layout, "Hi");
  TextElement::set_font_size(layout.tree(), panel, 30.0);
  get(&mut layout, block).font_size.set(10.0);
  layout.run(panel, WINDOW, 1.0);

  get(&mut layout, block).font_size.clear(Precedence::Local);
  layout.run(panel, WINDOW, 1.0);

  assert_eq!(get(&mut layout, block).font_size.get(), 30.0);
  assert_eq!(drawn(&mut layout, block).0.runs()[0].size(), 30.0);
}

// TextElement.FontSize set on a TextBlock, as a UI language does, is its own FontSize.
#[test]
fn attached_font_size_on_a_textblock_is_its_own() {
  let mut layout = layout();
  let block = layout.add("text", text_block("Hi"));

  TextElement::set_font_size(layout.tree(), block, 22.0);

  assert_eq!(get(&mut layout, block).font_size.get(), 22.0);
}

// Changing the panel's font size measures the TextBlock again.
#[test]
fn inherited_change_remeasures() {
  let mut layout = layout();
  let (panel, block) = in_panel(&mut layout, "Hi");
  layout.run(panel, WINDOW, 1.0);
  let before = layout.manager().desired(block).expect("measured");

  TextElement::set_font_size(layout.tree(), panel, 40.0);
  layout.run(panel, WINDOW, 1.0);

  assert!(layout.manager().desired(block).expect("measured").height > before.height);
}

// Changing Text measures again.
#[test]
fn text_change_invalidates_measure() {
  let mut layout = layout();
  let root = layout.add("text", text_block("Hi"));
  layout.run(root, WINDOW, 1.0);
  let before = layout.manager().desired(root).expect("measured");

  get(&mut layout, root).text.set(SENTENCE.to_owned());
  layout.run(root, WINDOW, 1.0);

  assert!(layout.manager().desired(root).expect("measured").width > before.width);
}

// Changing Foreground marks the TextBlock to be drawn again, with the new brush, without measuring
// it again. A new node starts dirty for every stage, so the render flag is cleared first.
#[test]
fn foreground_change_does_not_remeasure() {
  let mut layout = layout();
  let root = layout.add("text", text_block("Hi"));
  layout.run(root, WINDOW, 1.0);
  layout.tree().clear_dirty(root, Affects::RENDER);

  get(&mut layout, root).foreground.set(Color::rgb(0, 0, 255).into());
  let needs_drawing = layout.tree().dirty(root).contains(Affects::RENDER);
  let stats = layout.run(root, WINDOW, 1.0);

  assert!(needs_drawing);
  assert_eq!(stats.measured, 0);
  assert!(drawn_commands(&mut layout, root).iter().any(|command| matches!(
    command,
    Command::Text { brush, .. } if *brush == Brush::from(Color::rgb(0, 0, 255))
  )));
}

// Center places the paragraph in the middle of a wider box.
#[test]
fn alignment_places_the_paragraph() {
  let mut layout = layout();
  let root = layout.add("text", text_block("Hi"));
  get(&mut layout, root).text_alignment.set(TextAlignment::Center);
  get(&mut layout, root).layout.width.set(Some(100.0));

  layout.run(root, WINDOW, 1.0);

  let (paragraph, origin) = drawn(&mut layout, root);
  assert!((origin.x - (100.0 - paragraph.size().width) / 2.0).abs() < 0.01);
  assert_eq!(origin.y, 0.0);
}

// Right-to-left text placed by Start sits at the right edge of a wider box.
#[test]
fn right_to_left_start_sits_at_the_right() {
  let mut layout = layout();
  let root = layout.add("text", text_block("שלום"));
  get(&mut layout, root).layout.width.set(Some(100.0));

  layout.run(root, WINDOW, 1.0);

  let (paragraph, origin) = drawn(&mut layout, root);
  assert!((origin.x - (100.0 - paragraph.size().width)).abs() < 0.01);
}

// In a horizontal StackPanel the width is infinite: Wrap keeps one line.
#[test]
fn infinite_width_keeps_one_line() {
  let mut layout = layout();
  let panel = StackPanel::default();
  panel.orientation.set(Orientation::Horizontal);
  let panel = layout.add("panel", panel);
  let block = layout.add("text", text_block(SENTENCE));
  Children::add(layout.tree(), panel, block);
  get(&mut layout, block).text_wrapping.set(TextWrapping::Wrap);

  layout.run(panel, Size::new(80.0, 300.0), 1.0);

  assert_eq!(drawn(&mut layout, block).0.lines().len(), 1);
}

// The accessibility node is a label whose value is the whole text, and follows Text.
#[test]
fn textblock_accesskit_node() {
  let mut tree = UiTree::new();
  let node = tree.alloc(Box::new(text_block(SENTENCE)));
  let types = ElementTypes::from_link_time();
  let access = |tree: &UiTree| {
    let element = types.element(tree.get(node).expect("a node")).expect("an element");
    element.accessibility(&AccessCx::new(tree, node, Rect::new(0.0, 0.0, 10.0, 10.0)))
  };

  assert_eq!(access(&tree).role(), accesskit::Role::Label);
  assert_eq!(access(&tree).value(), Some(SENTENCE));
  tree.get_as::<TextBlock>(node).expect("a text block").text.set(String::new());
  assert_eq!(access(&tree).value(), Some(""));
}

/// Measures its one child in `available` and arranges it in a box of `arranged`.
#[derive(Debug)]
struct Frame {
  available: Cell<Size>,
  arranged: Cell<Size>,
}

impl Object for Frame {
  fn visit_properties(&self, _visit: &mut dyn FnMut(&dyn AnyProperty)) {}
}

impl Layoutable for Frame {
  fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    let child = cx.children().next().expect("a child");
    cx.measure(child, self.available.get())
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, _rect: Rect) {
    let child = cx.children().next().expect("a child");
    let size = self.arranged.get();
    cx.arrange(child, Rect::new(0.0, 0.0, size.width, size.height));
  }
}

/// A frame around a TextBlock of `text`, measured in `available`; the second run arranges it in
/// `arranged` and returns whether that arrange shaped the text again.
fn reshaped_by_second_arrange(block: TextBlock, available: Size, arranged: Size, scale: f32) -> bool {
  let mut layout = layout();
  let text = layout.add("text", block);
  let frame = layout.parent(
    "frame",
    Frame {
      available: Cell::new(available),
      arranged: Cell::new(available),
    },
    &[text],
  );
  layout.run(frame, WINDOW, scale);
  layout.tree().get_as::<Frame>(frame).expect("a frame").arranged.set(arranged);
  layout.manager().invalidate_arrange(frame);
  let stats = layout.run(frame, WINDOW, scale);
  stats.results.contains(&text)
}

/// A TextBlock of `text` that wraps as `wrapping` says.
fn wrapping(text: &str, wrapping: TextWrapping) -> TextBlock {
  let block = text_block(text);
  block.text_wrapping.set(wrapping);
  block
}

// A box 0.6 px narrower than the measure, less than a device pixel at scale 1, shapes nothing.
// The first run's box snaps to 101, so the second box (100) is a new rectangle and arrange runs.
#[test]
fn snapping_alone_does_not_shape_again() {
  let block = wrapping(SENTENCE, TextWrapping::Wrap);

  assert!(!reshaped_by_second_arrange(
    block,
    Size::new(100.6, 300.0),
    Size::new(100.0, 300.0),
    1.0
  ));
}

// The same 0.6 px is more than a device pixel at scale 3, so it shapes again there.
#[test]
fn snapping_threshold_follows_the_scale() {
  let block = wrapping(SENTENCE, TextWrapping::Wrap);

  assert!(reshaped_by_second_arrange(
    block,
    Size::new(100.6, 300.0),
    Size::new(100.0, 300.0),
    3.0
  ));
}

// Wrapped text in a clearly narrower box shapes again.
#[test]
fn narrower_box_shapes_again() {
  let block = wrapping(SENTENCE, TextWrapping::Wrap);

  assert!(reshaped_by_second_arrange(
    block,
    Size::new(200.0, 300.0),
    Size::new(100.0, 300.0),
    1.0
  ));
}

// NoWrap text without trimming never shapes again, whatever the box.
#[test]
fn no_wrap_without_trimming_never_shapes_again() {
  let block = text_block(SENTENCE);

  assert!(!reshaped_by_second_arrange(
    block,
    Size::new(400.0, 300.0),
    Size::new(50.0, 10.0),
    1.0
  ));
}

// Without trimming the height can't change the paragraph: a shorter box shapes nothing.
#[test]
fn shorter_box_without_trimming_does_not_shape_again() {
  let block = wrapping(SENTENCE, TextWrapping::Wrap);

  assert!(!reshaped_by_second_arrange(
    block,
    Size::new(100.0, 300.0),
    Size::new(100.0, 10.0),
    1.0
  ));
}

// A wrapped paragraph wider than its box only because of a word that can't break, arranged in the
// width it was shaped for, shapes nothing. The height changes so that arrange runs.
#[test]
fn limited_overflowing_word_does_not_shape_again() {
  let block = wrapping("a Supercalifragilisticexpialidocious b", TextWrapping::WrapWithOverflow);

  assert!(!reshaped_by_second_arrange(
    block,
    Size::new(100.0, 300.0),
    Size::new(100.0, 250.0),
    1.0
  ));
}

// Wrapped text in a wider box than it was measured for shapes again to fill it.
#[test]
fn wider_box_wraps_again() {
  let block = wrapping(SENTENCE, TextWrapping::Wrap);

  assert!(reshaped_by_second_arrange(
    block,
    Size::new(100.0, 300.0),
    Size::new(300.0, 300.0),
    1.0
  ));
}

/// Every wrapping mode.
fn wrappings() -> impl Strategy<Value = TextWrapping> {
  prop_oneof![
    Just(TextWrapping::NoWrap),
    Just(TextWrapping::Wrap),
    Just(TextWrapping::WrapWithOverflow)
  ]
}

/// Every trimming mode.
fn trimmings() -> impl Strategy<Value = TextTrimming> {
  prop_oneof![
    Just(TextTrimming::None),
    Just(TextTrimming::CharacterEllipsis),
    Just(TextTrimming::WordEllipsis),
    Just(TextTrimming::PrefixCharacterEllipsis),
    Just(TextTrimming::LeadingCharacterEllipsis),
    Just(TextTrimming::PathSegmentEllipsis)
  ]
}

proptest! {
  // Layout's invariants hold for random text, sizes, scales and modes. Containment is off: text may
  // overflow its box.
  #[test]
  fn textblock_invariants_hold(
    text in "[a-z /]{0,40}",
    wrapping in wrappings(),
    trimming in trimmings(),
    available in strategy::available(),
    scale in strategy::scale(),
  ) {
    let mut layout = layout();
    let block = text_block(&text);
    block.text_wrapping.set(wrapping);
    block.text_trimming.set(trimming);
    let root = layout.add("text", block);

    Invariants::default().assert(&mut layout, root, available, scale);
  }
}
