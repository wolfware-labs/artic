//! Keeps the crate's list of documents whole, so every Rust block in the repository's documents, the
//! Markdown files outside `crates/` and `.github/`, is compiled.

mod common;

use common::repository;
use std::path::Path;
use std::process::Command;

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
    untagged.extend(untagged_fences(&text).into_iter().map(|line| format!("{file}:{line}")));
  }

  assert!(untagged.is_empty(), "give these fences a language: {untagged:?}");
}

// Tildes, longer runs and quoted fences, nested quotes included, open blocks too, and only a run as long closes one.
#[test]
fn untagged_fences_follow_commonmark_runs() {
  let text = "~~~~\n```\n~~~~\n> ```\n> x\n> ```\n````rust\n```\n````\n```text\n```\n> > ```\n> > x\n> > ```\n";

  assert_eq!(untagged_fences(text), [1, 4, 12]);
}

/// The 1-based lines of the opening fences that name no language.
fn untagged_fences(text: &str) -> Vec<usize> {
  let mut untagged = Vec::new();
  let mut open: Option<(char, usize)> = None;
  for (index, line) in text.lines().enumerate() {
    let line = line.trim_start_matches(|character: char| character == '>' || character.is_whitespace());
    let Some(marker) = line.chars().next().filter(|marker| *marker == '`' || *marker == '~') else {
      continue;
    };
    let run = line.chars().take_while(|character| *character == marker).count();
    if run < 3 {
      continue;
    }
    let info = line[run..].trim();
    match open {
      None => {
        if info.is_empty() {
          untagged.push(index + 1);
        }
        open = Some((marker, run));
      }
      Some((open_marker, open_run)) if marker == open_marker && run >= open_run && info.is_empty() => open = None,
      Some(_) => {}
    }
  }
  untagged
}
