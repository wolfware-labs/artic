use super::*;
use crate::infoset::{Directive, Member, MemberKey, TypeRef, XamlNamespace, XamlNode};
use artic_diagnostics::Span;
use pretty_assertions::assert_eq;

/// Parses `text`, panicking on errors, and returns the root node.
fn root(text: &str) -> XamlNode {
  let mut sources = SourceMap::new();
  let file = sources.add("test.xaml", text);
  match parse(&sources, file) {
    Ok(document) => document.root,
    Err(diagnostics) => panic!("unexpected errors: {}", diagnostics.render(&sources)),
  }
}

/// Parses `text`, expecting errors, and returns each error's message with the source text it points
/// at.
fn errors(text: &str) -> Vec<(String, &str)> {
  let mut sources = SourceMap::new();
  let file = sources.add("test.xaml", text);
  let diagnostics = parse(&sources, file).expect_err("the document has errors");
  diagnostics
    .iter()
    .map(|diagnostic| (diagnostic.message.clone(), &text[diagnostic.span.start..diagnostic.span.end]))
    .collect()
}

/// The members of an object node.
fn members(node: &XamlNode) -> &[Member] {
  match node {
    XamlNode::Object { members, .. } => members,
    other => panic!("expected an object, found {other:?}"),
  }
}

/// The member keys of an object node, in order.
fn keys(node: &XamlNode) -> Vec<MemberKey> {
  members(node).iter().map(|member| member.key.clone()).collect()
}

/// The text of a value node.
fn text(node: &XamlNode) -> &str {
  match node {
    XamlNode::Value { text, .. } => text,
    other => panic!("expected a value, found {other:?}"),
  }
}

/// The type name of an object node.
fn type_name(node: &XamlNode) -> &str {
  match node {
    XamlNode::Object { ty, .. } => &ty.name,
    other => panic!("expected an object, found {other:?}"),
  }
}

/// The id of the file the helpers above parse: the first one added to a new map.
fn first_file() -> SourceId {
  SourceMap::new().add("test.xaml", "")
}

fn property(name: &str) -> MemberKey {
  MemberKey::Property(name.to_owned())
}

/// The vocabulary most tests declare as their default namespace.
fn ui() -> XamlNamespace {
  XamlNamespace::Uri("urn:ui".to_owned())
}

/// The namespace of an object node's type.
fn namespace(node: &XamlNode) -> &XamlNamespace {
  match node {
    XamlNode::Object { ty, .. } | XamlNode::MarkupExtension { ty, .. } => &ty.namespace,
    other => panic!("expected an object or extension, found {other:?}"),
  }
}

fn attached(prefix: Option<&str>, owner: &str, namespace: XamlNamespace, name: &str, span: Span) -> MemberKey {
  MemberKey::Attached {
    owner: TypeRef {
      prefix: prefix.map(str::to_owned),
      name: owner.to_owned(),
      namespace,
      span,
    },
    name: name.to_owned(),
  }
}

// An attribute becomes a property member holding its value.
#[test]
fn attributes_are_properties() {
  let root = root(r#"<Button xmlns="urn:ui" Width="100" Content="OK"/>"#);

  assert_eq!(type_name(&root), "Button");
  assert_eq!(keys(&root), [property("Width"), property("Content")]);
  assert_eq!(text(&members(&root)[0].value[0]), "100");
}

// `xmlns` attributes become namespace declarations, not members.
#[test]
fn xmlns_attributes_are_namespace_declarations() {
  let root = root(r#"<Page xmlns="urn:ui" xmlns:x="https://artic.rs/markup"/>"#);

  let XamlNode::Object { namespaces, members, .. } = &root else {
    unreachable!()
  };
  let declared: Vec<_> = namespaces
    .iter()
    .map(|namespace| (namespace.prefix.as_deref(), namespace.uri.as_str()))
    .collect();
  assert_eq!(declared, [(None, "urn:ui"), (Some("x"), "https://artic.rs/markup")]);
  assert!(members.is_empty());
}

// Attributes in the markup namespace are directives.
#[test]
fn markup_attributes_are_directives() {
  let root = root(r#"<Button xmlns="urn:ui" xmlns:x="https://artic.rs/markup" x:Name="ok" x:Key="k"/>"#);

  assert_eq!(
    keys(&root),
    [MemberKey::Directive(Directive::Name), MemberKey::Directive(Directive::Key)]
  );
}

// `Owner.Name` attributes are attached properties, unless the owner is the element's own type.
#[test]
fn dotted_attributes_are_attached_properties() {
  let source = r#"<Button xmlns="urn:ui" xmlns:d="urn:dock" Grid.Row="1" d:Dock.Side="Left" Button.Width="2"/>"#;
  let root = root(source);

  let span = |owner: &str| {
    let start = source.find(owner).expect("owner in source");
    Span::new(first_file(), start, start + owner.len())
  };
  assert_eq!(
    keys(&root),
    [
      attached(None, "Grid", ui(), "Row", span("Grid")),
      attached(
        Some("d"),
        "Dock",
        XamlNamespace::Uri("urn:dock".to_owned()),
        "Side",
        span("d:Dock")
      ),
      property("Width"),
    ]
  );
}

// `<Type.Name>` elements are property elements; any other owner makes them attached.
#[test]
fn property_elements() {
  let source = "<Button xmlns=\"urn:ui\"><Button.Content><Image/></Button.Content><Grid.Row>1</Grid.Row></Button>";
  let root = root(source);

  let start = source.find("Grid").expect("owner in source");
  let owner = Span::new(first_file(), start, start + "Grid".len());
  assert_eq!(keys(&root), [property("Content"), attached(None, "Grid", ui(), "Row", owner)]);
  assert_eq!(type_name(&members(&root)[0].value[0]), "Image");
  assert_eq!(text(&members(&root)[1].value[0]), "1");
}

// Text and child objects go, in document order, into one content member after the other members.
#[test]
fn content_is_one_member_in_document_order() {
  let root = root("<Panel xmlns=\"urn:ui\">a<Image/>b<Panel.Tag>t</Panel.Tag></Panel>");

  assert_eq!(keys(&root), [property("Tag"), MemberKey::Content]);
  let content = &members(&root)[1].value;
  assert_eq!(content.len(), 3);
  assert_eq!(text(&content[0]), "a");
  assert_eq!(type_name(&content[1]), "Image");
  assert_eq!(text(&content[2]), "b");
}

// Entities are decoded, CDATA is kept verbatim, and adjacent text and CDATA merge into one value.
#[test]
fn text_and_cdata_merge_into_one_value() {
  let root = root("<Text xmlns=\"urn:ui\">a &amp; <![CDATA[<b> &amp;]]> c</Text>");

  let content = &members(&root)[0].value;
  assert_eq!(content.len(), 1);
  assert_eq!(text(&content[0]), "a & <b> &amp; c");
}

/// The text of a value node as written, before whitespace normalization.
fn raw(node: &XamlNode) -> &str {
  match node {
    XamlNode::Value { raw, .. } => raw,
    other => panic!("expected a value, found {other:?}"),
  }
}

// Whitespace-only text between elements adds nothing, but keeps its raw text for types whose
// whitespace is significant.
#[test]
fn raw_text_is_kept() {
  let root = root("<Panel xmlns=\"urn:ui\">\n  <Image/>\n</Panel>");

  let content = &members(&root)[0].value;
  assert_eq!((text(&content[0]), raw(&content[0])), ("", "\n  "));
  assert_eq!((text(&content[2]), raw(&content[2])), ("", "\n"));
}

// Text is trimmed at the edges of its element and collapsed; text beside a child keeps that space.
#[test]
fn element_text_is_normalized() {
  let root = root("<Text xmlns=\"urn:ui\">\n  Hello\t <Bold> world </Bold> !\n</Text>");

  let content = &members(&root)[0].value;
  assert_eq!(text(&content[0]), "Hello ");
  assert_eq!(text(&members(&content[1])[0].value[0]), "world");
  assert_eq!(text(&content[2]), " !");
}

// A property element's content is normalized at its own tags.
#[test]
fn property_element_text_is_normalized() {
  let root = root("<Button xmlns=\"urn:ui\"><Button.Content>\n  Click  me\n</Button.Content></Button>");

  assert_eq!(text(&members(&root)[0].value[0]), "Click me");
}

// `xml:space="preserve"` keeps text as written in its subtree, and a nested `default` restores the
// rules; the attribute itself is consumed, not a member.
#[test]
fn xml_space_preserve_and_nested_default() {
  let root = root(
    r#"<Panel xmlns="urn:ui" xml:space="preserve">  a  <Panel xml:space="default">  b  </Panel><Text>  c  </Text></Panel>"#,
  );

  let content = &members(&root)[0].value;
  assert_eq!(text(&content[0]), "  a  ");
  assert_eq!(text(&members(&content[1])[0].value[0]), "b");
  assert_eq!(text(&members(&content[2])[0].value[0]), "  c  ");
  assert_eq!(keys(&root), [MemberKey::Content]);
}

// `xml:space` accepts only `default` and `preserve`.
#[test]
fn bad_xml_space_is_an_error() {
  let found = errors(r#"<Panel xmlns="urn:ui" xml:space="keep"/>"#);

  assert_eq!(found, [("`xml:space` must be `default` or `preserve`".to_owned(), "keep")]);
}

// Attribute values are normalized as XML requires: whitespace characters become spaces.
#[test]
fn attribute_values_are_normalized() {
  let root = root("<Text xmlns=\"urn:ui\" Value=\"a\r\nb\tc &amp; &#10;\"/>");

  assert_eq!(text(&members(&root)[0].value[0]), "a b c & \n");
}

// Attribute values in braces are markup extensions, for directives too; `xmlns` values stay text.
#[test]
fn braced_attributes_are_markup_extensions() {
  let source = r#"<Button xmlns="urn:ui" xmlns:x="https://artic.rs/markup" xmlns:a="{urn}" Text="{Binding Name}" x:Key="{x:Type Button}" Tag="{}{x}"/>"#;
  let root = root(source);

  let XamlNode::Object { namespaces, .. } = &root else {
    unreachable!()
  };
  assert_eq!(namespaces[2].uri, "{urn}");
  let values: Vec<_> = members(&root).iter().map(|member| &member.value[0]).collect();
  assert!(matches!(values[0], XamlNode::MarkupExtension { ty, .. } if ty.name == "Binding"));
  assert!(matches!(values[1], XamlNode::MarkupExtension { ty, .. } if ty.name == "Type"));
  assert_eq!(text(values[2]), "{x}");
}

// A malformed markup extension drops its attribute, and the rest of the document is still checked.
#[test]
fn markup_extension_errors_are_collected() {
  let found = errors(r#"<Button xmlns="urn:ui" Text="{Binding" Tag="{A 'x}"><Button.Tag a="1"/></Button>"#);

  let messages: Vec<_> = found.iter().map(|(message, _)| message.as_str()).collect();
  assert_eq!(
    messages,
    [
      "markup extension `{Binding` is never closed",
      "quoted value is never closed",
      "a property element can't have attributes",
    ]
  );
}

// Comments, the XML declaration and processing instructions are dropped.
#[test]
fn comments_and_declarations_are_dropped() {
  let root = root("<?xml version=\"1.0\"?>\n<!-- top --><Panel xmlns=\"urn:ui\"><!-- inside --><?pi data?></Panel>\n");

  assert_eq!(type_name(&root), "Panel");
  assert!(members(&root).is_empty());
}

// Every node and member points at the source text it came from.
#[test]
fn spans_point_at_source() {
  let source = r#"<ui:Button xmlns:ui="urn:ui" Width="100">OK<ui:Button.Tag/></ui:Button>"#;
  let root = root(source);
  let at = |span: Span| &source[span.start..span.end];

  let XamlNode::Object {
    ty,
    members,
    namespaces,
    span,
  } = &root
  else {
    unreachable!()
  };
  assert_eq!(at(*span), source);
  assert_eq!(at(ty.span), "ui:Button");
  assert_eq!(at(namespaces[0].span), r#"xmlns:ui="urn:ui""#);
  assert_eq!(at(members[0].span), "Width");
  assert_eq!(at(members[0].value[0].span()), "100");
  assert_eq!(at(members[1].span), "ui:Button.Tag");
  assert_eq!(at(members[2].span), "OK");
}

// Malformed XML stops parsing with one error at the position the tokenizer reports: for a bad
// attribute, where the attribute starts.
#[test]
fn malformed_xml_reports_span() {
  let source = "<Panel xmlns=\"urn:ui\">\n  <Image Width=100/>\n</Panel>";

  let found = errors(source);

  assert_eq!(found.len(), 1);
  assert!(found[0].0.starts_with("malformed XML"), "{}", found[0].0);
  let offset = source.find(" Width").expect("attribute in source");
  let mut sources = SourceMap::new();
  let file = sources.add("test.xaml", source);
  let diagnostics = parse(&sources, file).expect_err("malformed");
  let span = diagnostics.iter().next().expect("one error").span;
  assert_eq!((span.start, span.end), (offset, offset));
}

// A wrong end tag points at itself and at the start tag it should close.
#[test]
fn mismatched_end_tag() {
  let source = "<Panel xmlns=\"urn:ui\"><Image></Panel>";
  let mut sources = SourceMap::new();
  let file = sources.add("test.xaml", source);

  let diagnostics = parse(&sources, file).expect_err("mismatched");

  let error = diagnostics.iter().next().expect("one error");
  assert_eq!(error.message, "expected `</Image>`, found `</Panel>`");
  assert_eq!(&source[error.span.start..error.span.end], "</Panel>");
  let opened = &error.labels[0];
  assert_eq!(&source[opened.span.start..opened.span.end], "Image");
  assert_eq!(opened.message, "opened here");
}

// An element still open at the end of the file is reported there, pointing back at its start tag.
#[test]
fn unclosed_element() {
  let source = "<Panel xmlns=\"urn:ui\"><Image/>";
  let mut sources = SourceMap::new();
  let file = sources.add("test.xaml", source);

  let diagnostics = parse(&sources, file).expect_err("unclosed");

  let error = diagnostics.iter().next().expect("one error");
  assert_eq!(error.message, "`<Panel>` is never closed");
  assert_eq!((error.span.start, error.span.end), (source.len(), source.len()));
  assert_eq!(&source[error.labels[0].span.start..error.labels[0].span.end], "Panel");
}

// The document needs exactly one root element, and it must be an object element.
#[test]
fn root_problems() {
  assert_eq!(
    errors("<!-- nothing -->"),
    [("expected one root element".to_owned(), "<!-- nothing -->")]
  );
  assert_eq!(
    errors("<A xmlns=\"urn:ui\"/><B/>"),
    [("expected one root element, found more content after it".to_owned(), "")]
  );
  assert_eq!(
    errors("<A.B/>"),
    [("the root element can't be a property element".to_owned(), "A.B")]
  );
}

// A DTD is rejected.
#[test]
fn dtd_is_an_error() {
  let found = errors("<!DOCTYPE Panel><Panel xmlns=\"urn:ui\"/>");

  assert_eq!(found, [("DTDs aren't supported in XAML".to_owned(), "<!DOCTYPE Panel>")]);
}

// A member set twice, by attribute or property element, is an error pointing at both.
#[test]
fn duplicate_members() {
  let found = errors(r#"<Button xmlns="urn:ui" Width="1" Width="2" Grid.Row="1" Grid.Row="2"/>"#);
  let twice = "this member is set twice on the same element".to_owned();
  assert_eq!(found, [(twice.clone(), "Width"), (twice, "Grid.Row")]);

  let source = r#"<Button xmlns="urn:ui" Tag="1"><Button.Tag>2</Button.Tag></Button>"#;
  let mut sources = SourceMap::new();
  let file = sources.add("test.xaml", source);
  let diagnostics = parse(&sources, file).expect_err("duplicate");
  let error = diagnostics.iter().next().expect("one error");
  assert_eq!(&source[error.span.start..error.span.end], "Button.Tag");
  assert_eq!(&source[error.labels[0].span.start..error.labels[0].span.end], "Tag");
}

// Misplaced attributes and elements are all reported, not only the first.
#[test]
fn mapping_errors_are_collected() {
  let found =
    errors(r#"<Button xmlns="urn:ui" xmlns:ui="urn:ui" ui:Width="1"><Button.Tag a="1"><Grid.Row/></Button.Tag></Button>"#);

  assert_eq!(
    found,
    [
      ("`ui:Width` names no property".to_owned(), "ui:Width"),
      ("a property element can't have attributes".to_owned(), "a"),
      ("a property element can't contain a property element".to_owned(), "Grid.Row"),
    ]
  );
}

// An unknown entity is an error at the entity, wherever it appears.
#[test]
fn unknown_entities_are_errors() {
  let found = errors(r#"<Text xmlns="urn:ui" Value="&nbsp;">&copy;</Text>"#);

  assert_eq!(found.len(), 2);
  assert_eq!(found[0].1, "&nbsp;");
  assert_eq!(found[1].1, "&copy;");
}

// The default namespace and the markup namespace resolve on elements, owners and extensions.
#[test]
fn default_and_markup_namespaces_resolve() {
  let root =
    root(r#"<Button xmlns="urn:ui" xmlns:x="https://artic.rs/markup" Tag="{x:Null}" Text="{Binding}"><x:String/></Button>"#);

  assert_eq!(namespace(&root), &ui());
  let values: Vec<_> = members(&root).iter().map(|member| &member.value[0]).collect();
  assert_eq!(namespace(values[0]), &XamlNamespace::Markup);
  assert_eq!(namespace(values[1]), &ui());
  assert_eq!(namespace(&members(&root)[2].value[0]), &XamlNamespace::Markup);
}

// A `using:` prefix stands for the Rust module path after it.
#[test]
fn using_prefix_resolves_to_path() {
  let root = root(r#"<v:Card xmlns:v="using:my_app::views"/>"#);

  assert_eq!(namespace(&root), &XamlNamespace::Using("my_app::views".to_owned()));
}

// An inner declaration shadows an outer one for its element and descendants, and ends with them.
#[test]
fn nested_xmlns_shadows_outer() {
  let root =
    root(r#"<v:Page xmlns:v="using:app::views"><v:Panel xmlns:v="using:app::widgets"><v:Card/></v:Panel><v:Header/></v:Page>"#);

  let panel = &members(&root)[0].value[0];
  let card = &members(panel)[0].value[0];
  let header = &members(&root)[0].value[1];
  let views = XamlNamespace::Using("app::views".to_owned());
  let widgets = XamlNamespace::Using("app::widgets".to_owned());
  assert_eq!(
    [namespace(&root), namespace(panel), namespace(card), namespace(header)],
    [&views, &widgets, &widgets, &views]
  );
}

// An element's declarations apply to the whole element, including names written before them.
#[test]
fn declarations_apply_to_their_whole_element() {
  let root = root(r#"<a:Card a:Dock.Side="Left" xmlns:a="using:app::widgets"/>"#);

  let widgets = XamlNamespace::Using("app::widgets".to_owned());
  assert_eq!(namespace(&root), &widgets);
  assert!(matches!(&keys(&root)[0], MemberKey::Attached { owner, .. } if owner.namespace == widgets));
}

// Directives are recognized by namespace, not by the letters of the prefix.
#[test]
fn directives_follow_the_markup_namespace() {
  let root = root(r#"<Button xmlns="urn:ui" xmlns:m="https://artic.rs/markup" m:Name="ok"/>"#);
  assert_eq!(keys(&root), [MemberKey::Directive(Directive::Name)]);

  let found = errors(r#"<Button xmlns="urn:ui" xmlns:x="urn:other" x:Name="ok"/>"#);
  assert_eq!(found, [("`x:Name` names no property".to_owned(), "x:Name")]);
}

// Two prefixes for one namespace name the same types, so the owner check compares namespaces.
#[test]
fn prefixes_are_aliases() {
  let root = root(r#"<a:Button xmlns:a="urn:ui" xmlns:b="urn:ui"><b:Button.Content>x</b:Button.Content></a:Button>"#);

  assert_eq!(keys(&root), [property("Content")]);
}

// An undeclared prefix, or a missing default namespace, is reported once, at its first use.
#[test]
fn undeclared_prefixes_are_reported_once() {
  let found = errors(r#"<Panel><a:Card/><a:Card/><Image/></Panel>"#);

  assert_eq!(
    found,
    [
      ("no default namespace is declared".to_owned(), "Panel"),
      ("prefix `a` is not declared".to_owned(), "a"),
    ]
  );
}

/// Parses `text`, expecting errors, and returns each error's message, the text it points at, and its
/// help.
fn errors_with_help(text: &str) -> Vec<(String, &str, Option<String>)> {
  let mut sources = SourceMap::new();
  let file = sources.add("test.xaml", text);
  let diagnostics = parse(&sources, file).expect_err("the document has errors");
  diagnostics
    .iter()
    .map(|diagnostic| {
      let at = &text[diagnostic.span.start..diagnostic.span.end];
      (diagnostic.message.clone(), at, diagnostic.help.clone())
    })
    .collect()
}

// The Microsoft `x:` URI is rejected on its declaration, with the artic URI under the same prefix;
// its uses are still directives, so they add no errors.
#[test]
fn microsoft_x_uri_rejected_with_hint() {
  let found =
    errors_with_help(r#"<Button xmlns="urn:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="ok"/>"#);

  assert_eq!(
    found,
    [(
      "`http://schemas.microsoft.com/winfx/2006/xaml` isn't supported: artic's XAML language namespace has its own URI"
        .to_owned(),
      r#"xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml""#,
      Some(r#"use `xmlns:x="https://artic.rs/markup"`"#.to_owned()),
    )]
  );
}

// The WPF presentation URI is rejected with the artic UI URI.
#[test]
fn wpf_presentation_uri_rejected_with_hint() {
  let found = errors_with_help(r#"<Button xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#);

  assert_eq!(
    found[0].0,
    "`http://schemas.microsoft.com/winfx/2006/xaml/presentation` isn't supported: artic's controls are their own vocabulary"
  );
  assert_eq!(found[0].2.as_deref(), Some(r#"use `xmlns="https://artic.rs/ui"`"#));
  assert_eq!(found.len(), 1);
}

// `clr-namespace:` is rejected with a `using:` path made from the .NET namespace.
#[test]
fn clr_namespace_rejected_with_hint() {
  let found = errors_with_help(r#"<v:Card xmlns:v="clr-namespace:MyApp.ViewModels;assembly=MyApp"/>"#);

  assert_eq!(
    found,
    [(
      "`clr-namespace:` isn't supported: prefixes map to Rust modules, not .NET namespaces".to_owned(),
      r#"xmlns:v="clr-namespace:MyApp.ViewModels;assembly=MyApp""#,
      Some(r#"artic maps prefixes to Rust modules; for example `xmlns:v="using:my_app::view_models"`"#.to_owned()),
    )]
  );
}

// A `using:` path must be a module path from a crate name, each segment an identifier by Unicode's
// XID rules; the error points at the bad segment, as written (`Vec&lt;T&gt;` is `Vec<T>` once
// decoded).
#[test]
fn invalid_using_paths_are_errors() {
  let cases = [
    ("using:", "`using:` needs a Rust module path", ""),
    ("using:crate::views", "a `using:` path starts with a crate name", "crate"),
    ("using:::app", "a `using:` path starts with a crate name", ""),
    ("using:app::::views", "empty segment in a `using:` path", ""),
    ("using:app::", "empty segment in a `using:` path", ""),
    ("using:my-app::views", "`my-app` isn't a Rust identifier", "my-app"),
    ("using:app::Vec&lt;T&gt;", "`Vec<T>` isn't a Rust identifier", "Vec&lt;T&gt;"),
    ("using:app::type", "`type` is a Rust keyword", "type"),
    ("using:app::a\u{2E2F}", "`a\u{2E2F}` isn't a Rust identifier", "a\u{2E2F}"),
  ];
  for (uri, message, at) in cases {
    let source = format!(r#"<v:Card xmlns:v="{uri}"/>"#);
    assert_eq!(errors(&source), [(message.to_owned(), at)], "for {uri:?}");
  }
}

// An empty URI, a reserved prefix and a prefix declared twice on one element are errors.
#[test]
fn bad_declarations_are_errors() {
  let found = errors(r#"<Button xmlns="urn:ui" xmlns:a="" xmlns:xml="urn:x" xmlns:b="urn:b" xmlns:b="urn:c"/>"#);

  assert_eq!(
    found,
    [
      ("a namespace can't be empty".to_owned(), r#"xmlns:a="""#),
      ("the `xml` prefix is reserved".to_owned(), r#"xmlns:xml="urn:x""#),
      (
        "prefix `b` is declared twice on this element".to_owned(),
        r#"xmlns:b="urn:c""#
      ),
    ]
  );
}

/// The artic namespaces the rejection tests declare: triggers are rejected only in the artic UI
/// vocabulary.
const ARTIC: &str = r#"xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup""#;

// Each construct of the spec's "Not supported" list is rejected at its name, in every form it can
// take, with the reason and what to write instead.
#[test]
fn unsupported_constructs_are_rejected() {
  let triggers = (
    "styles react to state with selectors",
    "use a selector with a pseudo-class, like `Button:pointerover`",
  );
  let construction = "objects are created with `Default`, then their properties are set";
  let cases = [
    (
      r#"<Button {} x:Shared="False"/>"#,
      "x:Shared",
      "each resource is created once and shared",
      "create one instance per use with a template",
    ),
    (
      r#"<Button {} x:Subclass="a::B"/>"#,
      "x:Subclass",
      "the `x:Class` struct is the view's type",
      "remove `x:Subclass`",
    ),
    (
      r#"<Button {} x:TypeArguments="x:String"/>"#,
      "x:TypeArguments",
      "no generics from XAML in v1",
      "use a concrete type, or wrap the generic type in a non-generic one in Rust",
    ),
    (
      r#"<Button {} x:FactoryMethod="new"/>"#,
      "x:FactoryMethod",
      construction,
      "set properties instead of calling a factory method",
    ),
    (
      "<Panel {}><x:Arguments/></Panel>",
      "x:Arguments",
      construction,
      "set properties instead of passing constructor arguments",
    ),
    (
      "<Panel {}><x:Code/></Panel>",
      "x:Code",
      "views' logic is Rust, not inline code",
      "put the code in the view's `#[ui_class]` struct",
    ),
    (
      r#"<Button {} Tag="{x:Reference other}"/>"#,
      "x:Reference",
      "elements refer to each other through bindings",
      "use `{Binding ElementName=name}`",
    ),
    ("<Style {}><Trigger/></Style>", "Trigger", triggers.0, triggers.1),
    ("<Style {}><MultiTrigger/></Style>", "MultiTrigger", triggers.0, triggers.1),
    ("<Style {}><DataTrigger/></Style>", "DataTrigger", triggers.0, triggers.1),
    (
      "<Style {}><MultiDataTrigger/></Style>",
      "MultiDataTrigger",
      triggers.0,
      triggers.1,
    ),
    ("<Style {}><EventTrigger/></Style>", "EventTrigger", triggers.0, triggers.1),
  ];
  for (template, construct, reason, suggestion) in cases {
    let source = template.replace("{}", ARTIC);
    let found = errors_with_help(&source);
    let message = format!("`{construct}` isn't supported: {reason}");
    assert_eq!(found, [(message, construct, Some(suggestion.to_owned()))], "in {source}");
  }
}

// Any other name in the markup namespace is rejected with what the namespace has, whatever prefix
// it's written with.
#[test]
fn unknown_x_directive_rejected() {
  let source = format!(r#"<Button {ARTIC} xmlns:m="https://artic.rs/markup" x:Foo="1" Tag="{{m:Bar}}"><x:Baz/></Button>"#);

  let found = errors_with_help(&source);

  let messages: Vec<_> = found.iter().map(|(message, at, _)| (message.as_str(), *at)).collect();
  assert_eq!(
    messages,
    [
      ("`x:Foo` isn't part of the XAML language namespace", "x:Foo"),
      ("`m:Bar` isn't part of the XAML language namespace", "m:Bar"),
      ("`x:Baz` isn't part of the XAML language namespace", "x:Baz"),
    ]
  );
  assert!(
    found[0]
      .2
      .as_deref()
      .is_some_and(|help| help.starts_with("it has directives `x:Name`"))
  );
}

// A rejected name in the markup namespace is caught under any prefix.
#[test]
fn rejections_follow_the_markup_namespace() {
  let found = errors(r#"<Panel xmlns="https://artic.rs/ui" xmlns:m="https://artic.rs/markup"><m:Code/></Panel>"#);

  assert_eq!(
    found,
    [(
      "`x:Code` isn't supported: views' logic is Rust, not inline code".to_owned(),
      "m:Code"
    )]
  );
}

// Triggers are rejected only in the artic UI vocabulary: an app's own `Trigger` type is fine.
#[test]
fn own_trigger_types_are_allowed() {
  let root = root(r#"<Style xmlns="https://artic.rs/ui" xmlns:my="using:my_app::styles"><my:Trigger/></Style>"#);

  assert_eq!(type_name(&members(&root)[0].value[0]), "Trigger");
}

// The table holds every construct of the spec's "Not supported" list (§4) the parser can detect;
// loose XAML of types not marked `XamlType` needs the registry.
#[test]
fn table_lists_the_spec_constructs() {
  let spec = [
    "x:Code",
    "x:Subclass",
    "x:Shared",
    "x:Arguments",
    "x:FactoryMethod",
    "x:TypeArguments",
    "x:Reference",
    "Trigger",
    "DataTrigger",
    "EventTrigger",
    "clr-namespace:",
    "http://schemas.microsoft.com/winfx/2006/xaml",
  ];
  for construct in spec {
    assert!(
      unsupported::UNSUPPORTED.iter().any(|row| row.construct == construct),
      "{construct} is missing"
    );
  }
}
