//! Turning XAML text into the infoset.

mod builder;
mod markup;
mod namespace;
#[cfg(test)]
mod tests;
mod text;
mod unsupported;
mod whitespace;

use crate::infoset::{Member, MemberKey, XamlDocument, XamlNamespace};
use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, SourceMap};
use builder::Builder;

/// Parses the XAML file `file` of `sources` into its infoset.
///
/// # Errors
///
/// Returns every problem found: malformed XML stops parsing at the first error, while misplaced
/// attributes and elements are all collected before returning.
///
/// # Examples
///
/// ```
/// use artic_diagnostics::SourceMap;
/// use artic_xaml::infoset::{MemberKey, XamlNode};
/// use artic_xaml::parse;
///
/// let mut sources = SourceMap::new();
/// let file = sources.add("view.xaml", r#"<Button xmlns="https://artic.rs/ui" Width="100">OK</Button>"#);
/// let document = parse(&sources, file).expect("valid XAML");
/// let XamlNode::Object { ty, members, .. } = &document.root else { unreachable!() };
/// assert_eq!(ty.name, "Button");
/// assert_eq!(members[0].key, MemberKey::Property("Width".to_owned()));
/// assert_eq!(members[1].key, MemberKey::Content);
/// ```
pub fn parse(sources: &SourceMap, file: SourceId) -> Result<XamlDocument, Diagnostics> {
  Builder::new(file, sources.get(file)).run()
}

/// Marks an error that was already recorded and stops the work in progress: the whole parse in the
/// builder, one attribute value in the markup extension parser.
#[must_use]
struct Fatal;

/// An error if `members` already sets `member`'s key, pointing at both.
fn duplicate_of(members: &[Member], member: &Member) -> Option<Diagnostic> {
  let first = members.iter().find(|existing| same_member(&existing.key, &member.key))?;
  Some(
    Diagnostic::error(member.span, "this member is set twice on the same element")
      .with_label("set again here")
      .with_secondary(first.span, "first set here"),
  )
}

/// Whether two keys name the same member, whatever prefixes their owner types are written with.
fn same_member(a: &MemberKey, b: &MemberKey) -> bool {
  match (a, b) {
    (
      MemberKey::Attached {
        owner: a_owner,
        name: a_name,
      },
      MemberKey::Attached {
        owner: b_owner,
        name: b_name,
      },
    ) => a_owner.namespace == b_owner.namespace && a_owner.name == b_owner.name && a_name == b_name,
    _ => a == b,
  }
}

/// The namespace of a [`TypeRef`](crate::infoset::TypeRef) until the builder resolves it. It stays
/// only in documents with errors, which [`parse`] never returns.
fn unresolved() -> XamlNamespace {
  XamlNamespace::Uri(String::new())
}
