// The paths generated code takes from the facade reach every crate that defines a UI type, so a
// view compiles in an app that depends only on the facade.

#![cfg(feature = "view-compiler")]

use artic::view::crate_paths;
use artic_registry::{UI_TYPES, crate_of};
use std::collections::BTreeSet;

// Every crate that defines a registered UI type or group has a facade path, so generated code
// reaches it in an app that depends only on the facade.
#[test]
fn defining_crates_have_facade_paths() {
  let paths = crate_paths();
  let unmapped: BTreeSet<_> = UI_TYPES
    .iter()
    .flat_map(|info| std::iter::once(info.rust_path).chain(info.groups.iter().map(|group| group.methods)))
    .map(crate_of)
    .filter(|krate| !paths.path_of(krate).starts_with("::artic::"))
    .collect();
  assert!(unmapped.is_empty(), "no facade path: {unmapped:?}");
}
