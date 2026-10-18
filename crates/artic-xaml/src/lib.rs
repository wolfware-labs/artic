//! XAML parser producing the artic infoset, with spanned diagnostics.
//!
//! [`parse`](fn@parse) reads one file of a [`SourceMap`](diagnostics::SourceMap) into an
//! [`XamlDocument`](infoset::XamlDocument). XML is read with the `xmlparser` tokenizer, which keeps
//! byte positions for every token, so each node of the infoset points at its source text and every
//! error is reported with a span. Namespace prefixes are resolved with XML's scoping rules while
//! parsing, so every type reference in a returned document knows its namespace.
//!
//! [`NamespaceMap`](namespaces::NamespaceMap) says which crates each namespace URI stands for, for
//! the back ends that look types up.
//!
//! ```
//! use artic_xaml::diagnostics::SourceMap;
//! use artic_xaml::parse;
//!
//! let mut sources = SourceMap::new();
//! let file = sources.add("view.xaml", r#"<Button xmlns="https://artic.rs/ui"></Panel>"#);
//! let errors = parse(&sources, file).expect_err("mismatched end tag");
//! assert!(errors.render(&sources).contains("expected `</Button>`, found `</Panel>`"));
//! ```
//!
//! # Whitespace
//!
//! Text inside elements follows the MS-XAML rules. Each text value keeps `raw`, its whitespace as
//! written, and `text`, after these rules:
//!
//! - Whitespace is XML's: space, tab, carriage return and line feed. A non-breaking space is
//!   content.
//! - Runs of whitespace collapse to one space.
//! - An element's content is trimmed at its start and end tags only, so text beside a child
//!   element keeps the space on that side.
//! - Whitespace-only text, such as indentation between elements, has an empty `text`: it adds
//!   nothing, and types whose whitespace is significant read `raw` instead.
//! - `xml:space="preserve"` keeps text as written in its subtree, and `xml:space="default"`
//!   restores the rules.
//!
//! Attribute values only get XML's attribute normalization; trimming them is up to each type's
//! `FromStr`. A line break between East Asian characters becomes a space like any other, so write
//! such text on one line, or use `xml:space="preserve"`.
//!
//! ```
//! use artic_xaml::diagnostics::SourceMap;
//! use artic_xaml::infoset::{MemberKey, XamlNode};
//! use artic_xaml::parse;
//!
//! /// The root's content: each text's `text`, or `<Name>` for a child element.
//! fn content(xaml: &str) -> Vec<String> {
//!   let mut sources = SourceMap::new();
//!   let file = sources.add("view.xaml", xaml.replace("<Root", r#"<Root xmlns="https://artic.rs/ui""#));
//!   let XamlNode::Object { members, .. } = parse(&sources, file).expect("valid XAML").root else {
//!     unreachable!()
//!   };
//!   let content = members.into_iter().find(|member| member.key == MemberKey::Content).expect("content");
//!   let show = |node| match node {
//!     XamlNode::Value { text, .. } => text,
//!     XamlNode::Object { ty, .. } => format!("<{}>", ty.name),
//!     XamlNode::MarkupExtension { .. } => unreachable!(),
//!   };
//!   content.value.into_iter().map(show).collect()
//! }
//!
//! // Trimmed at the tags, runs collapsed.
//! assert_eq!(content("<Root>\n    Hello \t world\n</Root>"), ["Hello world"]);
//! // Text beside a child keeps that space.
//! assert_eq!(content("<Root>\n  Hello <Bold>world</Bold> !\n</Root>"), ["Hello ", "<Bold>", " !"]);
//! // Indentation between elements is empty.
//! assert_eq!(content("<Root>\n  <A/>\n  <B/>\n</Root>"), ["", "<A>", "", "<B>", ""]);
//! // `xml:space="preserve"` keeps text as written.
//! assert_eq!(content(r#"<Root xml:space="preserve">  two  spaces </Root>"#), ["  two  spaces "]);
//! ```

pub mod diagnostics;
pub mod infoset;
pub mod namespaces;
mod parse;

pub use parse::parse;
