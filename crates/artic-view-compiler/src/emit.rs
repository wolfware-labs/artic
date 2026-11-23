//! Writing a [`View`] as a formatted module body.

use crate::lower::{AttachedCall, ContentCall, Element, Name, Setter, View};
use artic_diagnostics::SourceFile;
use artic_view::CheckedContent;
use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use std::path::{Component, Path};
use syn::ext::IdentExt;

/// The module body for `view`, compiled from `source`. The path is written with its control
/// characters and bidirectional controls escaped in its header and comments, so it can't end a
/// comment and add code. Each element's statements follow a `// path:line:col` comment naming
/// where the element is written, so a rustc error in them leads back to the view.
pub(crate) fn emit(view: &View, source: &SourceFile) -> String {
  let file = escaped(&display(source.path()));
  let marker = |index: usize| {
    let index = Literal::usize_unsuffixed(index);
    quote!(__artic_source!(#index);)
  };
  let root = view.root;
  let tree_module = &view.tree;
  let traits = view.traits.values();
  let links = &view.links;
  let fields = view.names.iter().map(
    |Name {
       field,
       element,
       visibility,
     }| {
      let ty = &view.elements[*element].ty;
      let doc = format!(" The element named `{}`.", field.unraw());
      quote!(#[doc = #doc] #visibility #field: Handle<#ty>)
    },
  );
  let names: Vec<TokenStream> = view
    .names
    .iter()
    .map(|Name { field, element, .. }| {
      let local = local(*element);
      quote!(#field: #local)
    })
    .collect();
  let root_element = &view.elements[root];
  let code_type_root = view
    .code_type
    .as_ref()
    .map(|code_type| built(root_element, &code_type.path, quote!(View { #(#names,)* })));
  let root_is_tail = code_type_root.is_some() && root_element.attached.is_empty();
  let lets = view
    .elements
    .iter()
    .enumerate()
    .filter(|(index, _)| !root_is_tail || *index != root)
    .map(|(index, element)| {
      let local = local(index);
      let built = match &code_type_root {
        Some(code_type_root) if index == root => code_type_root.clone(),
        _ => built(element, &element.ty, quote!()),
      };
      let attached = element
        .attached
        .iter()
        .map(|AttachedCall { owner, setter, value }| quote!(#owner::#setter(tree, #local.id(), #value);));
      let marker = marker(index);
      quote! {
        #marker
        let #local = #built;
        #(#attached)*
      }
    });
  let members = view.code_type.as_ref().map(|code_type| {
    let root_ty = &root_element.ty;
    let content = code_type.content.iter().map(|(field, ty)| {
      quote! {
        /// The root's content, which the code type takes in the root's place.
        pub #field: #ty,
      }
    });
    quote! {
      /// The element type the code type takes the place of.
      pub type Root = #root_ty;

      /// The members of `Root` the code type has: its content.
      #[allow(dead_code, reason = "only a code type's macro reads it")]
      #[derive(Debug)]
      pub struct Members {
        #(#content)*
      }
    }
  });
  let (code_type, root_field, returns, tail, view_doc, build_doc) = match &view.code_type {
    Some(code_type) => {
      let (path, written) = (&code_type.path, &code_type.written);
      (
        quote!(Some(#written)),
        quote!(),
        quote!(Handle<#path>),
        match &code_type_root {
          Some(code_type_root) if root_is_tail => {
            let marker = marker(root);
            quote!(#marker #code_type_root)
          }
          _ => {
            let root_local = local(root);
            quote!(#root_local)
          }
        },
        format!(" The named nodes of `{file}`, one handle each."),
        format!(" Builds `{file}` in `tree`, with its code type at the root."),
      )
    }
    None => {
      let (root_ty, root_local) = (&root_element.ty, local(root));
      (
        quote!(None),
        quote!(
          /// The view's root.
          pub root: Handle<#root_ty>,
        ),
        quote!(View),
        quote!(View { root: #root_local, #(#names,)* }),
        format!(" The nodes of `{file}`: its root and one handle per named element."),
        format!(" Builds `{file}` in `tree` and returns its nodes."),
      )
    }
  };
  let tokens = quote! {
    use #tree_module::{Handle, UiTree};
    #(use #traits as _;)*
    #(use #links as _;)*

    /// The type the view's code is, which takes the root's place, as the view names it.
    #[allow(dead_code, reason = "only a code type's macro reads it")]
    pub const CODE_TYPE: Option<&str> = #code_type;

    #members

    #[doc = #view_doc]
    #[derive(Clone, Copy, Debug)]
    pub struct View {
      #root_field
      #(#fields,)*
    }

    #[doc = #build_doc]
    pub fn build(tree: &mut UiTree) -> #returns {
      #(#lets)*
      #tail
    }
  };
  let module: syn::File = syn::parse2(tokens).expect("the generated module parses");
  let mut order: Vec<usize> = (0..view.elements.len())
    .filter(|&index| !root_is_tail || index != root)
    .collect();
  if root_is_tail {
    order.push(root);
  }
  let comments: Vec<String> = order
    .iter()
    .map(|&index| {
      let position = source.line_col(view.elements[index].span.start);
      format!("{file}:{}:{}", position.line, position.column)
    })
    .collect();
  let body = with_sources(&prettyplease::unparse(&module), &order, &comments);
  format!("// Generated from {file}.\n\n{body}")
}

/// `code` with its markers made comments: the `__artic_source!(index);` statements of `build`'s
/// body, which the formatter puts each on a line of its own at the body's indent, in the order
/// `order` emitted them, each becoming `// ` and the comment at the same position in `comments`.
/// A line inside a multi-line string literal that reads like a marker is left alone, since only
/// the next expected marker at the body's indent matches.
fn with_sources(code: &str, order: &[usize], comments: &[String]) -> String {
  const INDENT: &str = "    ";
  let mut out = String::with_capacity(code.len());
  let mut next = 0;
  for line in code.lines() {
    let expected = order.get(next).map(|index| format!("{INDENT}__artic_source!({index});"));
    if expected.as_deref() == Some(line) {
      out.push_str(INDENT);
      out.push_str("// ");
      out.push_str(&comments[next]);
      next += 1;
    } else {
      out.push_str(line);
    }
    out.push('\n');
  }
  assert_eq!(next, order.len(), "every marker the emitter wrote is in the formatted code");
  out
}

/// The expression that builds `element` as `ty`, created with `ty::new(#arguments)`, then each
/// setter and the content call.
fn built(element: &Element, ty: &syn::Path, arguments: TokenStream) -> TokenStream {
  let setters = element
    .setters
    .iter()
    .map(|Setter { method, value }| quote!(.#method(#value)));
  let content = content(element.content.as_ref());
  quote!(#ty::new(#arguments) #(#setters)* #content .build(tree))
}

/// The content call of an element, or nothing.
fn content(content: Option<&ContentCall>) -> TokenStream {
  let Some(ContentCall { method, children }) = content else {
    return TokenStream::new();
  };
  match children {
    CheckedContent::None => TokenStream::new(),
    CheckedContent::Child(child) => {
      let child = local(*child);
      quote!(.#method(#child))
    }
    CheckedContent::Children(children) => {
      let children = children.iter().map(|child| local(*child));
      quote!(.#method([#(#children.id()),*]))
    }
  }
}

/// The local holding element `index`.
fn local(index: usize) -> proc_macro2::Ident {
  format_ident!("__e{index}")
}

/// `text` with its control characters, which can end a line, and the bidirectional controls rustc
/// refuses in comments written as escapes; every other character, accents and quotes among them,
/// as it is.
fn escaped(text: &str) -> String {
  let mut escaped = String::with_capacity(text.len());
  for character in text.chars() {
    let bidi = matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
    if character.is_control() || bidi {
      escaped.extend(character.escape_debug());
    } else {
      escaped.push(character);
    }
  }
  escaped
}

/// `path` with `/` separators and without a root, so the output is the same on every platform.
/// A caller passes a relative path; an absolute one at least writes no doubled separator.
fn display(path: &Path) -> String {
  path
    .components()
    .filter(|component| !matches!(component, Component::RootDir | Component::Prefix(_)))
    .map(|component| component.as_os_str().to_string_lossy())
    .collect::<Vec<_>>()
    .join("/")
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // Each marker at the body's indent becomes its comment, in the order emitted. A line that reads
  // like a marker inside a multi-line string literal stays as it is: one at another indent, or one
  // that names an element other than the next expected.
  #[test]
  fn markers_become_comments_in_order() {
    let code = concat!(
      "fn build() {\n",
      "    __artic_source!(1);\n",
      "    let a = r\"x\n",
      "__artic_source!(0);\n",
      "    __artic_source!(1);\n",
      "\";\n",
      "    __artic_source!(0);\n",
      "    a\n",
      "}\n",
    );
    let comments = ["v.ui:2:3".to_owned(), "v.ui:1:2".to_owned()];

    let written = with_sources(code, &[1, 0], &comments);

    let expected = concat!(
      "fn build() {\n",
      "    // v.ui:2:3\n",
      "    let a = r\"x\n",
      "__artic_source!(0);\n",
      "    __artic_source!(1);\n",
      "\";\n",
      "    // v.ui:1:2\n",
      "    a\n",
      "}\n",
    );
    assert_eq!(written, expected);
  }
}
