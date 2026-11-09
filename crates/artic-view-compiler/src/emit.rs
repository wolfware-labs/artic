//! Writing a [`View`] as a formatted module body.

use crate::lower::{Element, Name, Setter, View};
use artic_registry::ContentMethod;
use artic_view::CheckedContent;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::path::{Component, Path};
use syn::ext::IdentExt;

/// The module body for `view`, compiled from the file at `path`.
pub(crate) fn emit(view: &View, path: &Path) -> String {
  let file = display(path);
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
  let lets = view
    .elements
    .iter()
    .enumerate()
    .filter(|(index, _)| view.code_type.is_none() || *index != root)
    .map(|(index, element)| {
      let local = local(index);
      let built = built(element, &element.ty, quote!());
      quote!(let #local = #built;)
    });
  let root_element = &view.elements[root];
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
        built(root_element, path, quote!(View { #(#names,)* })),
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
  format!("// Generated from {file}.\n\n{}", prettyplease::unparse(&module))
}

/// The expression that builds `element` as `ty`, created with `ty::new(#arguments)`, then each
/// setter and the content call.
fn built(element: &Element, ty: &syn::Path, arguments: TokenStream) -> TokenStream {
  let setters = element
    .setters
    .iter()
    .map(|Setter { method, value }| quote!(.#method(#value)));
  let content = content(&element.content);
  quote!(#ty::new(#arguments) #(#setters)* #content .build(tree))
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
