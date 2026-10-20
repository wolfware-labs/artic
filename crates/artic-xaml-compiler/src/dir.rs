//! Compiling a directory of views for a build script.

use crate::compile_str;
use artic_registry::{Registry, RegistryError};
use artic_xaml::diagnostics::{Diagnostics, SourceMap};
use artic_xaml::namespaces::NamespaceMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

/// Why [`compile_dir`] stopped.
#[derive(Debug, thiserror::Error)]
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
  #[error("{}: {error}", .path.display())]
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

/// Compiles every `.xaml` file under `dir` into `$OUT_DIR/views/`, for a build script.
///
/// `dir` is read relative to the crate, where Cargo runs build scripts. Each view is written to
/// `$OUT_DIR/views/<path under dir>.rs`, and Cargo is told to rerun the script when the
/// directory or any view changes.
///
/// # Errors
///
/// Stops at the first view with errors, in sorted order, and returns its diagnostics; or returns
/// the I/O error, the registry's duplicates, or a missing `OUT_DIR`.
pub fn compile_dir(dir: impl AsRef<Path>, namespaces: &NamespaceMap) -> Result<(), CompileError> {
  let out = std::env::var_os("OUT_DIR").ok_or(CompileError::NoOutDir)?;
  let registry = Registry::from_link_time()?;
  let manifest = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
  let dir = dir.as_ref();
  let dir = manifest
    .as_deref()
    .and_then(|manifest| dir.strip_prefix(manifest).ok())
    .unwrap_or(dir);
  println!("cargo:rerun-if-changed={}", dir.display());
  for view in compile_into(dir, &Path::new(&out).join("views"), &registry, namespaces)? {
    println!("cargo:rerun-if-changed={}", view.display());
  }
  Ok(())
}

/// Compiles every view under `dir` into `out`, mirroring the layout, and returns the views read,
/// sorted. Stops at the first view with errors. This is [`compile_dir`]'s work without its
/// environment, for tests.
///
/// # Errors
///
/// As [`compile_dir`], except for `OUT_DIR` and the registry, which the caller provides.
#[doc(hidden)]
pub fn compile_into(
  dir: &Path,
  out: &Path,
  registry: &Registry,
  namespaces: &NamespaceMap,
) -> Result<Vec<PathBuf>, CompileError> {
  let views = find_views(dir)?;
  for view in &views {
    let relative = view.strip_prefix(dir).expect("a view found under its directory");
    let text = fs::read_to_string(view).map_err(io_error(view))?;
    let mut sources = SourceMap::new();
    let file = sources.add(shown(dir, relative), text);
    let rust = match compile_str(&sources, file, registry, namespaces) {
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

/// Every `.xaml` file under `dir`, in sorted order.
fn find_views(dir: &Path) -> Result<Vec<PathBuf>, CompileError> {
  let mut entries = fs::read_dir(dir)
    .and_then(|entries| {
      entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()
    })
    .map_err(io_error(dir))?;
  entries.sort();
  let mut views = Vec::new();
  for path in entries {
    if path.is_dir() {
      views.extend(find_views(&path)?);
    } else if path.extension().is_some_and(|extension| extension == "xaml") {
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
