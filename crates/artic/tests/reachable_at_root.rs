// Generated code names a UI type, a group's methods trait and a property's value type as
// `<crate>::<Name>`, so each one must be reachable at its crate's root. `reachable!` imports every
// listed path, which compiles only if it is, and the tests check the list against the registry, so
// a new type that isn't re-exported or listed fails here by name.

use artic_registry::{PropertyInfo, UI_TYPES};
use std::collections::BTreeSet;

/// Imports each `crate::{names}` in a module named after the crate, and lists the paths.
macro_rules! reachable {
  ($($krate:ident::{$($name:ident),* $(,)?}),* $(,)?) => {
    $(
      #[allow(unused_imports, non_snake_case)]
      mod $krate {
        use ::$krate::{$($name),*};
      }
    )*

    /// Every `(crate, name)` listed above.
    const REACHABLE: &[(&str, &str)] = &[$($((stringify!($krate), stringify!($name)),)*)*];
  };
}

reachable!(
  artic_controls::{Border, PanelMethods, StackPanel, TextBlock, TextElement, UserControl},
  artic_element::{
    AppearanceMethods, Application, Chrome, Classes, Cursor, Desktop, IdentityMethods, InteractionMethods,
    LayoutMethods, SizeToContent, Surface, Visibility,
  },
  artic_layout::{
    CornerRadius, FontFamily, FontStyle, FontWeight, HorizontalAlignment, Orientation, TextAlignment, TextTrimming,
    TextWrapping, Thickness, Transform, VerticalAlignment,
  },
  artic_render::{BoxShadow, Brush},
);

/// Value types every crate reaches without a path: the standard library's.
const STANDARD: &[&str] = &[
  "bool",
  "char",
  "f32",
  "f64",
  "i8",
  "i16",
  "i32",
  "i64",
  "i128",
  "isize",
  "u8",
  "u16",
  "u32",
  "u64",
  "u128",
  "usize",
  "String",
  "NonZeroU32",
  "NonZeroUsize",
];

/// The crate (first segment) and name (last segment) of a Rust path.
fn crate_and_name(path: &str) -> (&str, &str) {
  let krate = path.split("::").next().unwrap_or(path);
  let name = path.rsplit("::").next().unwrap_or(path);
  (krate, name)
}

/// The type a property's `value_type` names, without `Option` or a path: `Option < Brush >` gives
/// `Brush`.
fn value_name(value_type: &str) -> &str {
  let inner = value_type
    .strip_prefix("Option")
    .map_or(value_type, |rest| rest.trim().trim_start_matches('<').trim_end_matches('>'));
  crate_and_name(inner.trim()).1
}

/// Every property of every registered type, groups included.
fn all_properties() -> impl Iterator<Item = &'static PropertyInfo> {
  UI_TYPES.iter().flat_map(|info| {
    info
      .properties
      .iter()
      .chain(info.groups.iter().flat_map(|group| group.properties))
  })
}

// Every UI type is reachable at its crate's root.
#[test]
fn ui_types_are_at_their_crate_root() {
  let missing: Vec<_> = UI_TYPES
    .iter()
    .map(|info| (crate_and_name(info.rust_path).0, info.name))
    .filter(|path| !REACHABLE.contains(path))
    .collect();
  assert!(missing.is_empty(), "not listed or not at the root: {missing:?}");
}

// Every group's methods trait is reachable at its crate's root.
#[test]
fn methods_traits_are_at_their_crate_root() {
  let missing: BTreeSet<_> = UI_TYPES
    .iter()
    .flat_map(|info| info.groups)
    .map(|group| crate_and_name(group.methods))
    .filter(|path| !REACHABLE.contains(path))
    .collect();
  assert!(missing.is_empty(), "not listed or not at the root: {missing:?}");
}

// Every property's value type, attached ones included, is a standard type or listed above.
#[test]
fn value_types_are_at_their_crate_root() {
  let attached = UI_TYPES
    .iter()
    .flat_map(|info| info.attached_properties)
    .map(|property| property.value_type);
  let missing: BTreeSet<_> = all_properties()
    .map(|property| property.value_type)
    .chain(attached)
    .map(value_name)
    .filter(|name| !STANDARD.contains(name) && !REACHABLE.iter().any(|(_, listed)| listed == name))
    .collect();
  assert!(missing.is_empty(), "not listed or not at the root: {missing:?}");
}
