//! Checking hand-built documents: what is reported, where, and what the view keeps.

mod common;

use artic_view::document::{Content, Document};
use artic_view::{CheckedContent, check};
use common::*;
use pretty_assertions::assert_eq;

// An unknown property is reported at its name, and its value is never read.
#[test]
fn unknown_property_is_reported_at_its_name() {
  let document = Document {
    root: element(&PANEL, 0, vec![property_problem("Height", 10, "value objects come with #57")]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Panel` has no property `Height`", 10)]);
  assert!(view.elements[0].properties.is_empty());
}

// An event set where a property is names the issue that adds handlers.
#[test]
fn event_names_its_issue() {
  let document = Document {
    root: element(&PANEL, 0, vec![property("Click", 10, "on_click")]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("event handlers come with #82", 10)]);
}

// A property set twice is reported at the second value, pointing at the first; the first stays.
#[test]
fn property_set_twice_points_at_both() {
  let document = Document {
    root: element(&PANEL, 0, vec![property("Width", 10, "1"), property("Width", 20, "2")]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Width` is set twice", 21)]);
  let diagnostic = diagnostics.iter().next().expect("one diagnostic");
  assert_eq!(diagnostic.labels[0].span, span(11, 12));
  assert_eq!(view.elements[0].properties.len(), 1);
  assert_eq!(view.elements[0].properties[0].1.text, "1");
}

// A value's problem is reported once its property is found, and the property isn't set.
#[test]
fn value_problem_is_reported_once_its_property_is_found() {
  let document = Document {
    root: element(&PANEL, 0, vec![property_problem("Width", 10, "`{Binding}` comes with #94")]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`{Binding}` comes with #94", 11)]);
  assert!(view.elements[0].properties.is_empty());
}

// Text inside a type with a content property sets that property, at the content's span.
#[test]
fn content_text_sets_the_content_property() {
  let document = Document {
    root: element(&LABEL, 0, vec![content(20, vec![text("Hello", 21)])]),
  };

  let (view, diagnostics) = check(&document);

  assert!(diagnostics.is_empty(), "{diagnostics}");
  let (property, value) = &view.elements[0].properties[0];
  assert_eq!(property.info.name, "Text");
  assert_eq!((value.text.as_str(), value.span), ("Hello", span(20, 21)));
}

// A type whose whitespace is significant reads the text as written.
#[test]
fn significant_whitespace_reads_the_raw_text() {
  let raw = Content::Text {
    text: "a b".to_owned(),
    raw: " a  b ".to_owned(),
    span: span(21, 22),
  };
  let document = Document {
    root: element(&CODE, 0, vec![content(20, vec![raw])]),
  };

  let (view, _) = check(&document);

  assert_eq!(view.elements[0].properties[0].1.text, " a  b ");
}

// Content text sets the content property as an attribute does, so setting both is "set twice".
#[test]
fn content_text_and_its_attribute_are_set_twice() {
  let document = Document {
    root: element(
      &LABEL,
      0,
      vec![property("Text", 10, "Hi"), content(20, vec![text("Hello", 21)])],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Text` is set twice", 20)]);
  assert_eq!(view.elements[0].properties[0].1.text, "Hi");
}

// An element in a content property's text is reported and ends the content: the element is
// never checked and the text isn't set.
#[test]
fn element_in_content_text_ends_the_content() {
  let nodes = vec![text("a", 21), Content::Element(unknown(22, Vec::new())), text("b", 30)];
  let document = Document {
    root: element(&LABEL, 0, vec![content(20, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("value objects come with #57", 22)]);
  assert!(view.elements[0].properties.is_empty());
}

// Every child element counts toward "takes one child", found or not, and the child taken is the
// only one found.
#[test]
fn one_child_counts_every_element() {
  let nodes = vec![
    Content::Element(unknown(21, Vec::new())),
    Content::Element(element(&IMAGE, 30, Vec::new())),
  ];
  let document = Document {
    root: element(&FRAME, 0, vec![content(20, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [at("no type `Gauge`", 22), at("`Frame` takes one child", 30)]
  );
  assert_eq!(view.elements[1].content, CheckedContent::Child(0));
}

// Text among children is reported with a hint, and blank text is ignored.
#[test]
fn text_among_children_is_reported_with_help() {
  let nodes = vec![blank(20), text("hi", 21), Content::Element(element(&IMAGE, 30, Vec::new()))];
  let document = Document {
    root: element(&PANEL, 0, vec![content(20, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Panel` takes no text content", 21)]);
  let help = diagnostics.iter().next().and_then(|diagnostic| diagnostic.help.clone());
  assert_eq!(help.as_deref(), Some("put the text in an element, such as a `TextBlock`"));
  assert_eq!(view.elements[1].content, CheckedContent::Children(vec![0]));
}

// A type that takes no content reports its text and its elements, and still checks the elements.
#[test]
fn no_content_reports_and_still_checks_elements() {
  let child = element(&PANEL, 30, vec![property("Height", 31, "1")]);
  let document = Document {
    root: element(&IMAGE, 0, vec![content(20, vec![text("x", 21), Content::Element(child)])]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [
      at("`Image` takes no text content", 21),
      at("`Image` takes no child elements", 30),
      at("`Panel` has no property `Height`", 31),
    ]
  );
  assert_eq!(view.elements.len(), 2);
  assert_eq!(view.elements[1].content, CheckedContent::None);
}

// A collection of values reports anything but blank text, and the elements inside are never
// reached; blank text alone reports nothing.
#[test]
fn value_collection_never_reaches_its_elements() {
  let filled = Document {
    root: element(&STOPS, 0, vec![content(20, vec![Content::Element(unknown(21, Vec::new()))])]),
  };
  let blank_only = Document {
    root: element(&STOPS, 0, vec![content(20, vec![blank(20)])]),
  };

  let (_, filled) = check(&filled);
  let (_, blank_only) = check(&blank_only);

  assert_eq!(reported(&filled), [at("value collections come with #57", 20)]);
  assert!(blank_only.is_empty(), "{blank_only}");
}

// A name that isn't a field is reported, with the reason as help, each time it's written.
#[test]
fn invalid_name_is_reported_each_time() {
  let child = element(&IMAGE, 20, vec![name("1x", 25)]);
  let document = Document {
    root: element(&PANEL, 0, vec![name("1x", 5), content(19, vec![Content::Element(child)])]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [at("`1x` isn't a valid name", 5), at("`1x` isn't a valid name", 25)]
  );
  let help = diagnostics.iter().next().and_then(|diagnostic| diagnostic.help.clone());
  assert_eq!(help.as_deref(), Some("a name must be a Rust identifier"));
  assert!(view.names.is_empty());
}

// Two elements with one name are reported at the second, pointing at the first, which keeps it.
#[test]
fn two_elements_with_one_name() {
  let child = element(&IMAGE, 21, vec![name("email", 30)]);
  let document = Document {
    root: element(&PANEL, 0, vec![name("email", 10), content(20, vec![Content::Element(child)])]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`email` names two elements", 30)]);
  let diagnostic = diagnostics.iter().next().expect("one diagnostic");
  assert_eq!(diagnostic.labels[0].span, span(10, 11));
  assert_eq!(view.names.len(), 1);
  assert_eq!((view.names[0].0.text.as_str(), view.names[0].1), ("email", 1));
}

// An element whose type isn't found still has its name and the elements in its content checked,
// but not its properties.
#[test]
fn unknown_type_still_checks_its_name_and_children() {
  let child = element(&PANEL, 21, vec![property("Height", 22, "1")]);
  let document = Document {
    root: unknown(
      0,
      vec![
        name("1x", 5),
        property("Width", 10, "1"),
        content(20, vec![Content::Element(child)]),
      ],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [
      at("no type `Gauge`", 1),
      at("`1x` isn't a valid name", 5),
      at("`Panel` has no property `Height`", 22),
    ]
  );
  assert_eq!(view.root, None);
  assert_eq!(view.elements.len(), 1);
}

// A member the document can't hold is reported whether or not the element's type was found.
#[test]
fn unsupported_member_is_reported_on_any_element() {
  let found = Document {
    root: element(&PANEL, 0, vec![unsupported(10)]),
  };
  let missing = Document {
    root: unknown(0, vec![unsupported(10)]),
  };

  let (_, found) = check(&found);
  let (_, missing) = check(&missing);

  assert_eq!(reported(&found), [at("attached properties come with #50", 10)]);
  assert_eq!(
    reported(&missing),
    [at("no type `Gauge`", 1), at("attached properties come with #50", 10)]
  );
}

// A problem keyed `once` is reported at its first use only.
#[test]
fn once_problem_is_reported_once() {
  let nodes = vec![Content::Element(undeclared(20)), Content::Element(undeclared(30))];
  let document = Document {
    root: element(&PANEL, 0, vec![content(19, nodes)]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("no namespace `https://example.com/ui`", 21)]);
}

// A `once` problem the walk never reaches doesn't spend its key: the next reached use reports it.
#[test]
fn once_key_is_spent_only_where_reached() {
  let stops = element(&STOPS, 20, vec![content(21, vec![Content::Element(undeclared(22))])]);
  let nodes = vec![Content::Element(stops), Content::Element(undeclared(40))];
  let document = Document {
    root: element(&PANEL, 0, vec![content(19, nodes)]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [
      at("value collections come with #57", 21),
      at("no namespace `https://example.com/ui`", 41),
    ]
  );
}

// The view lists children before their parent, the root last, with its span and its names.
#[test]
fn view_lists_children_first() {
  let label = element(&LABEL, 30, vec![content(31, vec![text("Hi", 32)])]);
  let nodes = vec![Content::Element(element(&IMAGE, 20, Vec::new())), Content::Element(label)];
  let document = Document {
    root: element(&PANEL, 0, vec![name("form", 5), content(19, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert!(diagnostics.is_empty(), "{diagnostics}");
  let names: Vec<&str> = view.elements.iter().map(|element| element.ty.info.name).collect();
  assert_eq!(names, ["Image", "Label", "Panel"]);
  assert_eq!(view.root, Some(2));
  assert_eq!(view.span, span(0, 1));
  assert_eq!(view.elements[2].content, CheckedContent::Children(vec![0, 1]));
  assert_eq!((view.names[0].0.text.as_str(), view.names[0].1), ("form", 2));
}
