//! Checking hand-built documents: what is reported, where, and what the view keeps.

mod common;

use artic_diagnostics::Diagnostic;
use artic_view::document::{Content, Document, Member, Text, TypeUse};
use artic_view::testing::*;
use artic_view::{CheckedContent, FieldVisibility, check};
use common::*;
use pretty_assertions::assert_eq;

// An unknown property is reported at its name, and its value is never read.
#[test]
fn unknown_property_is_reported_at_its_name() {
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![property_problem(file(), "Height", 10, "value objects come with #438")],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Panel` has no property `Height`", 10)]);
  assert!(view.elements[0].properties.is_empty());
}

// A value written where an element of the tree goes, as a child or as the root, is reported once,
// at its element, kept out of the view, so nothing is generated for it, and left out of its
// container's "takes one child" count.
#[test]
fn value_is_reported_where_an_element_goes() {
  let as_child = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![content(
        file(),
        5,
        vec![Content::Element(element(file(), &TINT, 10, Vec::new()))],
      )],
    ),
  };
  let as_root = Document {
    root: element(file(), &TINT, 0, Vec::new()),
  };

  let in_a_list = Document {
    root: element(
      file(),
      &GALLERY,
      0,
      vec![content(
        file(),
        5,
        vec![Content::Element(element(file(), &TINT, 10, Vec::new()))],
      )],
    ),
  };
  let in_no_content = Document {
    root: element(
      file(),
      &IMAGE,
      0,
      vec![content(
        file(),
        5,
        vec![Content::Element(element(file(), &TINT, 10, Vec::new()))],
      )],
    ),
  };

  let beside_a_child = |nodes| Document {
    root: element(file(), &FRAME, 0, vec![content(file(), 5, nodes)]),
  };
  let value_second = beside_a_child(vec![
    Content::Element(element(file(), &PANEL, 10, Vec::new())),
    Content::Element(element(file(), &TINT, 20, Vec::new())),
  ]);
  let value_first = beside_a_child(vec![
    Content::Element(element(file(), &TINT, 10, Vec::new())),
    Content::Element(element(file(), &PANEL, 20, Vec::new())),
  ]);

  let (child_view, child_diagnostics) = check(&as_child);
  let (_, root_diagnostics) = check(&as_root);

  let refused = |at_offset| at("`Tint` is a value, not an element of the tree", at_offset);
  assert_eq!(reported(&child_diagnostics), [refused(10)]);
  assert_eq!(child_view.elements.len(), 1);
  assert_eq!(reported(&root_diagnostics), [refused(0)]);
  assert_eq!(reported(&check(&in_a_list).1), [refused(10)]);
  assert_eq!(reported(&check(&in_no_content).1), [refused(10)]);
  assert_eq!(reported(&check(&value_second).1), [refused(20)]);
  assert_eq!(reported(&check(&value_first).1), [refused(10)]);
}

// A type with no way to be created from markup is reported at its element, and its members and
// children are still checked.
#[test]
fn type_without_new_is_reported_at_its_element() {
  let login = element(file(), &LOGIN, 10, vec![property(file(), "Width", 20, "4")]);
  let document = Document {
    root: element(file(), &PANEL, 0, vec![content(file(), 5, vec![Content::Element(login)])]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [
      at("`Login` has no property `Width`", 20),
      at("`Login` can't be created from markup yet (#59)", 10)
    ]
  );
}

// An event set where a property is names the issue that adds handlers.
#[test]
fn event_names_its_issue() {
  let document = Document {
    root: element(file(), &PANEL, 0, vec![property(file(), "Click", 10, "on_click")]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("event handlers come with #82", 10)]);
}

// A property set twice is reported at the second value, pointing at the first; the first stays.
#[test]
fn property_set_twice_points_at_both() {
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![property(file(), "Width", 10, "1"), property(file(), "Width", 20, "2")],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Width` is set twice", 21)]);
  let diagnostic = diagnostics.iter().next().expect("one diagnostic");
  assert_eq!(diagnostic.labels[0].span, span(file(), 11, 12));
  assert_eq!(view.elements[0].properties.len(), 1);
  assert_eq!(view.elements[0].properties[0].1.text, "1");
}

// A value's problem is reported once its property is found, and the property isn't set.
#[test]
fn value_problem_is_reported_once_its_property_is_found() {
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![property_problem(file(), "Width", 10, "`{Binding}` comes with #94")],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`{Binding}` comes with #94", 11)]);
  assert!(view.elements[0].properties.is_empty());
}

// Text inside a type with a content property sets that property, at the content's span.
#[test]
fn content_text_sets_the_content_property() {
  let document = Document {
    root: element(file(), &LABEL, 0, vec![content(file(), 20, vec![text(file(), "Hello", 21)])]),
  };

  let (view, diagnostics) = check(&document);

  assert!(diagnostics.is_empty(), "{diagnostics}");
  let (property, value) = &view.elements[0].properties[0];
  assert_eq!(property.info.name, "Text");
  assert_eq!((value.text.as_str(), value.span), ("Hello", span(file(), 20, 21)));
}

// A content property reads the text after the language's rules, whitespace significant or not:
// only a rule of the front end's own keeps it as written.
#[test]
fn content_text_reads_the_ruled_text() {
  let raw = Content::Text {
    text: "a b".to_owned(),
    raw: " a  b ".to_owned(),
    span: span(file(), 21, 22),
  };
  let document = Document {
    root: element(file(), &CODE, 0, vec![content(file(), 20, vec![raw])]),
  };

  let (view, _) = check(&document);

  assert_eq!(view.elements[0].properties[0].1.text, "a b");
}

// Content text sets the content property as an attribute does, so setting both is "set twice".
#[test]
fn content_text_and_its_attribute_are_set_twice() {
  let document = Document {
    root: element(
      file(),
      &LABEL,
      0,
      vec![
        property(file(), "Text", 10, "Hi"),
        content(file(), 20, vec![text(file(), "Hello", 21)]),
      ],
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
  let nodes = vec![
    text(file(), "a", 21),
    Content::Element(unknown(file(), 22, Vec::new())),
    text(file(), "b", 30),
  ];
  let document = Document {
    root: element(file(), &LABEL, 0, vec![content(file(), 20, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("value objects come with #438", 22)]);
  assert!(view.elements[0].properties.is_empty());
}

// Every child element but a value counts toward "takes one child", found or not, and the child
// taken is the only one found.
#[test]
fn one_child_counts_every_element() {
  let nodes = vec![
    Content::Element(unknown(file(), 21, Vec::new())),
    Content::Element(element(file(), &IMAGE, 30, Vec::new())),
  ];
  let document = Document {
    root: element(file(), &FRAME, 0, vec![content(file(), 20, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [at("no type `Gauge`", 22), at("`Frame` takes one child", 30)]
  );
  assert_eq!(view.elements[1].content, CheckedContent::Child(0));
}

// Two found children of a one-child element are reported, and the element keeps neither.
#[test]
fn one_child_with_two_keeps_none() {
  let nodes = vec![
    Content::Element(element(file(), &IMAGE, 21, Vec::new())),
    Content::Element(element(file(), &IMAGE, 30, Vec::new())),
  ];
  let document = Document {
    root: element(file(), &FRAME, 0, vec![content(file(), 20, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Frame` takes one child", 30)]);
  assert_eq!(view.elements[2].content, CheckedContent::None);
}

// Text among children is reported with a hint, and blank text is ignored.
#[test]
fn text_among_children_is_reported_with_help() {
  let nodes = vec![
    blank(file(), 20),
    text(file(), "hi", 21),
    Content::Element(element(file(), &IMAGE, 30, Vec::new())),
  ];
  let document = Document {
    root: element(file(), &PANEL, 0, vec![content(file(), 20, nodes)]),
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
  let child = element(file(), &PANEL, 30, vec![property(file(), "Height", 31, "1")]);
  let document = Document {
    root: element(
      file(),
      &IMAGE,
      0,
      vec![content(file(), 20, vec![text(file(), "x", 21), Content::Element(child)])],
    ),
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
    root: element(
      file(),
      &STOPS,
      0,
      vec![content(file(), 20, vec![Content::Element(unknown(file(), 21, Vec::new()))])],
    ),
  };
  let blank_only = Document {
    root: element(file(), &STOPS, 0, vec![content(file(), 20, vec![blank(file(), 20)])]),
  };

  let (_, filled) = check(&filled);
  let (_, blank_only) = check(&blank_only);

  assert_eq!(reported(&filled), [at("value collections come with #438", 20)]);
  assert!(blank_only.is_empty(), "{blank_only}");
}

// A name that isn't a field is reported, with the reason as help, each time it's written.
#[test]
fn invalid_name_is_reported_each_time() {
  let child = element(file(), &IMAGE, 20, vec![name(file(), "1x", 25)]);
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![name(file(), "1x", 5), content(file(), 19, vec![Content::Element(child)])],
    ),
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
  let child = element(file(), &IMAGE, 21, vec![name(file(), "email", 30)]);
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![name(file(), "email", 10), content(file(), 20, vec![Content::Element(child)])],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`email` names two elements", 30)]);
  let diagnostic = diagnostics.iter().next().expect("one diagnostic");
  assert_eq!(diagnostic.labels[0].span, span(file(), 10, 11));
  assert_eq!(diagnostic.help, None);
  assert_eq!(view.names.len(), 1);
  assert_eq!((view.names[0].name.text.as_str(), view.names[0].element), ("email", 1));
}

// Two names with one snake_case field name two elements, and the help names both spellings.
#[test]
fn two_spellings_of_one_field() {
  let child = element(file(), &IMAGE, 21, vec![name(file(), "Email", 30)]);
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![name(file(), "email", 10), content(file(), 20, vec![Content::Element(child)])],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Email` names two elements", 30)]);
  let diagnostic = diagnostics.iter().next().expect("one diagnostic");
  assert_eq!(
    diagnostic.help.as_deref(),
    Some("`email` and `Email` both give the field `email`")
  );
  assert_eq!(view.names.len(), 1);
}

// Each checked name carries the field it gives, in the order the elements are built.
#[test]
fn checked_name_carries_its_field() {
  let child = element(file(), &IMAGE, 21, vec![name(file(), "EmailAddress", 30)]);
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![name(file(), "Type", 10), content(file(), 20, vec![Content::Element(child)])],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert!(diagnostics.is_empty());
  let names: Vec<(&str, &str, usize)> = view
    .names
    .iter()
    .map(|name| (name.name.text.as_str(), name.field.as_str(), name.element))
    .collect();
  assert_eq!(names, [("EmailAddress", "email_address", 0), ("Type", "r#type", 1)]);
}

// An element whose type isn't found still has its name and the elements in its content checked,
// but not its properties.
#[test]
fn unknown_type_still_checks_its_name_and_children() {
  let child = element(file(), &PANEL, 21, vec![property(file(), "Height", 22, "1")]);
  let document = Document {
    root: unknown(
      file(),
      0,
      vec![
        name(file(), "1x", 5),
        property(file(), "Width", 10, "1"),
        content(file(), 20, vec![Content::Element(child)]),
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
    root: element(file(), &PANEL, 0, vec![unsupported(file(), 10)]),
  };
  let missing = Document {
    root: unknown(file(), 0, vec![unsupported(file(), 10)]),
  };

  let (_, found) = check(&found);
  let (_, missing) = check(&missing);

  assert_eq!(reported(&found), [at("resource keys come with #111", 10)]);
  assert_eq!(
    reported(&missing),
    [at("no type `Gauge`", 1), at("resource keys come with #111", 10)]
  );
}

// A problem keyed `once` is reported at its first use only.
#[test]
fn once_problem_is_reported_once() {
  let nodes = vec![
    Content::Element(undeclared(file(), 20)),
    Content::Element(undeclared(file(), 30)),
  ];
  let document = Document {
    root: element(file(), &PANEL, 0, vec![content(file(), 19, nodes)]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("no namespace `https://example.com/ui`", 21)]);
}

// A `once` problem the walk never reaches doesn't spend its key: the next reached use reports it.
#[test]
fn once_key_is_spent_only_where_reached() {
  let stops = element(
    file(),
    &STOPS,
    20,
    vec![content(file(), 21, vec![Content::Element(undeclared(file(), 22))])],
  );
  let nodes = vec![Content::Element(stops), Content::Element(undeclared(file(), 40))];
  let document = Document {
    root: element(file(), &PANEL, 0, vec![content(file(), 19, nodes)]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [
      at("value collections come with #438", 21),
      at("no namespace `https://example.com/ui`", 41),
    ]
  );
}

// The view lists children before their parent, the root last, with its span and its names.
#[test]
fn view_lists_children_first() {
  let label = element(file(), &LABEL, 30, vec![content(file(), 31, vec![text(file(), "Hi", 32)])]);
  let nodes = vec![
    Content::Element(element(file(), &IMAGE, 20, Vec::new())),
    Content::Element(label),
  ];
  let document = Document {
    root: element(file(), &PANEL, 0, vec![name(file(), "form", 5), content(file(), 19, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert!(diagnostics.is_empty(), "{diagnostics}");
  let names: Vec<&str> = view.elements.iter().map(|element| element.ty.info.name).collect();
  assert_eq!(names, ["Image", "Label", "Panel"]);
  assert_eq!(view.root, Some(2));
  assert_eq!(view.span, span(file(), 0, 1));
  assert_eq!(view.elements[2].content, CheckedContent::Children(vec![0, 1]));
  assert_eq!((view.names[0].name.text.as_str(), view.names[0].element), ("form", 2));
}

// The code type written on the root is the view's.
#[test]
fn code_type_on_the_root_is_kept() {
  let document = Document {
    root: element(file(), &HOST, 0, vec![code_type(file(), "app::views::Login", 5)]),
  };

  let (view, diagnostics) = check(&document);

  assert!(diagnostics.is_empty());
  let code_type = view.code_type.expect("a code type");
  assert_eq!(
    (code_type.text.as_str(), code_type.span),
    ("app::views::Login", span(file(), 5, 6))
  );
}

// A view without a code type has none.
#[test]
fn a_view_without_a_code_type_has_none() {
  let document = Document {
    root: element(file(), &HOST, 0, Vec::new()),
  };

  let (view, diagnostics) = check(&document);

  assert!(diagnostics.is_empty());
  assert_eq!(view.code_type, None);
}

// A code type written below the root is reported there and isn't the view's.
#[test]
fn code_type_below_the_root_is_reported() {
  let child = element(file(), &HOST, 21, vec![code_type(file(), "app::views::Login", 25)]);
  let document = Document {
    root: element(file(), &PANEL, 0, vec![content(file(), 20, vec![Content::Element(child)])]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [at("only the root element can name the view's code type", 25)]
  );
  assert_eq!(view.code_type, None);
}

// A code type named twice is reported at the second, pointing at the first, which stays.
#[test]
fn code_type_named_twice_points_at_both() {
  let document = Document {
    root: element(
      file(),
      &HOST,
      0,
      vec![code_type(file(), "app::One", 5), code_type(file(), "app::Two", 9)],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("the code type is named twice", 9)]);
  let diagnostic = diagnostics.iter().next().expect("one diagnostic");
  assert_eq!(diagnostic.labels[0].span, span(file(), 5, 6));
  assert_eq!(view.code_type.map(|code_type| code_type.text).as_deref(), Some("app::One"));
}

// A code type takes its root's content as a `Child` or `Children` field, so a root whose content
// is a property, or a collection no builder method fills, is reported at the root, with that help.
#[test]
fn code_type_root_content_is_a_child_or_children() {
  for (root, name) in [(&CAPTION, "Caption"), (&BAG, "Bag")] {
    let document = Document {
      root: element(file(), root, 0, vec![code_type(file(), "app::views::Login", 5)]),
    };

    let (_, diagnostics) = check(&document);

    assert_eq!(
      reported(&diagnostics),
      [at(
        &format!("`{name}` can't be the root of a view with a code type: its content isn't a child or children"),
        0
      )]
    );
    let help = diagnostics.iter().next().and_then(|diagnostic| diagnostic.help.clone());
    assert_eq!(
      help.as_deref(),
      Some("a code type takes its root's content as a `Child` or `Children` field")
    );
  }
}

// A root that takes one child can be a code type's root.
#[test]
fn code_type_root_with_a_child_is_accepted() {
  let document = Document {
    root: element(file(), &HOST, 0, vec![code_type(file(), "app::views::Login", 5)]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), []);
}

// A root whose type a code type can't take the place of is reported at the root.
#[test]
fn root_type_must_allow_a_code_type() {
  let document = Document {
    root: element(file(), &PANEL, 0, vec![code_type(file(), "app::views::Login", 5)]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [at("`Panel` can't be the root of a view with a code type", 0)]
  );
}

// A root whose type wasn't found reports only that, not that it can't take a code type.
#[test]
fn unknown_root_reports_only_its_type() {
  let document = Document {
    root: unknown(file(), 0, vec![code_type(file(), "app::views::Login", 5)]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("no type `Gauge`", 1)]);
}

// The root of a view with a code type can't be named, written before the code type or after,
// and its name gives no field.
#[test]
fn named_code_type_root_is_reported() {
  for members in [
    vec![name(file(), "login", 5), code_type(file(), "app::views::Login", 9)],
    vec![code_type(file(), "app::views::Login", 9), name(file(), "login", 5)],
  ] {
    let document = Document {
      root: element(file(), &HOST, 0, members),
    };

    let (view, diagnostics) = check(&document);

    assert_eq!(
      reported(&diagnostics),
      [at("the root of a view with a code type can't be named", 5)]
    );
    assert!(view.names.is_empty());
  }
}

// A named root of a view with a code type is reported once, and its name stays free for a child,
// whichever member comes first.
#[test]
fn root_name_leaves_a_child_its_name() {
  for members in [
    [name(file(), "login", 5), code_type(file(), "app::views::Login", 9)],
    [code_type(file(), "app::views::Login", 9), name(file(), "login", 5)],
  ] {
    let child = element(file(), &IMAGE, 30, vec![name(file(), "login", 31)]);
    let mut members = members.to_vec();
    members.push(content(file(), 19, vec![Content::Element(child)]));
    let document = Document {
      root: element(file(), &HOST, 0, members),
    };

    let (view, diagnostics) = check(&document);

    assert_eq!(
      reported(&diagnostics),
      [at("the root of a view with a code type can't be named", 5)]
    );
    let names: Vec<(&str, usize)> = view.names.iter().map(|name| (name.field.as_str(), name.element)).collect();
    assert_eq!(names, [("login", 0)]);
  }
}

// A field visibility of `pub` or `pub(crate)` is kept with its name; a name without one has none.
#[test]
fn field_visibility_is_kept_with_its_name() {
  let children = [
    element(file(), &IMAGE, 20, vec![name(file(), "a", 21), visibility(file(), "pub", 22)]),
    element(
      file(),
      &IMAGE,
      30,
      vec![visibility(file(), "pub(crate)", 31), name(file(), "b", 32)],
    ),
    element(file(), &IMAGE, 40, vec![name(file(), "c", 41)]),
  ];
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![content(file(), 10, children.map(Content::Element).into())],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert!(diagnostics.is_empty());
  let visibilities: Vec<(&str, Option<FieldVisibility>)> =
    view.names.iter().map(|name| (name.field.as_str(), name.visibility)).collect();
  assert_eq!(
    visibilities,
    [
      ("a", Some(FieldVisibility::Public)),
      ("b", Some(FieldVisibility::Crate)),
      ("c", None)
    ]
  );
}

// Any other visibility is reported, with what to write, and the name stays without one.
#[test]
fn other_field_visibility_is_reported() {
  for text in ["pub(super)", "private", " pub", "pub(crate) ", ""] {
    let document = Document {
      root: element(file(), &PANEL, 0, vec![name(file(), "panel", 5), visibility(file(), text, 9)]),
    };

    let (view, diagnostics) = check(&document);

    assert_eq!(
      reported(&diagnostics),
      [at(&format!("`{text}` isn't a field visibility"), 9)],
      "{text:?}"
    );
    let help = diagnostics.iter().next().and_then(|diagnostic| diagnostic.help.clone());
    assert_eq!(
      help.as_deref(),
      Some("write `pub` or `pub(crate)`, or nothing for the back end's default")
    );
    assert_eq!(view.names[0].visibility, None);
  }
}

// A field visibility on an element without a name is reported; one beside an invalid name isn't,
// since the name is reported already.
#[test]
fn field_visibility_needs_a_name() {
  let unnamed = Document {
    root: element(file(), &PANEL, 0, vec![visibility(file(), "pub", 9)]),
  };
  let misnamed = Document {
    root: element(file(), &PANEL, 0, vec![name(file(), "1x", 5), visibility(file(), "pub", 9)]),
  };

  let (_, unnamed) = check(&unnamed);
  let (_, misnamed) = check(&misnamed);

  assert_eq!(reported(&unnamed), [at("a field visibility needs a name on its element", 9)]);
  assert_eq!(reported(&misnamed), [at("`1x` isn't a valid name", 5)]);
}

// A code type's or a visibility's problem from the front end is reported where the checker
// reaches it.
#[test]
fn front_end_problems_are_reported() {
  let document = Document {
    root: element(
      file(),
      &HOST,
      0,
      vec![
        Member::CodeType(Err(Diagnostic::error(span(file(), 5, 6), "takes text").into())),
        name(file(), "host", 7),
        Member::FieldVisibility(Err(Diagnostic::error(span(file(), 9, 10), "takes text").into())),
      ],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("takes text", 5), at("takes text", 9)]);
  assert_eq!(view.code_type, None);
  assert_eq!(view.names[0].visibility, None);
}

// Text in a collection with a text wrapper becomes wrapper elements beside the children, and in a
// significant type blank text between two elements counts as one space; blank text at the edges
// adds nothing.
#[test]
fn text_becomes_wrapper_elements() {
  let image = || Content::Element(element(file(), &IMAGE, 30, Vec::new()));
  let nodes = vec![
    blank(file(), 10),
    text(file(), "Hello ", 20),
    image(),
    blank(file(), 40),
    image(),
    blank(file(), 50),
  ];
  let document = Document {
    root: element(file(), &PARAGRAPH, 0, vec![content(file(), 5, nodes)]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), []);
  assert_eq!(view.elements[4].content, CheckedContent::Children(vec![0, 1, 2, 3]));
  let pieces: Vec<Option<&str>> = view
    .elements
    .iter()
    .map(|element| element.properties.first().map(|(_, value)| value.text.as_str()))
    .collect();
  assert_eq!(pieces, [Some("Hello "), None, Some(" "), None, None]);
  assert_eq!(view.elements[0].ty.info.name, "Code");
}

// Text written as nothing between two elements adds no space, even in a significant type.
#[test]
fn empty_text_between_elements_adds_nothing() {
  let image = || Content::Element(element(file(), &IMAGE, 30, Vec::new()));
  let document = Document {
    root: element(
      file(),
      &PARAGRAPH,
      0,
      vec![content(file(), 5, vec![image(), text(file(), "", 40), image()])],
    ),
  };

  let (view, _) = check(&document);

  assert_eq!(view.elements[2].content, CheckedContent::Children(vec![0, 1]));
}

// Blank text between two elements adds nothing in a type whose whitespace collapses.
#[test]
fn blank_text_between_elements_adds_nothing_when_it_collapses() {
  let image = || Content::Element(element(file(), &IMAGE, 30, Vec::new()));
  let document = Document {
    root: element(
      file(),
      &PANEL,
      0,
      vec![content(file(), 5, vec![image(), blank(file(), 40), image()])],
    ),
  };

  let (view, _) = check(&document);

  assert_eq!(view.elements[2].content, CheckedContent::Children(vec![0, 1]));
}

// A text wrapper whose content isn't a property can't take the text, which is reported.
#[test]
fn a_wrapper_without_a_content_property_reports_the_text() {
  let document = Document {
    root: element(file(), &SHELF, 0, vec![content(file(), 5, vec![text(file(), "Hi", 20)])]),
  };

  let (_, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("`Shelf` takes no text content", 20)]);
}

// A wrapper markup can't create, or one the collection doesn't take, can't take the text either.
#[test]
fn a_wrapper_the_collection_cannot_use_reports_the_text() {
  for (collection, name) in [(&PINBOARD, "Pinboard"), (&GALLERY, "Gallery")] {
    let document = Document {
      root: element(file(), collection, 0, vec![content(file(), 5, vec![text(file(), "Hi", 20)])]),
    };

    let (_, diagnostics) = check(&document);

    assert_eq!(reported(&diagnostics), [at(&format!("`{name}` takes no text content"), 20)]);
  }
}

// Blank text before the first element and after the last adds nothing, even where whitespace is
// significant: only blank text with an element on each side counts as a space.
#[test]
fn blank_text_at_the_edges_adds_nothing() {
  let image = || Content::Element(element(file(), &IMAGE, 30, Vec::new()));
  let document = Document {
    root: element(
      file(),
      &PARAGRAPH,
      0,
      vec![content(
        file(),
        5,
        vec![blank(file(), 10), image(), text(file(), "x", 40), image(), blank(file(), 50)],
      )],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), []);
  assert_eq!(view.elements[3].content, CheckedContent::Children(vec![0, 1, 2]));
  assert_eq!(view.elements[1].properties[0].1.text, "x");
}

// Blank text between two elements of a significant type with no usable wrapper adds nothing and
// isn't reported: there is no text to report.
#[test]
fn blank_text_without_a_wrapper_is_not_reported() {
  let image = || Content::Element(element(file(), &IMAGE, 30, Vec::new()));
  let document = Document {
    root: element(
      file(),
      &SHELF,
      0,
      vec![content(file(), 5, vec![image(), blank(file(), 40), image()])],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), []);
  assert_eq!(view.elements[2].content, CheckedContent::Children(vec![0, 1]));
}

// An attached property for children of its owner is kept on a child of one, and reported on the
// root or on a child of another type; one for any element is kept on the root.
#[test]
fn attached_target_follows_the_parent() {
  let inside = element(file(), &IMAGE, 20, vec![attached(file(), &RACK, "Slot", 30, "1")]);
  let outside = element(file(), &IMAGE, 50, vec![attached(file(), &RACK, "Slot", 60, "2")]);
  let panel = element(file(), &PANEL, 40, vec![content(file(), 45, vec![Content::Element(outside)])]);
  let document = Document {
    root: element(
      file(),
      &RACK,
      0,
      vec![
        attached(file(), &RACK, "Tag", 5, "top"),
        attached(file(), &RACK, "Slot", 8, "0"),
        content(file(), 10, vec![Content::Element(inside), Content::Element(panel)]),
      ],
    ),
  };

  let (view, diagnostics) = check(&document);

  let misplaced = "`Rack.Slot` can only be set on a child of a `Rack`";
  assert_eq!(reported(&diagnostics), [at(misplaced, 8), at(misplaced, 60)]);
  assert_eq!(view.elements[0].attached[0].value.text, "1");
  let root = view.elements.last().expect("the root");
  assert_eq!(root.attached.len(), 1);
  assert_eq!(root.attached[0].info.name, "Tag");
}

// Inside an element whose type wasn't found, the target rule can't be checked, so the property
// is kept.
#[test]
fn attached_target_inside_an_unknown_type_is_kept() {
  let child = element(file(), &IMAGE, 20, vec![attached(file(), &RACK, "Slot", 30, "1")]);
  let document = Document {
    root: unknown(file(), 0, vec![content(file(), 10, vec![Content::Element(child)])]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), [at("no type `Gauge`", 1)]);
  assert_eq!(view.elements[0].attached.len(), 1);
}

// An owner without the property, an owner that wasn't found, a value with a problem and a
// property set twice are each reported, and nothing is kept for them.
#[test]
fn attached_problems_are_reported() {
  let missing_owner = Member::Attached {
    owner: Err(Diagnostic::error(span(file(), 20, 21), "no type `Dock`").into()),
    name: "Side".to_owned(),
    span: span(file(), 20, 21),
    value: Ok(Text {
      text: "Left".to_owned(),
      span: span(file(), 21, 22),
    }),
  };
  let bad_value = Member::Attached {
    owner: Ok(TypeUse {
      info: &RACK,
      module: None,
      span: span(file(), 30, 31),
    }),
    name: "Tag".to_owned(),
    span: span(file(), 30, 31),
    value: Err(Diagnostic::error(span(file(), 31, 32), "value objects come with #438").into()),
  };
  let document = Document {
    root: element(
      file(),
      &IMAGE,
      0,
      vec![
        attached(file(), &RACK, "Bogus", 10, "1"),
        missing_owner,
        bad_value,
        attached(file(), &RACK, "Tag", 40, "a"),
        attached(file(), &RACK, "Tag", 50, "b"),
      ],
    ),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(
    reported(&diagnostics),
    [
      at("`Rack` has no attached property `Bogus`", 10),
      at("no type `Dock`", 20),
      at("value objects come with #438", 31),
      at("`Rack.Tag` is set twice", 51),
    ]
  );
  assert_eq!(view.elements[0].attached.len(), 1);
}

// Two different attached properties of one owner on one element are both kept: only the same
// property twice is "set twice".
#[test]
fn two_attached_properties_of_one_owner_are_both_kept() {
  let child = element(
    file(),
    &IMAGE,
    20,
    vec![
      attached(file(), &RACK, "Slot", 30, "1"),
      attached(file(), &RACK, "Tag", 40, "a"),
    ],
  );
  let document = Document {
    root: element(file(), &RACK, 0, vec![content(file(), 10, vec![Content::Element(child)])]),
  };

  let (view, diagnostics) = check(&document);

  assert_eq!(reported(&diagnostics), []);
  let names: Vec<&str> = view.elements[0].attached.iter().map(|set| set.info.name).collect();
  assert_eq!(names, ["Slot", "Tag"]);
}
