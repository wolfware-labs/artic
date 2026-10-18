//! Keeps the crate's list of documents whole, so every Rust block in the repository's documents, the
//! Markdown files outside `crates/` and `.github/`, is compiled.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repository() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn markdown_files() -> Vec<String> {
  let output = Command::new("git")
    .args(["ls-files", "--", "*.md", ":!crates", ":!.github"])
    .current_dir(repository())
    .output()
    .expect("git runs");
  assert!(output.status.success(), "git ls-files failed");
  String::from_utf8(output.stdout)
    .expect("paths are UTF-8")
    .lines()
    .map(str::to_owned)
    .collect()
}

// A tracked Markdown file the crate doesn't include would never have its Rust compiled.
#[test]
fn every_markdown_file_is_included() {
  let lib = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")).expect("lib.rs reads");

  let missing: Vec<String> = markdown_files()
    .into_iter()
    .filter(|file| !lib.contains(&format!("include_str!(\"../../../{file}\")")))
    .collect();

  assert!(missing.is_empty(), "add these to crates/docs-check/src/lib.rs: {missing:?}");
}

// rustdoc compiles a fence with no language as Rust, so every opening fence names one.
#[test]
fn every_fence_names_a_language() {
  let mut untagged = Vec::new();
  for file in markdown_files() {
    let text = std::fs::read_to_string(repository().join(&file)).expect("document reads");
    let mut open = false;
    for (index, line) in text.lines().enumerate() {
      let Some(info) = line.trim_start().strip_prefix("```") else {
        continue;
      };
      if !open && info.trim().is_empty() {
        untagged.push(format!("{file}:{}", index + 1));
      }
      open = !open;
    }
  }

  assert!(untagged.is_empty(), "give these fences a language: {untagged:?}");
}
