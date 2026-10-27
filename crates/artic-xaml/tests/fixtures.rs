// The parser fixture corpus: every file under `fixtures/valid/` must parse and snapshots its
// infoset; every file under `fixtures/invalid/` must fail and snapshots its rendered errors. See
// `fixtures/README.md`.

use artic_diagnostics::{SourceFile, SourceId, SourceMap, Span};
use artic_xaml::infoset::{Member, MemberKey, NamespaceDecl, TypeRef, XamlDocument, XamlNamespace, XamlNode};
use artic_xaml::parse;
use std::fmt::Write;
use std::path::Path;

#[test]
fn valid() {
  insta::glob!("fixtures/valid/**/*.xaml", |path| {
    let (sources, file) = load(path);
    let document =
      parse(&sources, file).unwrap_or_else(|errors| panic!("{} should parse:\n{}", path.display(), errors.render(&sources)));
    insta::assert_snapshot!(print(sources.get(file), &document));
  });
}

#[test]
fn invalid() {
  insta::glob!("fixtures/invalid/**/*.xaml", |path| {
    let (sources, file) = load(path);
    let Err(errors) = parse(&sources, file) else {
      panic!("{} should have errors", path.display());
    };
    insta::assert_snapshot!(errors.render(&sources));
  });
}

/// Loads a fixture under the path it has inside `fixtures/`, with `/` separators, so rendered errors
/// read the same on every machine. The path is cut at its last `fixtures` component rather than
/// compared with the manifest directory, whose absolute form can differ (Windows `\\?\` paths).
fn load(path: &Path) -> (SourceMap, SourceId) {
  let parts: Vec<_> = path.iter().map(|part| part.to_string_lossy()).collect();
  let fixtures = parts
    .iter()
    .rposition(|part| part == "fixtures")
    .expect("a fixture is under tests/fixtures");
  let text = std::fs::read_to_string(path).expect("a readable fixture");
  let mut sources = SourceMap::new();
  let file = sources.add(parts[fixtures + 1..].join("/"), text);
  (sources, file)
}

/// The infoset as indented text: one line per node or member, spans as `line:col..line:col`.
fn print(file: &SourceFile, document: &XamlDocument) -> String {
  let mut printer = Printer {
    file,
    text: String::new(),
  };
  printer.node(&document.root, 0);
  printer.text
}

struct Printer<'a> {
  file: &'a SourceFile,
  text: String,
}

impl Printer<'_> {
  fn node(&mut self, node: &XamlNode, depth: usize) {
    match node {
      XamlNode::Object {
        ty, members, namespaces, ..
      } => {
        self.line(depth, format!("Object {} {}", type_ref(ty), self.at(ty.span)));
        for declaration in namespaces {
          let attribute = NamespaceDecl::attribute(declaration.prefix.as_deref());
          let at = self.at(declaration.span);
          self.line(depth + 1, format!("{attribute} {:?} {at}", declaration.uri));
        }
        for member in members {
          self.member(member, depth + 1);
        }
      }
      XamlNode::Value { text, raw, span } => {
        let raw = if raw == text { String::new() } else { format!(" raw {raw:?}") };
        self.line(depth, format!("Value {text:?}{raw} {}", self.at(*span)));
      }
      XamlNode::MarkupExtension {
        ty, positional, named, ..
      } => {
        self.line(depth, format!("MarkupExtension {} {}", type_ref(ty), self.at(ty.span)));
        if !positional.is_empty() {
          self.line(depth + 1, "positional".to_owned());
          for argument in positional {
            self.node(argument, depth + 2);
          }
        }
        for member in named {
          self.member(member, depth + 1);
        }
      }
    }
  }

  fn member(&mut self, member: &Member, depth: usize) {
    let key = match &member.key {
      MemberKey::Property(name) => name.clone(),
      MemberKey::Attached { owner, name } => {
        format!("attached {owner}.{name} [{}]", namespace(&owner.namespace))
      }
      MemberKey::Directive(directive) => format!("directive {directive:?}"),
      MemberKey::Content => "content".to_owned(),
      MemberKey::Items => "items".to_owned(),
    };
    self.line(depth, format!("{key} {}", self.at(member.span)));
    for value in &member.value {
      self.node(value, depth + 1);
    }
  }

  fn line(&mut self, depth: usize, line: String) {
    writeln!(self.text, "{:indent$}{line}", "", indent = depth * 2).expect("writing to a String");
  }

  fn at(&self, span: Span) -> String {
    let (start, end) = (self.file.line_col(span.start), self.file.line_col(span.end));
    format!("{}:{}..{}:{}", start.line, start.column, end.line, end.column)
  }
}

/// A type as written, with the namespace it resolved to: `x:Static [markup]`.
fn type_ref(ty: &TypeRef) -> String {
  format!("{ty} [{}]", namespace(&ty.namespace))
}

fn namespace(namespace: &XamlNamespace) -> String {
  match namespace {
    XamlNamespace::Markup => "markup".to_owned(),
    XamlNamespace::Uri(uri) => uri.clone(),
    XamlNamespace::Using(path) => format!("using:{path}"),
  }
}
