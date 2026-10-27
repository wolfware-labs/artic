//! Writing a [`View`] as a formatted module body.

use crate::lower::{Setter, View};
use artic_registry::ContentMethod;
use artic_view::CheckedContent;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::path::{Component, Path};

/// The module body for `view`, compiled from the file at `path`.
pub(crate) fn emit(view: &View, path: &Path) -> String {
  let file = display(path);
  let root = view.root;
  let root_ty = &view.elements[root].ty;
  let root_local = local(root);
  let tree_module = &view.tree;
  let traits = view.traits.values();
  let links = &view.links;
  let fields = view.names.iter().map(|(name, index)| {
    let ty = &view.elements[*index].ty;
    quote!(pub #name: Handle<#ty>)
  });
  let names = view.names.iter().map(|(name, index)| {
    let local = local(*index);
    quote!(#name: #local)
  });
  let lets = view.elements.iter().enumerate().map(|(index, element)| {
    let local = local(index);
    let ty = &element.ty;
    let setters = element
      .setters
      .iter()
      .map(|Setter { method, value }| quote!(.#method(#value)));
    let content = content(&element.content);
    quote!(let #local = #ty::new() #(#setters)* #content .build(tree);)
  });
  let view_doc = format!(" The nodes of `{file}`: its root and one handle per named element.");
  let build_doc = format!(" Builds `{file}` in `tree` and returns its nodes.");
  let tokens = quote! {
    use #tree_module::{Handle, UiTree};
    #(use #traits as _;)*
    #(use #links as _;)*

    #[doc = #view_doc]
    #[derive(Clone, Copy, Debug)]
    pub struct View {
      pub root: Handle<#root_ty>,
      #(#fields,)*
    }

    #[doc = #build_doc]
    pub fn build(tree: &mut UiTree) -> View {
      #(#lets)*
      View { root: #root_local, #(#names,)* }
    }
  };
  let module: syn::File = syn::parse2(tokens).expect("the generated module parses");
  format!("// Generated from {file}.\n\n{}", prettyplease::unparse(&module))
}

/// The content call of an element, or nothing.
fn content(content: &CheckedContent) -> TokenStream {
  match content {
    CheckedContent::None => TokenStream::new(),
    CheckedContent::Child(child) => {
      let method = format_ident!("{}", ContentMethod::Child.name());
      let child = local(*child);
      quote!(.#method(#child))
    }
    CheckedContent::Children(children) => {
      let method = format_ident!("{}", ContentMethod::Children.name());
      let children = children.iter().map(|child| local(*child));
      quote!(.#method([#(#children.id()),*]))
    }
  }
}

/// The local holding element `index`.
fn local(index: usize) -> proc_macro2::Ident {
  format_ident!("__e{index}")
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
