//! Compiling a directory of views for a build script.

use crate::CratePaths;
use crate::compile_str;
use artic_core::error::Chain;
use artic_diagnostics::{Diagnostics, SourceMap};
use artic_registry::{Registry, RegistryError};
use artic_view::Language;
use std::fmt::{self, Debug, Display, Formatter};
use std::path::{Path, PathBuf};
use std::{fs, io};

/// Why [`compile_dir`] stopped.
#[derive(thiserror::Error)]
pub enum CompileError {
  /// A view has errors.
  #[error("{}", .diagnostics.render(.sources))]
  Diagnostics {
    /// The view's source, which the diagnostics point into.
    sources: SourceMap,
    /// Every problem in the view.
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

/// Compiles every file of `language` under `dir` into `$OUT_DIR/views/`, for a build script.
///
/// `dir` is read relative to the crate, where Cargo runs build scripts. Each view is written to
/// `$OUT_DIR/views/<path under dir>.rs`, replacing what an earlier run wrote there, and Cargo is
/// told to rerun the script when the directory or any view changes. A symlink to a directory
/// isn't followed.
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
/// Stops at the first view with errors, in sorted order, and returns its diagnostics; or returns
/// the I/O error, the registry's duplicates, or a missing `OUT_DIR`.
pub fn compile_dir(dir: impl AsRef<Path>, language: &(impl Language + ?Sized), paths: &CratePaths) -> Result<(), CompileError> {
  let out = std::env::var_os("OUT_DIR").ok_or(CompileError::NoOutDir)?;
  let registry = Registry::from_link_time()?;
  let manifest = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
  let dir = dir.as_ref();
  let dir = manifest
    .as_deref()
    .and_then(|manifest| dir.strip_prefix(manifest).ok())
    .unwrap_or(dir);
  println!("cargo:rerun-if-changed={}", dir.display());
  for view in compile_into(dir, &Path::new(&out).join("views"), language, &registry, paths)? {
    println!("cargo:rerun-if-changed={}", view.display());
  }
  Ok(())
}

/// Compiles every file of `language` under `dir` into `out`, mirroring the layout, and returns the
/// views read, sorted. `out` is replaced, so a view removed since the last run leaves no output
/// behind. Stops at the first view with errors.
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
  let views = find_views(dir, language.extension())?;
  if out.exists() {
    fs::remove_dir_all(out).map_err(io_error(out))?;
  }
  for view in &views {
    let relative = view.strip_prefix(dir).expect("a view found under its directory");
    let text = fs::read_to_string(view).map_err(io_error(view))?;
    let mut sources = SourceMap::new();
    let file = sources.add(shown(dir, relative), text);
    let rust = match compile_str(&sources, file, language, registry, paths) {
      Ok(rust) => rust,
      Err(diagnostics) => return Err(CompileError::Diagnostics { sources, diagnostics }),
    };
    let target = out.join(relative).with_extension("rs");
    if let Some(parent) = target.parent() {
      fs::create_dir_all(parent).map_err(io_error(parent))?;
    }
    fs::write(&target, rust).map_err(io_error(&target))?;
  }
  Ok(views)
}

/// Every file under `dir` with `extension`, in sorted order. A symlink to a directory isn't
/// followed, so a link back to an ancestor can't loop.
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
      views.push(path);
    }
  }
  Ok(views)
}

/// The path a view is named by in its header and diagnostics: under `dir` as the build script
/// wrote it when that is relative, or relative to `dir` otherwise, so no absolute path reaches
/// the output.
fn shown(dir: &Path, relative: &Path) -> PathBuf {
  if dir.is_relative() {
    dir.join(relative)
  } else {
    relative.to_owned()
  }
}

/// Turns an I/O error into a [`CompileError`] naming `path`.
fn io_error(path: &Path) -> impl FnOnce(io::Error) -> CompileError + '_ {
  move |error| CompileError::Io {
    path: path.to_owned(),
    error,
  }
}
