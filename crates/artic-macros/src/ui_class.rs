use crate::crate_root::{ELEMENT, crate_root};
use crate::element::expand_struct;
use crate::ui_type::attributes::parse_type_attributes;
use artic_ident::{is_keyword, snake_case};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::path::PathBuf;
use syn::{DeriveInput, Error, Fields, FieldsNamed, Ident, ItemStruct, LitStr, parse_quote};

/// Where the macro is expanded: what the compiler and Cargo say about the struct's crate.
pub struct Site {
  /// The file the struct is in, if the compiler says.
  pub source_file: Option<PathBuf>,
  /// The crate's directory, `CARGO_MANIFEST_DIR`.
  pub manifest_dir: PathBuf,
  /// The build script's output directory, `OUT_DIR`, if the crate has a build script.
  pub out_dir: Option<PathBuf>,
}

/// Makes the struct the code type of the view at `argument`: includes the compiled view in a module
/// named after the struct, adds the `ui` and `child` fields, makes the struct an element with the
/// default `ElementImpl`, and writes `create` and the assertion that the view names this struct.
pub fn expand(argument: TokenStream, item: TokenStream, site: &Site) -> syn::Result<TokenStream> {
  let path: LitStr = syn::parse2(argument).map_err(|error| {
    Error::new(
      error.span(),
      "`#[ui_class]` takes the view's path, such as `#[ui_class(\"login.xaml\")]`",
    )
  })?;
  let mut item: ItemStruct = syn::parse2(item)?;
  if !item.generics.params.is_empty() {
    return Err(Error::new_spanned(
      &item.generics,
      "`#[ui_class]` can't be used on a generic struct",
    ));
  }
  let mut fields: FieldsNamed = match &item.fields {
    Fields::Named(fields) => fields.clone(),
    Fields::Unit => parse_quote!({}),
    Fields::Unnamed(fields) => {
      return Err(Error::new_spanned(
        fields,
        "`#[ui_class]` needs a struct with named fields, or a unit struct",
      ));
    }
  };
  for field in &fields.named {
    let name = field.ident.as_ref().expect("named fields have names");
    if name == "ui" || name == "child" {
      return Err(Error::new_spanned(
        name,
        format!("`{name}` is added by `#[ui_class]`; rename this field"),
      ));
    }
  }
  let ident = item.ident.clone();
  let module = module_ident(&ident)?;
  let view = view_path(site, &path)?;

  let probe: DeriveInput = syn::parse2(quote!(#item))?;
  let root = crate_root(parse_type_attributes(&probe)?.crate_path.as_ref(), &ident, ELEMENT)?;
  let vis = item.vis.clone();
  let ui_doc = format!("The named nodes of `{view}`.");
  let added: FieldsNamed = parse_quote!({
    #[doc = #ui_doc]
    #[ui(required)]
    #vis ui: #module::View,
    /// The content of the view's root.
    #[ui(content)]
    pub child: #root::__private::Child,
  });
  let own = std::mem::take(&mut fields.named);
  fields.named = added.named.into_iter().chain(own).collect();
  item.fields = Fields::Named(fields);
  item.semi_token = None;
  let element = expand_struct(item)?;

  let compiled = format!("/views/{}", with_rs_extension(&view));
  let module_doc = format!("The compiled `{view}`.");
  let create_doc = format!("Builds `{view}` in `tree`, with this type at the root.");
  let name = ident.to_string();
  let message = format!("`{view}` must name `");
  let message_end = format!("::{name}` as its code type");
  Ok(quote_spanned! {ident.span()=>
    #[doc = #module_doc]
    #vis mod #module {
      ::core::include!(::core::concat!(::core::env!("OUT_DIR"), #compiled));
    }

    #element

    impl #root::__private::ElementImpl for #ident {}

    impl #ident {
      #[doc = #create_doc]
      #[must_use = "the handle is needed to link or reach the node"]
      #vis fn create(tree: &mut #root::__private::UiTree) -> #root::__private::Handle<Self> {
        #module::build(tree)
      }
    }

    const _: () = ::core::assert!(
      #root::__private::same_code_type(#module::CODE_TYPE, ::core::concat!(::core::module_path!(), "::", #name)),
      ::core::concat!(#message, ::core::module_path!(), #message_end),
    );
  })
}

/// The module the compiled view goes in: the struct's name in snake case, raw if it's a keyword.
fn module_ident(ident: &Ident) -> syn::Result<Ident> {
  let name = snake_case(&ident.to_string());
  match name.as_str() {
    "self" | "super" | "crate" => Err(Error::new_spanned(
      ident,
      format!("`{name}` can't name the module the view goes in; rename this struct"),
    )),
    _ if is_keyword(&name) => Ok(Ident::new_raw(&name, ident.span())),
    _ => Ok(Ident::new(&name, ident.span())),
  }
}

/// The view's path in the crate, checked against the site: the struct's file is known, the view is
/// inside the crate, and the build script compiled it.
fn view_path(site: &Site, path: &LitStr) -> syn::Result<String> {
  let argument = path.value();
  let source_file = site.source_file.as_ref().ok_or_else(|| {
    Error::new(
      path.span(),
      "can't find the file this struct is in, so the view's path can't be resolved",
    )
  })?;
  let view = crate_relative(
    &source_file.to_string_lossy(),
    &site.manifest_dir.to_string_lossy(),
    &argument,
  )
  .ok_or_else(|| {
    Error::new(
      path.span(),
      format!("`{argument}` is outside the crate, so it has no compiled view"),
    )
  })?;
  let out_dir = site.out_dir.as_ref().ok_or_else(|| {
    Error::new(
      path.span(),
      "this crate compiles no views: it has no build script, so it has no compiled view",
    )
  })?;
  if !out_dir.join("views").join(with_rs_extension(&view)).is_file() {
    return Err(Error::new(
      path.span(),
      format!("no compiled view for `{view}`: the file doesn't exist, or the build script's `compile_dir` doesn't cover it"),
    ));
  }
  Ok(view)
}

/// `path` with its last segment's extension replaced by `rs`, as `compile_dir` names its output.
fn with_rs_extension(path: &str) -> String {
  let name_start = path.rfind('/').map_or(0, |slash| slash + 1);
  match path[name_start..].rfind('.') {
    Some(dot) if dot > 0 => format!("{}.rs", &path[..name_start + dot]),
    _ => format!("{path}.rs"),
  }
}

/// The path of `argument`, written relative to `source_file` as `include_str!` reads it, relative
/// to `manifest_dir` with `/` separators, or `None` if it isn't inside the crate.
fn crate_relative(source_file: &str, manifest_dir: &str, argument: &str) -> Option<String> {
  let source_file = slashes(source_file);
  let argument = slashes(argument);
  let mut segments: Vec<&str> = if is_absolute(&argument) {
    Vec::new()
  } else {
    let mut segments: Vec<&str> = source_file.split('/').collect();
    segments.pop();
    segments
  };
  for segment in argument.split('/') {
    match segment {
      "" | "." if !segments.is_empty() => {}
      ".." => {
        segments.pop();
      }
      _ => segments.push(segment),
    }
  }
  let manifest_dir = slashes(manifest_dir);
  let relative = segments.join("/");
  let relative = relative.strip_prefix(manifest_dir.trim_end_matches('/'))?.strip_prefix('/')?;
  Some(relative.to_owned())
}

/// `path` with `/` separators and without Windows' verbatim prefix.
fn slashes(path: &str) -> String {
  path.strip_prefix(r"\\?\").unwrap_or(path).replace('\\', "/")
}

/// Whether a path with `/` separators starts at a root or a drive.
fn is_absolute(path: &str) -> bool {
  let bytes = path.as_bytes();
  path.starts_with('/') || (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/')
}

#[cfg(test)]
mod tests {
  use super::*;
  use proc_macro2::Span;
  use quote::quote;
  use std::fs;

  /// A site for `src/views/login.rs` in the crate at `/w/app`, whose build script wrote
  /// `$OUT_DIR/views/src/views/login.rs` unless `compiled` is false.
  fn site(name: &str, compiled: bool) -> Site {
    let out_dir = std::env::temp_dir().join(format!("artic-ui-class-{name}-{}", std::process::id()));
    let views = out_dir.join("views/src/views");
    fs::create_dir_all(&views).expect("a temporary directory");
    if compiled {
      fs::write(views.join("login.rs"), "").expect("a compiled view");
    }
    Site {
      source_file: Some("/w/app/src/views/login.rs".into()),
      manifest_dir: "/w/app".into(),
      out_dir: Some(out_dir),
    }
  }

  fn error(argument: TokenStream, item: TokenStream, site: &Site) -> String {
    expand(argument, item, site).expect_err("an error").to_string()
  }

  // Everything the macro writes for a struct with a field of its own, expanded whole.
  #[test]
  fn whole_expansion() {
    let item = quote! {
      #[derive(Debug)]
      pub struct LoginView {
        attempts: u32,
      }
    };

    let expanded = expand(quote!("login.xaml"), item, &site("whole", true)).expect("valid");

    let file: syn::File = syn::parse2(expanded).expect("parses");
    insta::assert_snapshot!(prettyplease::unparse(&file));
  }

  // A path from a Windows build, with backslashes and the verbatim prefix, gives the same view.
  #[test]
  fn windows_paths_use_slashes() {
    assert_eq!(
      crate_relative(r"\\?\C:\w\app\src\views\login.rs", r"C:\w\app", "login.xaml").as_deref(),
      Some("src/views/login.xaml")
    );
  }

  // `.` and `..` segments are resolved, as the file system would.
  #[test]
  fn parent_segments_resolve() {
    assert_eq!(
      crate_relative("/w/app/src/views/login.rs", "/w/app/", "./../shared/login.xaml").as_deref(),
      Some("src/shared/login.xaml")
    );
  }

  // A view outside the crate, by `..` or by an absolute path, has no path in the crate.
  #[test]
  fn outside_the_crate_has_no_path() {
    assert_eq!(crate_relative("/w/app/src/login.rs", "/w/app", "../../login.xaml"), None);
    assert_eq!(crate_relative("/w/app/src/login.rs", "/w/app", "/w/other/login.xaml"), None);
    assert_eq!(crate_relative("/w/application/src/login.rs", "/w/app", "login.xaml"), None);
  }

  // An absolute path inside the crate is the view at that path.
  #[test]
  fn absolute_path_inside_the_crate() {
    assert_eq!(
      crate_relative("/w/app/src/login.rs", "/w/app", "/w/app/views/login.xaml").as_deref(),
      Some("views/login.xaml")
    );
  }

  // A struct named like a keyword gets a raw module name; one named like a path keyword can't
  // name a module at all.
  #[test]
  fn module_names_avoid_keywords() {
    let named = |name: &str| module_ident(&Ident::new(name, Span::call_site())).map(|ident| ident.to_string());
    assert_eq!(named("LoginView").expect("a module"), "login_view");
    assert_eq!(named("Type").expect("a module"), "r#type");
    assert!(named("Crate").is_err());
  }

  // Each structural problem is reported before the file system is read, so a site without a
  // source file still gets it.
  #[test]
  fn structural_problems_come_first() {
    let site = Site {
      source_file: None,
      manifest_dir: "/w/app".into(),
      out_dir: None,
    };
    let cases = [
      (
        quote!(),
        quote!(
          struct A;
        ),
        "`#[ui_class]` takes the view's path",
      ),
      (
        quote!(login),
        quote!(
          struct A;
        ),
        "`#[ui_class]` takes the view's path",
      ),
      (
        quote!("a.xaml"),
        quote!(
          struct A<T>(T);
        ),
        "`#[ui_class]` can't be used on a generic struct",
      ),
      (
        quote!("a.xaml"),
        quote!(
          struct A(u8);
        ),
        "`#[ui_class]` needs a struct with named fields",
      ),
      (
        quote!("a.xaml"),
        quote!(
          struct A {
            ui: u8,
          }
        ),
        "`ui` is added by `#[ui_class]`",
      ),
      (
        quote!("a.xaml"),
        quote!(
          struct A {
            child: u8,
          }
        ),
        "`child` is added by `#[ui_class]`",
      ),
    ];
    for (argument, item, message) in cases {
      let error = error(argument, item, &site);
      assert!(error.starts_with(message), "{error}");
    }
  }

  // Each problem with the view's file is reported, in the order the macro meets them.
  #[test]
  fn file_problems() {
    let item = || {
      quote!(
        struct LoginView;
      )
    };
    let no_source = Site {
      source_file: None,
      ..site("no-source", true)
    };
    let outside = Site {
      manifest_dir: "/w/other".into(),
      ..site("outside", true)
    };
    let no_out_dir = Site {
      out_dir: None,
      ..site("no-out-dir", true)
    };

    assert!(error(quote!("login.xaml"), item(), &no_source).starts_with("can't find the file this struct is in"));
    assert!(error(quote!("login.xaml"), item(), &outside).starts_with("`login.xaml` is outside the crate"));
    assert!(error(quote!("login.xaml"), item(), &no_out_dir).starts_with("this crate compiles no views"));
    assert_eq!(
      error(quote!("login.xaml"), item(), &site("missing", false)),
      "no compiled view for `src/views/login.xaml`: the file doesn't exist, or the build script's `compile_dir` doesn't cover it"
    );
  }
}
