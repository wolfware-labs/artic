//! Compiling a directory of views for a build script.

use crate::CratePaths;
use crate::compile_str;
use artic_core::error::Chain;
use artic_diagnostics::{Diagnostic, Diagnostics, Severity, SourceMap, Span};
use artic_registry::{Registry, RegistryError};
use artic_view::Language;
use std::fmt::{self, Debug, Display, Formatter};
use std::path::{Component, Path, PathBuf};
use std::{fs, io};

/// Why [`compile_dir`] stopped.
#[derive(thiserror::Error)]
pub enum CompileError {
  /// Views have errors.
  #[error("{}", .diagnostics.render(.sources))]
  Diagnostics {
    /// The views' sources, which the diagnostics point into.
    sources: SourceMap,
    /// Every problem in every view, view by view.
    diagnostics: Diagnostics,
  },
  /// A file or directory couldn't be read or written.
  #[error("{} couldn't be read or written", .path.display())]
  Io {
    /// The file or directory.
    path: PathBuf,
    /// What failed.
    #[source]
    error: io::Error,
  },
  /// The linked crates register two types at one Rust path, or another duplicate.
  #[error(transparent)]
  Registry(#[from] RegistryError),
  /// The directory's or a view's path holds a control character, such as a newline, which would
  /// start a line of its own in Cargo's `rerun-if-changed` output.
  #[error("the path {path:?} has a control character")]
  ControlCharacter {
    /// The directory's or the view's path.
    path: PathBuf,
  },
  /// `OUT_DIR` isn't set, so this isn't running in a build script.
  #[error("`OUT_DIR` isn't set: call `compile_dir` from a build script")]
  NoOutDir,
}

/// Shows the error as its `Display` does, then each cause, so a build script that unwraps it, or
/// returns it from `main`, prints the rendered diagnostics, or the message and its causes, rather
/// than the source map.
impl Debug for CompileError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    Display::fmt(&Chain(self), f)
  }
}

impl CompileError {
  /// Reports the error as Cargo shows a build script's errors, and ends the build script: the
  /// rendered diagnostics, or the message and its causes, on stderr, which Cargo prints under the
  /// failed script, then a one-line [`summary`](CompileError::summary) as `cargo::error=`, and
  /// exit code 1. A build script ends with it when [`compile_dir`] fails:
  ///
  /// ```no_run
  /// # use artic_view_compiler::{CratePaths, compile_dir};
  /// # fn language() -> &'static dyn artic_view::Language { unimplemented!() }
  /// compile_dir("src/views", language(), &CratePaths::new()).unwrap_or_else(|error| error.exit());
  /// ```
  pub fn exit(self) -> ! {
    eprintln!("{self:?}");
    println!("cargo::error={}", self.summary());
    std::process::exit(1)
  }

  /// One line saying what failed: how many errors in how many views, or the error's message with
  /// its lines joined by `; `, since a registry error lists one duplicate per line.
  #[must_use]
  pub fn summary(&self) -> String {
    let CompileError::Diagnostics { diagnostics, .. } = self else {
      return self.to_string().lines().collect::<Vec<_>>().join("; ");
    };
    let errors: Vec<_> = diagnostics
      .iter()
      .filter(|diagnostic| diagnostic.severity == Severity::Error)
      .collect();
    let mut views: Vec<_> = errors.iter().map(|diagnostic| diagnostic.span.source).collect();
    views.dedup();
    let plural = |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    format!("{} in {}", plural(errors.len(), "error"), plural(views.len(), "view"))
  }
}

/// Compiles every file of `language` under `dir` into `$OUT_DIR/views/`, for a build script.
///
/// `dir` is read relative to the crate, where Cargo runs build scripts. Each view is written at
/// its path in the crate under `$OUT_DIR/views/`, such as `$OUT_DIR/views/src/views/login.rs` for
/// `src/views/login.ui`, which is where a code type's macro finds it; a `dir` outside the crate
/// writes each view at its path under `dir`. `$OUT_DIR/views/` is replaced, so a view removed
/// since the last run leaves nothing behind, and a build script with views in several
/// directories calls it once, on a directory that holds them all. Cargo is told to rerun the
/// script when the directory or any view changes. A symlink to a directory isn't followed.
///
/// The compiler finds types in the registry of the build script itself, which holds the crates it
/// links. So `build.rs` names each control crate its views use with `use <crate> as _;`, and
/// that crate is a build-dependency as well as a dependency: the generated code compiles in the
/// app and names it too.
///
/// An app usually calls it through the facade, as `artic::view::compile_dir`, with the facade's
/// `artic::view::crate_paths()`, or through its language's short form.
///
/// # Errors
///
/// Returns the diagnostics of every view with errors, together, view by view in sorted order; or
/// the I/O error, the directory or a view whose path holds a control character, the registry's
/// duplicates, or a missing `OUT_DIR`.
pub fn compile_dir(dir: impl AsRef<Path>, language: &(impl Language + ?Sized), paths: &CratePaths) -> Result<(), CompileError> {
  let out = std::env::var_os("OUT_DIR").ok_or(CompileError::NoOutDir)?;
  let registry = Registry::from_link_time()?;
  let manifest = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
  let dir = dir.as_ref();
  let dir = manifest
    .as_deref()
    .and_then(|manifest| dir.strip_prefix(manifest).ok())
    .unwrap_or(dir);
  let views = compile_into(dir, &Path::new(&out).join("views"), language, &registry, paths)?;
  println!("cargo:rerun-if-changed={}", dir.display());
  for view in views {
    println!("cargo:rerun-if-changed={}", view.display());
  }
  Ok(())
}

/// Compiles every file of `language` under `dir` into `out`, each at the path its header names it
/// by, and returns the views read, sorted. `out` is replaced, so a view removed since the last
/// run leaves no output behind. Every view is compiled, and the problems of all of them are
/// returned together, so a build shows every broken view at once.
///
/// This is [`compile_dir`]'s work with every input passed in, for a tool that isn't a build
/// script: it reads no environment and prints nothing for Cargo.
///
/// # Errors
///
/// As [`compile_dir`], except for `OUT_DIR` and the registry, which the caller provides.
pub fn compile_into(
  dir: &Path,
  out: &Path,
  language: &(impl Language + ?Sized),
  registry: &Registry,
  paths: &CratePaths,
) -> Result<Vec<PathBuf>, CompileError> {
  checked(dir)?;
  let views = find_views(dir, language.extension())?;
  if out.exists() {
    fs::remove_dir_all(out).map_err(io_error(out))?;
  }
  let mut sources = SourceMap::new();
  let mut problems = Diagnostics::new();
  for view in &views {
    let relative = view.strip_prefix(dir).expect("a view found under its directory");
    let text = fs::read_to_string(view).map_err(io_error(view))?;
    let shown = shown(dir, relative);
    let file = sources.add(shown.clone(), text);
    let rust = match compile_str(&sources, file, language, registry, paths) {
      Ok(rust) => rust,
      Err(diagnostics) => {
        if !diagnostics.has_errors() {
          problems.push(Diagnostic::error(
            Span::new(file, 0, 0),
            "this view wasn't compiled, and its language reported no error",
          ));
        }
        problems.extend(diagnostics);
        continue;
      }
    };
    let target = out.join(shown).with_extension("rs");
    if let Some(parent) = target.parent() {
      fs::create_dir_all(parent).map_err(io_error(parent))?;
    }
    fs::write(&target, rust).map_err(io_error(&target))?;
  }
  if !problems.is_empty() {
    return Err(CompileError::Diagnostics {
      sources,
      diagnostics: problems,
    });
  }
  Ok(views)
}

/// Every file under `dir` with `extension`, in sorted order, or an error for the first whose path
/// holds a control character. A symlink to a directory isn't followed, so a link back to an
/// ancestor can't loop.
fn find_views(dir: &Path, extension: &str) -> Result<Vec<PathBuf>, CompileError> {
  let mut entries = fs::read_dir(dir)
    .and_then(|entries| {
      entries
        .map(|entry| entry.and_then(|entry| Ok((entry.path(), entry.file_type()?))))
        .collect::<io::Result<Vec<_>>>()
    })
    .map_err(io_error(dir))?;
  entries.sort_by(|(one, _), (other, _)| one.cmp(other));
  let mut views = Vec::new();
  for (path, kind) in entries {
    if kind.is_dir() {
      views.extend(find_views(&path, extension)?);
    } else if path.extension().is_some_and(|found| found == extension) {
      checked(&path)?;
      views.push(path);
    }
  }
  Ok(views)
}

/// The path a view is named by in its header and diagnostics, and written at under the output:
/// its path in the crate when `dir` is in the crate, a relative path without `..`; otherwise its
/// path under `dir`, so no absolute path reaches the output and no output leaves its directory.
fn shown(dir: &Path, relative: &Path) -> PathBuf {
  let in_crate = dir.is_relative()
    && dir
      .components()
      .all(|component| matches!(component, Component::Normal(_) | Component::CurDir));
  if in_crate {
    dir
      .components()
      .filter(|component| matches!(component, Component::Normal(_)))
      .collect::<PathBuf>()
      .join(relative)
  } else {
    relative.to_owned()
  }
}

/// Refuses `path` when it holds a control character: Cargo reads a build script's output a line at
/// a time, so a newline in a path printed there starts a directive of its own.
fn checked(path: &Path) -> Result<(), CompileError> {
  if path.to_string_lossy().chars().any(char::is_control) {
    return Err(CompileError::ControlCharacter { path: path.to_owned() });
  }
  Ok(())
}

/// Turns an I/O error into a [`CompileError`] naming `path`.
fn io_error(path: &Path) -> impl FnOnce(io::Error) -> CompileError + '_ {
  move |error| CompileError::Io {
    path: path.to_owned(),
    error,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_registry::Duplicate;
  use pretty_assertions::assert_eq;

  // A registry error lists one duplicate per line; its summary joins them, since Cargo shows one
  // line of `cargo::error=`.
  #[test]
  fn summary_is_one_line() {
    let error = CompileError::Registry(RegistryError {
      duplicates: vec![Duplicate::RustPath { path: "a::B" }, Duplicate::RustPath { path: "a::C" }],
    });

    assert_eq!(
      error.summary(),
      "two types are registered for the Rust path `a::B`; two types are registered for the Rust path `a::C`"
    );
  }

  // A directory in the crate names each view by its path in the crate; one outside it, absolute
  // or reached through `..`, by its path under the directory.
  #[test]
  fn views_are_named_by_their_path_in_the_crate() {
    let login = Path::new("login.ui");
    for (dir, shown_as) in [
      ("src/views", "src/views/login.ui"),
      ("./src/views", "src/views/login.ui"),
      ("src/./views", "src/views/login.ui"),
      ("../shared/views", "login.ui"),
      ("src/../views", "login.ui"),
      ("/abs/views", "login.ui"),
    ] {
      assert_eq!(shown(Path::new(dir), login), PathBuf::from(shown_as), "{dir}");
    }
  }
}
