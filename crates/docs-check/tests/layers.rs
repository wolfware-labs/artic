//! Checks the crate graph against `docs/architecture.md`: every edge Cargo has is in its tables and
//! every edge there is in Cargo, each crate's layer is its depth, dev-dependencies break the layers
//! only where the exceptions table says so, nothing in the layers reaches an adapter, and the facade
//! depends only on what it re-exports.

mod common;

use common::repository;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

const ARCHITECTURE: &str = "docs/architecture.md";
const SEAMS: &str = "| Seam | Interface | v1 |";
const LAYERS: &str = "| Layer | Crates | Role |";
const DEPENDENCIES: &str = "| Crate | Depends on | Dev-dependencies |";
const EXCEPTIONS: &str = "| Crate | Dev-depends on | Why |";
const README_LAYERS: &str = "| Layer | Crate | Responsibility |";

fn read(path: &str) -> String {
  std::fs::read_to_string(repository().join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// One edge as a table cell writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edge {
  name: String,
  planned: bool,
  feature: Option<String>,
}

/// The rows of the table in `document` (read from `path`) whose header line is `header`, each a list
/// of trimmed cells.
fn table(document: &str, path: &str, header: &str) -> Vec<Vec<String>> {
  let mut lines = document.lines().map(str::trim).skip_while(|line| *line != header).skip(2);
  let width = cells(header).len();
  let mut rows = Vec::new();
  for line in lines.by_ref() {
    if !line.starts_with('|') {
      break;
    }
    let row = cells(line);
    assert!(
      row.len() == width,
      "{path}: `{line}` has {} cells; the table headed {header} has {width}",
      row.len()
    );
    rows.push(row);
  }
  assert!(!rows.is_empty(), "{path} has no table headed {header}, or it has no rows");
  let next = lines.find(|line| !line.is_empty());
  assert!(
    !next.is_some_and(|line| line.starts_with('|')),
    "blank line inside the table headed {header} in {path}"
  );
  rows
}

fn cells(line: &str) -> Vec<String> {
  line.trim_matches('|').split('|').map(|cell| cell.trim().to_owned()).collect()
}

/// Every backticked name in `text`.
fn names(text: &str) -> Vec<String> {
  text.split('`').skip(1).step_by(2).map(str::to_owned).collect()
}

/// The edges a dependency cell lists: `none`, or backticked crates separated by commas, each
/// optionally followed by `(planned)`, `(planned #N)`, `(feature `f`)` or both, separated by `, `.
fn cell(text: &str) -> Result<Vec<Edge>, String> {
  if text == "none" {
    return Ok(Vec::new());
  }
  let mut items = Vec::new();
  let mut depth = 0;
  let mut start = 0;
  for (index, character) in text.char_indices() {
    match character {
      '(' => depth += 1,
      ')' => depth -= 1,
      ',' if depth == 0 => {
        items.push(&text[start..index]);
        start = index + 1;
      }
      _ => {}
    }
  }
  items.push(&text[start..]);
  items.into_iter().map(|item| edge(item.trim())).collect()
}

fn edge(item: &str) -> Result<Edge, String> {
  let bad = || format!("`{item}` isn't a crate in backticks with an optional (planned #N, feature `f`)");
  let rest = item.strip_prefix('`').ok_or_else(bad)?;
  let (name, rest) = rest.split_once('`').ok_or_else(bad)?;
  let rest = rest.trim();
  let mut edge = Edge {
    name: name.to_owned(),
    planned: false,
    feature: None,
  };
  if rest.is_empty() {
    return Ok(edge);
  }
  let qualifiers = rest
    .strip_prefix('(')
    .and_then(|rest| rest.strip_suffix(')'))
    .ok_or_else(bad)?;
  for qualifier in qualifiers.split(", ") {
    if qualifier == "planned" {
      edge.planned = true;
    } else if let Some(issue) = qualifier.strip_prefix("planned #") {
      if issue.is_empty() || !issue.chars().all(|digit| digit.is_ascii_digit()) {
        return Err(bad());
      }
      edge.planned = true;
    } else if let Some(feature) = qualifier.strip_prefix("feature `").and_then(|rest| rest.strip_suffix('`')) {
      edge.feature = Some(feature.to_owned());
    } else {
      return Err(bad());
    }
  }
  Ok(edge)
}

/// A workspace package as `cargo metadata` describes it.
struct Package {
  /// Whether its `publish` lets it be published: unset, or a non-empty list of registries.
  publish: bool,
  /// Workspace dependencies by kind (`normal` takes build dependencies too), each with whether
  /// it's optional.
  dependencies: BTreeMap<&'static str, BTreeMap<String, bool>>,
  features: BTreeMap<String, Vec<String>>,
}

fn metadata() -> BTreeMap<String, Package> {
  let output = Command::new(env!("CARGO"))
    .args(["metadata", "--format-version", "1", "--no-deps"])
    .current_dir(repository())
    .output()
    .expect("cargo metadata runs");
  assert!(output.status.success(), "cargo metadata failed");
  let json: Value = serde_json::from_slice(&output.stdout).expect("cargo metadata prints JSON");
  let packages = json["packages"].as_array().expect("packages");
  let members: BTreeSet<&str> = packages.iter().filter_map(|package| package["name"].as_str()).collect();
  packages
    .iter()
    .map(|package| {
      let name = package["name"].as_str().expect("a name").to_owned();
      let mut dependencies: BTreeMap<&'static str, BTreeMap<String, bool>> = BTreeMap::new();
      for dependency in package["dependencies"].as_array().expect("dependencies") {
        let target = dependency["name"].as_str().expect("a dependency name");
        if !members.contains(target) || target == name {
          continue;
        }
        let kind = if dependency["kind"].as_str() == Some("dev") {
          "dev"
        } else {
          "normal"
        };
        let optional = dependency["optional"].as_bool().unwrap_or(false);
        dependencies.entry(kind).or_default().insert(target.to_owned(), optional);
      }
      let features = package["features"]
        .as_object()
        .expect("features")
        .iter()
        .map(|(feature, enables)| {
          let enables = enables.as_array().expect("a feature list");
          (
            feature.clone(),
            enables.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
          )
        })
        .collect();
      let publish = package["publish"].as_array().is_none_or(|registries| !registries.is_empty());
      (
        name,
        Package {
          publish,
          dependencies,
          features,
        },
      )
    })
    .collect()
}

/// The architecture's tables, read once.
struct Architecture {
  seams: BTreeMap<String, Vec<String>>,
  layers: BTreeMap<String, Option<usize>>,
  depends: BTreeMap<String, Vec<Edge>>,
  dev: BTreeMap<String, Vec<Edge>>,
  exceptions: Vec<(String, Vec<String>)>,
}

impl Architecture {
  /// Reads the tables and checks their shape against the workspace: the three Seams rows each name
  /// a crate, and every crate a table names is a workspace member in a layer.
  fn read(packages: &BTreeMap<String, Package>) -> Architecture {
    let document = read(ARCHITECTURE);
    let seams = table(&document, ARCHITECTURE, SEAMS)
      .into_iter()
      .map(|row| (row[0].clone(), names(&row[2])))
      .collect();
    let mut layers = BTreeMap::new();
    for row in table(&document, ARCHITECTURE, LAYERS) {
      let layer = if row[0] == "outside" {
        None
      } else {
        Some(
          row[0]
            .parse()
            .unwrap_or_else(|_| panic!("layer `{}` isn't a number or `outside`", row[0])),
        )
      };
      for name in names(&row[1]) {
        assert!(layers.insert(name.clone(), layer).is_none(), "{name} is in two Layers rows");
      }
    }
    let mut depends = BTreeMap::new();
    let mut dev = BTreeMap::new();
    let mut problems = Vec::new();
    for row in table(&document, ARCHITECTURE, DEPENDENCIES) {
      let crate_name = names(&row[0]).concat();
      for (column, edges) in [(&row[1], &mut depends), (&row[2], &mut dev)] {
        match cell(column) {
          Ok(list) => {
            if edges.insert(crate_name.clone(), list).is_some() {
              problems.push(format!("{crate_name} has two dependency rows"));
            }
          }
          Err(problem) => problems.push(format!("{crate_name}: {problem}")),
        }
      }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let exceptions = table(&document, ARCHITECTURE, EXCEPTIONS)
      .into_iter()
      .map(|row| {
        let crate_name = if row[0] == "any crate" {
          row[0].clone()
        } else {
          names(&row[0]).concat()
        };
        (crate_name, names(&row[1]))
      })
      .collect();
    let architecture = Architecture {
      seams,
      layers,
      depends,
      dev,
      exceptions,
    };
    architecture.validate(packages);
    architecture
  }

  fn validate(&self, packages: &BTreeMap<String, Package>) {
    let mut problems = Vec::new();
    let rows: Vec<&str> = self.seams.keys().map(String::as_str).collect();
    if rows != ["Host", "Renderer", "UI language"] {
      problems.push(format!(
        "the Seams table has rows {rows:?}, not Host, Renderer and UI language"
      ));
    }
    for (seam, crates) in &self.seams {
      if crates.is_empty() {
        problems.push(format!("the Seams row {seam} names no crate in backticks"));
      }
    }
    let named = self
      .seams
      .values()
      .flatten()
      .chain(
        self
          .depends
          .values()
          .chain(self.dev.values())
          .flatten()
          .map(|edge| &edge.name),
      )
      .chain(self.exceptions.iter().flat_map(|(_, targets)| targets))
      .chain(
        self
          .exceptions
          .iter()
          .map(|(name, _)| name)
          .filter(|name| *name != "any crate"),
      );
    for name in named {
      if !packages.contains_key(name) || self.layer(name).is_none() {
        problems.push(format!("{name}: named in {ARCHITECTURE}, not a workspace member in a layer"));
      }
    }
    assert_none(problems);
  }

  fn layer(&self, name: &str) -> Option<usize> {
    self.layers.get(name).copied().flatten()
  }

  /// Every crate a Seams row names: the adapters and the UI-language crates.
  fn seam_crates(&self) -> BTreeSet<&str> {
    self.seams.values().flatten().map(String::as_str).collect()
  }

  /// The crates `name` reaches through Depends on edges, planned included.
  fn closure(&self, name: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![name.to_owned()];
    while let Some(next) = pending.pop() {
      for edge in self.depends.get(&next).into_iter().flatten() {
        if seen.insert(edge.name.clone()) {
          pending.push(edge.name.clone());
        }
      }
    }
    seen
  }
}

/// Fails with every problem found, one per line.
fn assert_none(problems: Vec<String>) {
  assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

// A cell lists crates with optional qualifiers: planned with an issue, the feature that adds it, or
// both.
#[test]
fn cells_read_planned_and_feature_edges() {
  let edge = |name: &str, planned: bool, feature: Option<&str>| Edge {
    name: name.to_owned(),
    planned,
    feature: feature.map(str::to_owned),
  };

  assert_eq!(
    cell("`a`, `b` (planned #5), `c` (feature `x`), `d` (planned #6, feature `y`), `e` (planned)"),
    Ok(vec![
      edge("a", false, None),
      edge("b", true, None),
      edge("c", false, Some("x")),
      edge("d", true, Some("y")),
      edge("e", true, None),
    ])
  );
  assert_eq!(cell("none"), Ok(Vec::new()));
}

// Prose in a cell is an error, so notes can't creep back into the table.
#[test]
fn an_unknown_qualifier_is_an_error() {
  assert!(cell("`a` (data only)").is_err());
  assert!(cell("`a` with the `x` feature").is_err());
  assert!(cell("`a` (planned #)").is_err());
}

// Every workspace member has one layer, or is outside the layers as a test-only package with no
// dependency row; every layered crate has one dependency row.
#[test]
fn every_member_has_a_layer() {
  let packages = metadata();
  let architecture = Architecture::read(&packages);
  let mut problems = Vec::new();
  for (name, package) in &packages {
    match architecture.layers.get(name) {
      None => problems.push(format!("{name}: in the workspace, in no Layers row of docs/architecture.md")),
      Some(None) => {
        if package.publish {
          problems.push(format!("{name}: outside the layers, so it needs `publish = false`"));
        }
        if architecture.depends.contains_key(name) {
          problems.push(format!("{name}: outside the layers, so it has no dependency row"));
        }
      }
      Some(Some(_)) => {
        if !architecture.depends.contains_key(name) {
          problems.push(format!("{name}: in a layer, with no dependency row"));
        }
      }
    }
  }
  for name in architecture.layers.keys().chain(architecture.depends.keys()) {
    if !packages.contains_key(name) {
      problems.push(format!("{name}: in docs/architecture.md, not in the workspace"));
    }
  }
  assert_none(problems);
}

// The dependency table and Cargo list the same edges, of the same kind, with the same features.
#[test]
fn the_table_lists_every_edge() {
  let packages = metadata();
  let architecture = Architecture::read(&packages);
  let mut problems = Vec::new();
  for (name, package) in &packages {
    if architecture.layer(name).is_none() {
      continue;
    }
    for (kind, table) in [("normal", &architecture.depends), ("dev", &architecture.dev)] {
      let cargo = package.dependencies.get(kind).cloned().unwrap_or_default();
      let listed: Vec<&Edge> = table.get(name).into_iter().flatten().collect();
      for (target, optional) in &cargo {
        if architecture.layers.get(target) == Some(&None) {
          problems.push(format!(
            "{name} -> {target} ({kind}): depends on a package outside the layers"
          ));
        }
        match listed.iter().find(|edge| &edge.name == target) {
          None => problems.push(format!(
            "{name} -> {target} ({kind}): in Cargo.toml, not in docs/architecture.md"
          )),
          Some(edge) if edge.planned => problems.push(format!(
            "{name} -> {target} ({kind}): marked planned in docs/architecture.md, already in Cargo.toml"
          )),
          Some(edge) => match (&edge.feature, optional) {
            (None, true) => problems.push(format!(
              "{name} -> {target} ({kind}): optional in Cargo.toml, so the table names its feature"
            )),
            (Some(feature), false) => problems.push(format!(
              "{name} -> {target} ({kind}): not optional in Cargo.toml, yet the table puts it behind `{feature}`"
            )),
            (Some(feature), true) => {
              let enables = package.features.get(feature).cloned().unwrap_or_default();
              if !enables.contains(&format!("dep:{target}")) {
                problems.push(format!("{name} -> {target} ({kind}): feature `{feature}` doesn't add it"));
              }
            }
            (None, false) => {}
          },
        }
      }
      for edge in listed {
        if !edge.planned && !cargo.contains_key(&edge.name) {
          problems.push(format!(
            "{name} -> {} ({kind}): in docs/architecture.md, not in Cargo.toml",
            edge.name
          ));
        }
      }
    }
  }
  assert_none(problems);
}

// A crate's layer is one more than the deepest crate it depends on, planned included.
#[test]
fn layers_are_depths() {
  let architecture = Architecture::read(&metadata());
  let mut problems = Vec::new();
  for (name, edges) in &architecture.depends {
    let Some(layer) = architecture.layer(name) else { continue };
    let depth = edges
      .iter()
      .map(|edge| architecture.layer(&edge.name).map_or(0, |layer| layer + 1))
      .max()
      .unwrap_or(0);
    if layer != depth {
      problems.push(format!("{name}: in layer {layer}, but its dependencies put it in {depth}"));
    }
  }
  assert_none(problems);
}

// A dev-dependency on the same or a higher layer is one the exceptions table lists, and every
// exception there is a dev-dependency the table has.
#[test]
fn dev_edges_follow_the_layers() {
  let architecture = Architecture::read(&metadata());
  let mut problems = Vec::new();
  for (name, edges) in &architecture.dev {
    let Some(layer) = architecture.layer(name) else { continue };
    for edge in edges {
      let target = architecture.layer(&edge.name).unwrap_or(0);
      if target < layer {
        continue;
      }
      let excepted = architecture.exceptions.iter().any(|(crate_name, targets)| {
        targets.contains(&edge.name)
          && (crate_name == name || (crate_name == "any crate" && !architecture.closure(&edge.name).contains(name)))
      });
      if !excepted {
        problems.push(format!(
          "{name} -> {} (dev): layer {target} from layer {layer}, and no row of the exceptions table allows it",
          edge.name
        ));
      }
    }
  }
  for (crate_name, targets) in &architecture.exceptions {
    for target in targets {
      let listed = architecture.dev.iter().any(|(name, edges)| {
        (crate_name == "any crate" || crate_name == name) && edges.iter().any(|edge| &edge.name == target)
      });
      if !listed {
        problems.push(format!(
          "exception {crate_name} -> {target}: no such dev-dependency in the table"
        ));
      }
      if crate_name != "any crate" && architecture.layer(target) < architecture.layer(crate_name) {
        problems.push(format!(
          "exception {crate_name} -> {target}: points to a lower layer, which needs no exception"
        ));
      }
    }
  }
  assert_none(problems);
}

// Only packages outside the layers depend on an adapter, of any kind.
#[test]
fn nothing_depends_on_an_adapter() {
  let packages = metadata();
  let architecture = Architecture::read(&packages);
  let adapters: BTreeSet<&String> = ["Host", "Renderer"]
    .iter()
    .flat_map(|seam| {
      architecture
        .seams
        .get(*seam)
        .unwrap_or_else(|| panic!("no {seam} row in Seams"))
    })
    .collect();
  assert!(!adapters.is_empty(), "the Seams table names no adapter");
  let mut problems = Vec::new();
  for (name, package) in &packages {
    if architecture.layer(name).is_none() {
      continue;
    }
    for (kind, targets) in &package.dependencies {
      for target in targets.keys() {
        if adapters.contains(target) {
          problems.push(format!(
            "{name} -> {target} ({kind}): only packages outside the layers depend on an adapter"
          ));
        }
      }
    }
  }
  assert_none(problems);
}

// The facade depends without a feature only on crates it re-exports, none of them a seam's, and
// its default features add no seam's crate.
#[test]
fn the_facade_depends_only_on_what_it_re_exports() {
  let packages = metadata();
  let architecture = Architecture::read(&packages);
  let facade = &packages["artic"];
  let lib = read("crates/artic/src/lib.rs");
  let seams = architecture.seam_crates();
  let mut problems = Vec::new();
  for (target, optional) in facade.dependencies.get("normal").into_iter().flatten() {
    if *optional {
      continue;
    }
    if seams.contains(target.as_str()) {
      problems.push(format!("artic -> {target}: a seam's crate, so it comes with a feature"));
    }
    let re_export = format!("pub use {} ", target.replace('-', "_"));
    if !lib.lines().any(|line| line.trim_start().starts_with(&re_export)) {
      problems.push(format!("artic -> {target}: depended on, not re-exported in src/lib.rs"));
    }
  }
  let mut pending = vec!["default".to_owned()];
  let mut seen = BTreeSet::new();
  while let Some(feature) = pending.pop() {
    for enabled in facade.features.get(&feature).into_iter().flatten() {
      let added = match (enabled.strip_prefix("dep:"), enabled.split_once('/')) {
        (Some(target), _) => Some(target),
        (None, Some((target, _))) if !target.ends_with('?') => Some(target),
        (None, Some(_)) => None,
        (None, None) => {
          if seen.insert(enabled.clone()) {
            pending.push(enabled.clone());
          }
          None
        }
      };
      if let Some(target) = added.filter(|target| seams.contains(target)) {
        problems.push(format!("artic's default features add {target}, a seam's crate"));
      }
    }
  }
  assert_none(problems);
}

// The README's table of crates and layers agrees with docs/architecture.md.
#[test]
fn readme_layers_match() {
  let architecture = Architecture::read(&metadata());
  let rows = table(&read("README.md"), "README.md", README_LAYERS);
  let readme: BTreeMap<String, Option<usize>> = rows
    .iter()
    .map(|row| (names(&row[1]).concat(), row[0].parse().ok()))
    .collect();
  assert_eq!(readme.len(), rows.len(), "README.md lists a crate twice");
  let layered: BTreeMap<String, Option<usize>> = architecture
    .layers
    .iter()
    .filter(|(_, layer)| layer.is_some())
    .map(|(name, layer)| (name.clone(), *layer))
    .collect();
  assert_eq!(
    readme, layered,
    "README.md's crates and layers differ from docs/architecture.md's"
  );
}

// A blank line inside a table is an error, not the table's end, so no row is dropped unseen.
#[test]
#[should_panic(expected = "blank line inside the table headed | A | B | in doc.md")]
fn a_blank_line_inside_a_table_is_an_error() {
  table("| A | B |\n| --- | --- |\n| 1 | 2 |\n\n| 3 | 4 |\n", "doc.md", "| A | B |");
}

// A row with fewer cells than the header is an error naming the row.
#[test]
#[should_panic(expected = "doc.md: `| 1 |` has 1 cells; the table headed | A | B | has 2")]
fn a_short_row_is_an_error() {
  table("| A | B |\n| --- | --- |\n| 1 |\n", "doc.md", "| A | B |");
}
