//! Writing a [`View`] as a formatted module body.

use crate::lower::{Content, Setter, View};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::path::Path;

/// The module body for `view`, compiled from the file at `path`.
pub(crate) fn emit(view: &View, path: &Path) -> String {
  let file = display(path);
  let root = view.elements.len() - 1;
  let root_ty = &view.elements[root].ty;
  let root_local = local(root);
  let traits = view.traits.values();
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
  let view_doc = format!(" The nodes of `{file}`: its root and one handle per `x:Name`.");
  let build_doc = format!(" Builds `{file}` in `tree` and returns its nodes.");
  let tokens = quote! {
    use ::artic::prelude::{Handle, UiTree};
    #(use #traits as _;)*

    #[doc = #view_doc]
    #[allow(non_snake_case)]
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
fn content(content: &Content) -> TokenStream {
  match content {
    Content::None => TokenStream::new(),
    Content::Child { method, child } => {
      let child = local(*child);
      quote!(.#method(#child))
    }
    Content::Children { method, children } => {
      let children = children.iter().map(|child| local(*child));
      quote!(.#method([#(#children.id()),*]))
    }
  }
}

/// The local holding element `index`.
fn local(index: usize) -> proc_macro2::Ident {
  format_ident!("__e{index}")
}

/// `path` with `/` separators, so the output is the same on every platform.
fn display(path: &Path) -> String {
  path.iter().map(|part| part.to_string_lossy()).collect::<Vec<_>>().join("/")
}
