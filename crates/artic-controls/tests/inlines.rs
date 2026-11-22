// A TextBlock's inlines: Run, Bold and Italic flatten into one styled text, inheriting their font
// down the logical tree, and a change inside one reaches the block.

use artic_controls::{Bold, Inlines, Italic, Run, StackPanel, TextBlock, TextFormatMethods};
use artic_core::property::Affects;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{AccessCx, ElementTypes};
use artic_layout::geometry::Size;
use artic_layout::text::{FontStyle, FontWeight, StyledText};
use artic_registry::{ContentInfo, UiType, ValueError, Whitespace};
use artic_render::{Brush, Color, Command, TextBrushes};
use artic_test::layout::Headless;

const WINDOW: Size = Size::new(400.0, 300.0);

/// A run of `text`, built in `tree`.
fn run(tree: &mut UiTree, text: &str) -> NodeId {
  Run::new().text(text).build(tree).id()
}

/// The text the block's inlines flatten into.
fn flattened(tree: &UiTree, block: NodeId) -> StyledText {
  Inlines::styled_text(tree, block).expect("the block holds inlines")
}

/// Each span of `text`, as its piece of text and its style's weight and slant.
fn pieces(text: &StyledText) -> Vec<(&str, FontWeight, FontStyle)> {
  text
    .spans()
    .map(|(range, style)| {
      let style = &text.styles()[style];
      (&text.text()[range], style.weight, style.style)
    })
    .collect()
}

// `<Italic>Hello <Bold>world</Bold></Italic><Run>!</Run>` flattens into one text whose spans carry
// the style of every inline around them: italic, then italic and bold, then neither.
#[test]
fn inline_runs_flatten_with_styles() {
  let mut tree = UiTree::new();
  let hello = run(&mut tree, "Hello ");
  let world = run(&mut tree, "world");
  let bold = Bold::new().inlines([world]).build(&mut tree);
  let italic = Italic::new().inlines([hello, bold.id()]).build(&mut tree);
  let bang = run(&mut tree, "!");
  let block = TextBlock::new().inlines([italic.id(), bang]).build(&mut tree).id();

  let text = flattened(&tree, block);

  assert_eq!(
    pieces(&text),
    [
      ("Hello ", FontWeight::NORMAL, FontStyle::Italic),
      ("world", FontWeight::BOLD, FontStyle::Italic),
      ("!", FontWeight::NORMAL, FontStyle::Normal),
    ]
  );
}

// Bold beats the weight the block passes down, and a weight set on the run inside it beats Bold.
#[test]
fn the_innermost_weight_wins() {
  let mut tree = UiTree::new();
  let plain = run(&mut tree, "a");
  let bolded = run(&mut tree, "b");
  let black = Run::new().text("c").font_weight(FontWeight::BLACK).build(&mut tree).id();
  let bold = Bold::new().inlines([bolded, black]).build(&mut tree);
  let block = TextBlock::new()
    .font_weight(FontWeight::LIGHT)
    .inlines([plain, bold.id()])
    .build(&mut tree)
    .id();

  let weights: Vec<FontWeight> = pieces(&flattened(&tree, block)).into_iter().map(|piece| piece.1).collect();

  assert_eq!(weights, [FontWeight::LIGHT, FontWeight::BOLD, FontWeight::BLACK]);
}

// `Hello <Bold>world</Bold>` built through the builder keeps the space between the runs, and every
// type inlines go in keeps its text's whitespace.
#[test]
fn inline_whitespace_preserved() {
  let mut tree = UiTree::new();
  let hello = run(&mut tree, "Hello ");
  let world = run(&mut tree, "world");
  let bold = Bold::new().inlines([world]).build(&mut tree);
  let block = TextBlock::new().inlines([hello, bold.id()]).build(&mut tree).id();

  assert_eq!(flattened(&tree, block).text(), "Hello world");
  for info in [TextBlock::TYPE_INFO, Run::TYPE_INFO, Bold::TYPE_INFO, Italic::TYPE_INFO] {
    assert_eq!(info.whitespace, Whitespace::Significant, "{}", info.name);
  }
}

// Inlines win over Text while there are any; Text shows again once the last one leaves.
#[test]
fn inlines_win_over_text_while_present() {
  let mut tree = UiTree::new();
  let inline = run(&mut tree, "inline");
  let block = TextBlock::new().text("own").inlines([inline]).build(&mut tree).id();

  assert_eq!(flattened(&tree, block).text(), "inline");
  tree.detach(TreeKind::Logical, inline);
  assert_eq!(Inlines::styled_text(&tree, block), None);
}

/// A block in a window-sized layout holding `inlines`, laid out once.
fn laid_out(layout: &mut Headless, inlines: &[NodeId]) -> NodeId {
  let block = TextBlock::new().inlines(inlines.to_vec()).build(layout.tree()).id();
  layout.run(block, WINDOW, 1.0);
  block
}

// Changing a run's text measures its block again.
#[test]
fn run_change_invalidates_measure() {
  let mut layout = Headless::new();
  let inline = run(layout.tree(), "Hi");
  let block = laid_out(&mut layout, &[inline]);
  let before = layout.manager().desired(block).expect("measured");

  let run = layout.tree().get_as::<Run>(inline).expect("a run");
  run.text.set("Hi there, a longer line".to_owned());
  let stats = layout.run(block, WINDOW, 1.0);

  assert_eq!(stats.measured, 1);
  assert!(layout.manager().desired(block).expect("measured").width > before.width);
}

/// The brushes the block draws its text with.
fn brushes(layout: &mut Headless, block: NodeId) -> TextBrushes {
  layout
    .record(block)
    .into_iter()
    .find_map(|command| match command {
      Command::Text { brushes, .. } => Some(brushes),
      _ => None,
    })
    .expect("a text command")
}

// Each run is painted with its own brush, and changing one redraws the block without measuring it.
#[test]
fn each_run_paints_with_its_foreground() {
  let mut layout = Headless::new();
  let red = Brush::from(Color::rgb(255, 0, 0));
  let black = Brush::from(Color::rgb(0, 0, 0));
  let plain = run(layout.tree(), "plain ");
  let colored = Run::new().text("red").foreground(red.clone()).build(layout.tree()).id();
  let block = laid_out(&mut layout, &[plain, colored]);
  assert_eq!(brushes(&mut layout, block), TextBrushes::from(vec![black.clone(), red]));
  layout.tree().clear_dirty(block, Affects::RENDER);

  let blue = Brush::from(Color::rgb(0, 0, 255));
  let run = layout.tree().get_as::<Run>(colored).expect("a run");
  run.format.foreground.set(blue.clone());
  let needs_drawing = layout.tree().dirty(block).contains(Affects::RENDER);
  let stats = layout.run(block, WINDOW, 1.0);

  assert!(needs_drawing);
  assert_eq!(stats.measured, 0);
  assert_eq!(brushes(&mut layout, block), TextBrushes::from(vec![black, blue]));
}

// The accessibility value is the runs' text, joined in reading order.
#[test]
fn textblock_inlines_accesskit_value() {
  let mut tree = UiTree::new();
  let hello = run(&mut tree, "Hello ");
  let world = run(&mut tree, "world");
  let bold = Bold::new().inlines([world]).build(&mut tree);
  let block = TextBlock::new()
    .text("ignored")
    .inlines([hello, bold.id()])
    .build(&mut tree)
    .id();
  let types = ElementTypes::from_link_time();
  let element = types.element(tree.get(block).expect("a node")).expect("an element");

  let node = element.accessibility(&AccessCx::new(&tree, block, Size::new(10.0, 10.0)));

  assert_eq!(node.value(), Some("Hello world"));
}

// A node that isn't an inline is refused, and the block's children don't change.
#[test]
fn a_non_inline_child_is_refused() {
  let mut tree = UiTree::new();
  let block = TextBlock::new().build(&mut tree).id();
  let panel = tree.alloc(Box::new(StackPanel::default()));
  let Some(ContentInfo::Collection { add_child, .. }) = TextBlock::TYPE_INFO.content else {
    panic!("TextBlock's content is a collection");
  };

  assert_eq!(
    add_child(&mut tree, block, panel),
    Err(ValueError::UnexpectedChild {
      expected: "`Run`, `Bold` or `Italic`".to_owned()
    })
  );
  assert_eq!(tree.children(TreeKind::Logical, block).count(), 0);
}

// Building a block with a child of another type panics before anything is allocated.
#[test]
fn building_with_a_non_inline_child_leaves_the_tree_as_it_was() {
  let mut tree = UiTree::new();
  let panel = tree.alloc(Box::new(StackPanel::default()));
  let before = tree.len();

  let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    let _ = TextBlock::new().inlines([panel]).build(&mut tree);
  }));

  assert!(built.is_err());
  assert_eq!(tree.len(), before);
}

// A node that isn't in the tree is refused by the link, not by its type.
#[test]
fn a_stale_child_is_refused_by_the_link() {
  let mut tree = UiTree::new();
  let block = TextBlock::new().build(&mut tree).id();
  let stale = run(&mut tree, "gone");
  tree.free(stale);
  let Some(ContentInfo::Collection { add_child, .. }) = TextBlock::TYPE_INFO.content else {
    panic!("TextBlock's content is a collection");
  };

  assert!(matches!(add_child(&mut tree, block, stale), Err(ValueError::InvalidChild(_))));
}

/// The accessibility value of the block at `node`.
fn value(tree: &UiTree, node: NodeId) -> Option<String> {
  let types = ElementTypes::from_link_time();
  let element = types.element(tree.get(node).expect("a node")).expect("an element");
  element
    .accessibility(&AccessCx::new(tree, node, Size::new(10.0, 10.0)))
    .value()
    .map(str::to_owned)
}

// Once the last inline leaves, the block measures and reads `Text` again.
#[test]
fn text_shows_again_once_the_last_inline_leaves() {
  let mut layout = Headless::new();
  let inline = run(layout.tree(), "a much longer inline");
  let block = TextBlock::new().text("own").inlines([inline]).build(layout.tree()).id();
  layout.run(block, WINDOW, 1.0);
  let with_inline = layout.manager().desired(block).expect("measured");

  layout.tree().detach(TreeKind::Logical, inline);
  let stats = layout.run(block, WINDOW, 1.0);

  assert_eq!(stats.measured, 1);
  assert!(layout.manager().desired(block).expect("measured").width < with_inline.width);
  assert_eq!(value(layout.tree(), block).as_deref(), Some("own"));
}

// Inlines with no run in them keep the block one line high, as empty text does.
#[test]
fn inlines_without_runs_keep_one_line() {
  let mut layout = Headless::new();
  let empty = laid_out(&mut layout, &[]);
  let one_line = layout.manager().desired(empty).expect("measured");
  let bold = Bold::new().build(layout.tree()).id();
  let block = laid_out(&mut layout, &[bold]);

  let size = layout.manager().desired(block).expect("measured");

  assert!(size.height > 0.0);
  assert_eq!(size, one_line);
}

// A weight set on Bold itself wins over its type value, and measures the block again.
#[test]
fn bold_own_weight_wins_and_remeasures() {
  let mut layout = Headless::new();
  let inner = run(layout.tree(), "inside");
  let bold = Bold::new().inlines([inner]).build(layout.tree()).id();
  let block = laid_out(&mut layout, &[bold]);

  let bold_node = layout.tree().get_as::<Bold>(bold).expect("a bold");
  bold_node.format.font_weight.set(FontWeight::BLACK);
  let stats = layout.run(block, WINDOW, 1.0);

  assert_eq!(stats.measured, 1);
  assert_eq!(pieces(&flattened(layout.tree(), block))[0].1, FontWeight::BLACK);
}
