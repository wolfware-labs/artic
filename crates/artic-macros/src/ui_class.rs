use crate::crate_root::{ELEMENT, crate_root};
use crate::element::expand_struct;
use crate::members::attributes::parse_field_attributes;
use crate::ui_type::attributes::parse_type_attributes;
use artic_ident::{is_keyword, snake_case};
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::fs;
use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
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
      "`#[ui_class]` takes the view's path, such as `#[ui_class(\"login.ui\")]`",
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
    if let Some(span) = parse_field_attributes(field)?.and_then(|attributes| attributes.required) {
      return Err(Error::new(
        span,
        "a code type's own fields start at their defaults, since its view creates it from the named nodes alone; \
         `required` can't be used here",
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
  let name = ident.unraw().to_string();
  let message = format!("`{}` must name `", view.replace('{', "{{").replace('}', "}}"));
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
  let name = snake_case(&ident.unraw().to_string());
  match name.as_str() {
    "self" | "super" | "crate" => Err(Error::new_spanned(
      ident,
      format!("`{name}` can't name the module the view goes in; rename this struct"),
    )),
    _ if is_keyword(&name) => Ok(Ident::new_raw(&name, ident.span())),
    _ => Ok(Ident::new(&name, ident.span())),
  }
}

/// The view's path in the crate, checked against the site: the view is inside the crate and the
/// build script compiled it. Without the struct's file, which rust-analyzer doesn't give, it's the
/// one compiled view whose path ends with the argument's.
fn view_path(site: &Site, path: &LitStr) -> syn::Result<String> {
  let argument = path.value();
  let no_views = || {
    Error::new(
      path.span(),
      "this crate compiles no views: it has no build script, so it has no compiled view",
    )
  };
  let outside = |argument: &str| {
    Error::new(
      path.span(),
      format!("`{argument}` is outside the crate, so it has no compiled view"),
    )
  };
  let not_compiled = |view: &str| {
    Error::new(
      path.span(),
      format!("no compiled view for `{view}`: the file doesn't exist, or the build script's `compile_dir` doesn't cover it"),
    )
  };
  let Some(source_file) = &site.source_file else {
    let views = site.out_dir.as_ref().ok_or_else(no_views)?.join("views");
    let written = slashes(&argument);
    let (end, exact) = if is_absolute(&written) {
      let inside = crate_relative("", &site.manifest_dir.to_string_lossy(), &written).ok_or_else(|| outside(&argument))?;
      (inside, true)
    } else {
      let mut segments: Vec<&str> = Vec::new();
      for segment in written.split('/') {
        match segment {
          "" | "." => {}
          ".." => {
            segments.pop();
          }
          _ => segments.push(segment),
        }
      }
      (segments.join("/"), false)
    };
    let (stem, extension) = split_extension(&end);
    let compiled_end = format!("{stem}.rs");
    let found: Vec<String> = compiled_views(&views, "")
      .into_iter()
      .filter(|compiled| *compiled == compiled_end || (!exact && compiled.ends_with(&format!("/{compiled_end}"))))
      .map(|compiled| format!("{}{extension}", compiled.strip_suffix(".rs").unwrap_or(&compiled)))
      .collect();
    return match found.as_slice() {
      [] => Err(not_compiled(&argument)),
      [view] => Ok(view.clone()),
      [first @ .., last] => {
        let first: Vec<String> = first.iter().map(|view| format!("`{view}`")).collect();
        Err(Error::new(
          path.span(),
          format!(
            "several compiled views end with `{argument}`, {} and `{last}`, and this editor doesn't say which file the struct is in",
            first.join(", ")
          ),
        ))
      }
    };
  };
  let view = crate_relative(
    &source_file.to_string_lossy(),
    &site.manifest_dir.to_string_lossy(),
    &argument,
  )
  .ok_or_else(|| outside(&argument))?;
  let out_dir = site.out_dir.as_ref().ok_or_else(no_views)?;
  if !out_dir.join("views").join(with_rs_extension(&view)).is_file() {
    return Err(not_compiled(&view));
  }
  Ok(view)
}

/// The compiled views under `dir`, each as its path below it with `/` separators after `prefix`,
/// sorted.
fn compiled_views(dir: &Path, prefix: &str) -> Vec<String> {
  let Ok(entries) = fs::read_dir(dir) else {
    return Vec::new();
  };
  let mut views = Vec::new();
  for entry in entries.flatten() {
    let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
    let path = entry.path();
    if path.is_dir() {
      views.extend(compiled_views(&path, &format!("{name}/")));
    } else if name.ends_with(".rs") {
      views.push(name);
    }
  }
  views.sort();
  views
}

/// `path` with its last segment's extension replaced by `rs`, as `compile_dir` names its output.
fn with_rs_extension(path: &str) -> String {
  format!("{}.rs", split_extension(path).0)
}

/// `path` split before its last segment's extension, as `Path::extension` reads it: a leading
/// dot starts no extension.
fn split_extension(path: &str) -> (&str, &str) {
  let name_start = path.rfind('/').map_or(0, |slash| slash + 1);
  match path[name_start..].rfind('.') {
    Some(dot) if dot > 0 => path.split_at(name_start + dot),
    _ => (path, ""),
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
  use std::ops::{Deref, DerefMut};

  /// A site whose `OUT_DIR` is a temporary directory holding `compiled` under `views/`, removed
  /// when the site drops.
  struct TestSite {
    site: Site,
    dir: PathBuf,
  }

  impl TestSite {
    fn new(name: &str, source_file: Option<&str>, compiled: &[&str]) -> TestSite {
      let dir = std::env::temp_dir().join(format!("artic-ui-class-{name}-{}", std::process::id()));
      for path in compiled {
        let path = dir.join("views").join(path);
        fs::create_dir_all(path.parent().expect("a directory")).expect("a temporary directory");
        fs::write(path, "").expect("a compiled view");
      }
      let site = Site {
        source_file: source_file.map(Into::into),
        manifest_dir: "/w/app".into(),
        out_dir: Some(dir.clone()),
      };
      TestSite { site, dir }
    }
  }

  impl Deref for TestSite {
    type Target = Site;

    fn deref(&self) -> &Site {
      &self.site
    }
  }

  impl DerefMut for TestSite {
    fn deref_mut(&mut self) -> &mut Site {
      &mut self.site
    }
  }

  impl Drop for TestSite {
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.dir);
    }
  }

  /// A site for `src/views/login.rs` in the crate at `/w/app`, whose build script wrote
  /// `$OUT_DIR/views/src/views/login.rs` unless `compiled` is false.
  fn site(name: &str, compiled: bool) -> TestSite {
    let views: &[&str] = if compiled { &["src/views/login.rs"] } else { &[] };
    TestSite::new(name, Some("/w/app/src/views/login.rs"), views)
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

    let expanded = expand(quote!("login.ui"), item, &site("whole", true)).expect("valid");

    let file: syn::File = syn::parse2(expanded).expect("parses");
    insta::assert_snapshot!(prettyplease::unparse(&file));
  }

  // A path from a Windows build, with backslashes and the verbatim prefix, gives the same view.
  #[test]
  fn windows_paths_use_slashes() {
    assert_eq!(
      crate_relative(r"\\?\C:\w\app\src\views\login.rs", r"C:\w\app", "login.ui").as_deref(),
      Some("src/views/login.ui")
    );
  }

  // `.` and `..` segments are resolved, as the file system would.
  #[test]
  fn parent_segments_resolve() {
    assert_eq!(
      crate_relative("/w/app/src/views/login.rs", "/w/app/", "./../shared/login.ui").as_deref(),
      Some("src/shared/login.ui")
    );
  }

  // A view outside the crate, by `..` or by an absolute path, has no path in the crate.
  #[test]
  fn outside_the_crate_has_no_path() {
    assert_eq!(crate_relative("/w/app/src/login.rs", "/w/app", "../../login.ui"), None);
    assert_eq!(crate_relative("/w/app/src/login.rs", "/w/app", "/w/other/login.ui"), None);
    assert_eq!(crate_relative("/w/application/src/login.rs", "/w/app", "login.ui"), None);
  }

  // An absolute path inside the crate is the view at that path.
  #[test]
  fn absolute_path_inside_the_crate() {
    assert_eq!(
      crate_relative("/w/app/src/login.rs", "/w/app", "/w/app/views/login.ui").as_deref(),
      Some("views/login.ui")
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
        quote!("a.ui"),
        quote!(
          struct A<T>(T);
        ),
        "`#[ui_class]` can't be used on a generic struct",
      ),
      (
        quote!("a.ui"),
        quote!(
          struct A(u8);
        ),
        "`#[ui_class]` needs a struct with named fields",
      ),
      (
        quote!("a.ui"),
        quote!(
          struct A {
            ui: u8,
          }
        ),
        "`ui` is added by `#[ui_class]`",
      ),
      (
        quote!("a.ui"),
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
    let item = || quote! { struct LoginView; };
    let mut outside = site("outside", true);
    outside.manifest_dir = "/w/other".into();
    let mut no_out_dir = site("no-out-dir", true);
    no_out_dir.out_dir = None;

    assert!(error(quote!("login.ui"), item(), &outside).starts_with("`login.ui` is outside the crate"));
    assert!(error(quote!("login.ui"), item(), &no_out_dir).starts_with("this crate compiles no views"));
    assert_eq!(
      error(quote!("login.ui"), item(), &site("missing", false)),
      "no compiled view for `src/views/login.ui`: the file doesn't exist, or the build script's `compile_dir` doesn't cover it"
    );
  }

  /// A site with no source file, as rust-analyzer expands macros, whose build script wrote each
  /// of `compiled` under `$OUT_DIR/views`.
  fn editor_site(name: &str, compiled: &[&str]) -> TestSite {
    TestSite::new(&format!("editor-{name}"), None, compiled)
  }

  // Without the struct's file, the view is the one compiled view whose path ends with the
  // argument's: a `..` after a segment goes back over it, and a leading one is left out.
  #[test]
  fn editor_finds_the_compiled_view_by_its_end() {
    let site = editor_site("one", &["src/views/login.rs", "src/views/card.rs", "src/old_login.rs"]);

    assert_eq!(view_path(&site, &literal("login.ui")).expect("found"), "src/views/login.ui");
    assert_eq!(
      view_path(&site, &literal("../views/login.ui")).expect("found"),
      "src/views/login.ui"
    );
    assert_eq!(
      view_path(&site, &literal("card/../login.ui")).expect("found"),
      "src/views/login.ui"
    );
    assert_eq!(
      view_path(&site, &literal(".//login.ui")).expect("found"),
      "src/views/login.ui"
    );
  }

  // A file's extension starts at the last dot of its name, unless that dot starts the name.
  #[test]
  fn extension_is_read_from_the_last_name() {
    assert_eq!(split_extension("views/login.ui"), ("views/login", ".ui"));
    assert_eq!(split_extension("v1.2/card"), ("v1.2/card", ""));
    assert_eq!(split_extension("a/.ui"), ("a/.ui", ""));
    assert_eq!(split_extension(".ui"), (".ui", ""));
  }

  // A path is absolute from a root, or from a drive letter followed by `:/`.
  #[test]
  fn absolute_paths_start_at_a_root_or_a_drive() {
    assert!(is_absolute("/w/app"));
    assert!(is_absolute("C:/w/app"));
    assert!(!is_absolute("w/app"));
    assert!(!is_absolute("C:"));
    assert!(!is_absolute("C:w"));
    assert!(!is_absolute("Cx/w"));
    assert!(!is_absolute("1:/w"));
  }

  // Without the struct's file, no compiled view or several with the argument's end are reported.
  #[test]
  fn editor_reports_no_view_or_several() {
    let site = editor_site("several", &["src/a/login.rs", "src/b/login.rs"]);

    assert_eq!(
      view_path(&site, &literal("card.ui")).expect_err("none").to_string(),
      "no compiled view for `card.ui`: the file doesn't exist, or the build script's `compile_dir` doesn't cover it"
    );
    assert_eq!(
      view_path(&site, &literal("login.ui")).expect_err("several").to_string(),
      "several compiled views end with `login.ui`, `src/a/login.ui` and `src/b/login.ui`, and this editor doesn't say which file the struct is in"
    );
  }

  fn literal(text: &str) -> LitStr {
    LitStr::new(text, Span::call_site())
  }

  // Without the struct's file, an argument without an extension, or with dots in its directories,
  // ends where `include_str!` would read it, and an absolute one is the view at that path.
  #[test]
  fn editor_reads_extensions_and_absolute_paths_as_rustc_does() {
    let site = editor_site("extensions", &["src/views/login.rs", "src/v1.2/card.rs"]);

    assert_eq!(
      view_path(&site, &literal("../views/login")).expect("found"),
      "src/views/login"
    );
    assert_eq!(view_path(&site, &literal("../v1.2/card")).expect("found"), "src/v1.2/card");
    assert_eq!(
      view_path(&site, &literal("/w/app/src/views/login.ui")).expect("found"),
      "src/views/login.ui"
    );
    assert_eq!(
      view_path(&site, &literal("/w/app/src/views/card/./../login.ui")).expect("found"),
      "src/views/login.ui"
    );
  }

  // A view path with braces is text in the assertion's message, not a format argument.
  #[test]
  fn braces_in_the_view_path_are_escaped_in_the_message() {
    let site = TestSite::new("braces", Some("/w/app/src/views/login.rs"), &["src/views/{a}.rs"]);

    let expanded = expand(quote!("{a}.ui"), quote! { struct LoginView; }, &site).expect("valid");

    assert!(
      expanded.to_string().contains("\"`src/views/{{a}}.ui` must name `\""),
      "{expanded}"
    );
  }

  // A struct named with a raw identifier gets the module and the code type of its plain name.
  #[test]
  fn raw_struct_name_is_read_without_its_prefix() {
    let expanded = expand(quote!("login.ui"), quote! { struct r#LoginView; }, &site("raw", true)).expect("valid");

    let text = expanded.to_string();
    assert!(text.contains("mod login_view"), "{text}");
    assert!(text.contains("\"::\" , \"LoginView\""), "{text}");
  }

  // A code type's own fields start at their defaults, since the compiled view calls `new` with
  // the named nodes only, so one of them can't be required.
  #[test]
  fn own_required_field_is_reported() {
    let item = quote! {
      struct LoginView {
        #[ui(required)]
        session: u32,
      }
    };

    assert!(
      error(quote!("login.ui"), item, &site("required", true)).starts_with("a code type's own fields start at their defaults"),
    );
  }
}
