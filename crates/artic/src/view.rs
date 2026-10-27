//! Compiling views to Rust, for any UI language: the paths generated code reaches artic's crates
//! by, and the build script's driver.

pub use artic_view_compiler::{CompileError, CratePaths, compile_dir};

/// The path generated code reaches each artic crate by: the facade module it's re-exported as,
/// such as `artic_controls` as `::artic::controls`. An app that renamed its `artic` dependency
/// sets each path again and compiles its views with its language's long form.
#[must_use]
pub fn crate_paths() -> CratePaths {
  let mut paths = CratePaths::new();
  paths.path("artic_controls", "::artic::controls");
  paths.path("artic_core", "::artic::core");
  paths.path("artic_element", "::artic::element");
  paths.path("artic_layout", "::artic::layout");
  paths.path("artic_render", "::artic::render");
  paths
}

#[cfg(test)]
mod tests {
  use super::*;

  // Each artic crate generated code names is reached through the facade module it's re-exported as.
  #[test]
  fn crate_paths_map_crates_to_facade_modules() {
    let paths = crate_paths();

    assert_eq!(paths.path_of("artic_controls"), "::artic::controls");
    assert_eq!(paths.path_of("artic_core"), "::artic::core");
    assert_eq!(paths.path_of("artic_element"), "::artic::element");
    assert_eq!(paths.path_of("artic_layout"), "::artic::layout");
    assert_eq!(paths.path_of("artic_render"), "::artic::render");
    assert_eq!(
      paths.crates_named().collect::<Vec<_>>(),
      [
        "artic_controls",
        "artic_core",
        "artic_element",
        "artic_layout",
        "artic_render"
      ]
    );
  }
}
