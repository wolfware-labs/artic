//! What both test binaries need: where the repository is.

use std::path::{Path, PathBuf};

pub fn repository() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
