// TextBlock shows a string in the font and brush its nearest ancestor sets, wrapped, trimmed and
// aligned within the box layout gives it.

use artic_controls::{StackPanel, TextElement};
use artic_core::tree::UiTree;
use artic_element::Children;
use artic_layout::text::{FontFamily, FontStyle, FontWeight};
use artic_render::{Brush, Color};

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
