//! The markup extension grammar inside attribute values (MS-XAML §5.3), such as
//! `{Binding Email, Mode=TwoWay}`.

use super::text::{Decoded, OffsetMap};
use super::{Fatal, duplicate_of, unresolved};
use crate::infoset::{Member, MemberKey, TypeRef, XamlNode};
use artic_diagnostics::{Diagnostic, Diagnostics, SourceId, Span};

/// Parses an attribute value that starts with `{`: a markup extension, or literal text after a
/// `{}` escape. Returns `None` if the value has an error, which is recorded in `diagnostics`.
pub(super) fn parse(value: &Decoded, file: SourceId, diagnostics: &mut Diagnostics) -> Option<XamlNode> {
  Parser::new(value, file, diagnostics).run()
}

struct Parser<'a> {
  /// The decoded attribute value.
  text: &'a str,
  /// The byte of `text` read next.
  position: usize,
  offsets: &'a OffsetMap,
  file: SourceId,
  diagnostics: &'a mut Diagnostics,
}

impl<'a> Parser<'a> {
  fn new(value: &'a Decoded, file: SourceId, diagnostics: &'a mut Diagnostics) -> Parser<'a> {
    Parser {
      text: &value.text,
      position: 0,
      offsets: &value.offsets,
      file,
      diagnostics,
    }
  }

  /// Parses the whole value: literal text after the `{}` escape, or one extension followed by
  /// nothing but whitespace.
  fn run(mut self) -> Option<XamlNode> {
    let text = self.text;
    if let Some(literal) = text.strip_prefix("{}") {
      return Some(XamlNode::value(literal.to_owned(), self.span("{}".len(), text.len())));
    }
    let node = self.extension().ok()?;
    self.skip_whitespace();
    if self.position < text.len() {
      let span = self.span(self.position, text.trim_end().len());
      self
        .diagnostics
        .push(Diagnostic::error(span, "unexpected text after the markup extension"));
      return None;
    }
    Some(node)
  }

  /// Parses `{Name arguments}`, starting at its `{`.
  fn extension(&mut self) -> Result<XamlNode, Fatal> {
    let text = self.text;
    let open = self.position;
    self.bump();
    let name_start = self.position;
    while self
      .peek()
      .is_some_and(|character| !character.is_whitespace() && !matches!(character, ',' | '}'))
    {
      self.bump();
    }
    let name = &text[name_start..self.position];
    if name.is_empty() {
      let span = self.span(name_start, name_start);
      return Err(self.fatal(Diagnostic::error(span, "expected a markup extension name")));
    }
    let opened = self.span(open, self.position);
    let (prefix, local) = match name.split_once(':') {
      Some((prefix, local)) => (Some(prefix.to_owned()), local),
      None => (None, name),
    };
    let ty = TypeRef {
      prefix,
      name: local.to_owned(),
      namespace: unresolved(),
      span: self.span(name_start, self.position),
    };
    let mut positional = Vec::new();
    let mut named = Vec::new();
    self.skip_whitespace();
    if self.peek() != Some('}') {
      loop {
        self.skip_whitespace();
        if self.peek().is_none() {
          return Err(self.unclosed(opened, name));
        }
        self.argument(&mut positional, &mut named)?;
        self.skip_whitespace();
        match self.peek() {
          Some(',') => self.bump(),
          Some('}') => break,
          None => return Err(self.unclosed(opened, name)),
          Some(found) => {
            let span = self.span(self.position, self.position + found.len_utf8());
            return Err(self.fatal(Diagnostic::error(span, format!("expected `,` or `}}`, found `{found}`"))));
          }
        }
      }
    }
    self.bump();
    Ok(XamlNode::MarkupExtension {
      ty,
      positional,
      named,
      span: self.span(open, self.position),
    })
  }

  /// Parses one argument, positional or `Name=value`, into `positional` or `named`.
  fn argument(&mut self, positional: &mut Vec<XamlNode>, named: &mut Vec<Member>) -> Result<(), Fatal> {
    let start = self.position;
    match self.peek() {
      Some(',' | '}') => {
        let span = self.span(start, start);
        return Err(self.fatal(Diagnostic::error(span, "expected an argument")));
      }
      Some('=') => {
        let span = self.span(start, start);
        return Err(self.bad_name(span));
      }
      _ => {}
    }
    let plain = !matches!(self.peek(), Some('{' | '\'' | '"'));
    let value = self.value()?;
    self.skip_whitespace();
    if self.peek() != Some('=') {
      if let Some(first) = named.first() {
        return Err(
          self.fatal(
            Diagnostic::error(value.span(), "positional argument after a named one")
              .with_secondary(first.span, "named argument here")
              .with_help("move positional arguments before named ones"),
          ),
        );
      }
      positional.push(value);
      return Ok(());
    }
    let (name, name_span) = match value {
      XamlNode::Value { text, span, .. } if plain && !text.contains(char::is_whitespace) => (text, span),
      other => return Err(self.bad_name(other.span())),
    };
    self.bump();
    self.skip_whitespace();
    if matches!(self.peek(), Some(',' | '}')) {
      let span = self.span(self.position, self.position);
      return Err(self.fatal(Diagnostic::error(span, "expected a value after `=`")));
    }
    let member = Member {
      key: MemberKey::Property(name),
      value: vec![self.value()?],
      span: name_span,
    };
    match duplicate_of(named, &member) {
      Some(duplicate) => self.diagnostics.push(duplicate),
      None => named.push(member),
    }
    Ok(())
  }

  /// Parses an argument's value: a nested extension, an escaped, quoted or plain text.
  fn value(&mut self) -> Result<XamlNode, Fatal> {
    match self.peek() {
      Some('{') if self.text[self.position..].starts_with("{}") => Ok(self.escaped()),
      Some('{') => self.extension(),
      Some(quote @ ('\'' | '"')) => self.quoted(quote),
      _ => Ok(self.plain()),
    }
  }

  /// Reads literal text after a `{}` escape, up to the next `,` or `}` outside balanced braces.
  fn escaped(&mut self) -> XamlNode {
    let text = self.text;
    self.position += "{}".len();
    let start = self.position;
    let mut depth: usize = 0;
    while let Some(character) = self.peek() {
      match character {
        ',' | '}' if depth == 0 => break,
        '{' => depth += 1,
        '}' => depth -= 1,
        _ => {}
      }
      self.bump();
    }
    let end = start + text[start..self.position].trim_end().len();
    XamlNode::value(text[start..end].to_owned(), self.span(start, end))
  }

  /// Reads text between `quote`s, where `\` escapes the next character. The span excludes the
  /// quotes.
  fn quoted(&mut self, quote: char) -> Result<XamlNode, Fatal> {
    let open = self.position;
    self.bump();
    let start = self.position;
    let mut text = String::new();
    loop {
      match self.peek() {
        None => {
          let end = self.text.len();
          let diagnostic = Diagnostic::error(self.span(end, end), "quoted value is never closed")
            .with_secondary(self.span(open, start), "opened here");
          return Err(self.fatal(diagnostic));
        }
        Some(character) if character == quote => break,
        Some('\\') => {
          self.bump();
          if let Some(escaped) = self.peek() {
            text.push(escaped);
            self.bump();
          }
        }
        Some(character) => {
          text.push(character);
          self.bump();
        }
      }
    }
    let span = self.span(start, self.position);
    self.bump();
    Ok(XamlNode::value(text, span))
  }

  /// Reads unquoted text up to `,`, `=` or `}`, where `\` escapes the next character. Trailing
  /// whitespace isn't part of the value.
  fn plain(&mut self) -> XamlNode {
    let start = self.position;
    let mut text = String::new();
    // The text's length, and the position, after its last character that isn't trailing whitespace.
    let mut kept = 0;
    let mut end = start;
    while let Some(character) = self.peek() {
      match character {
        ',' | '=' | '}' => break,
        '\\' => {
          self.bump();
          if let Some(escaped) = self.peek() {
            text.push(escaped);
            self.bump();
            kept = text.len();
            end = self.position;
          }
        }
        _ => {
          text.push(character);
          self.bump();
          if !character.is_whitespace() {
            kept = text.len();
            end = self.position;
          }
        }
      }
    }
    text.truncate(kept);
    XamlNode::value(text, self.span(start, end))
  }

  fn bad_name(&mut self, span: Span) -> Fatal {
    self.fatal(Diagnostic::error(span, "expected an argument name before `=`").with_help("a name is a single word, unquoted"))
  }

  fn unclosed(&mut self, opened: Span, name: &str) -> Fatal {
    let end = self.text.len();
    let diagnostic = Diagnostic::error(self.span(end, end), format!("markup extension `{{{name}` is never closed"))
      .with_secondary(opened, "opened here");
    self.fatal(diagnostic)
  }

  fn peek(&self) -> Option<char> {
    self.text[self.position..].chars().next()
  }

  fn bump(&mut self) {
    if let Some(character) = self.peek() {
      self.position += character.len_utf8();
    }
  }

  fn skip_whitespace(&mut self) {
    while self.peek().is_some_and(char::is_whitespace) {
      self.bump();
    }
  }

  fn fatal(&mut self, diagnostic: Diagnostic) -> Fatal {
    self.diagnostics.push(diagnostic);
    Fatal
  }

  /// The source span of bytes `start..end` of the decoded value.
  fn span(&self, start: usize, end: usize) -> Span {
    Span::new(self.file, self.offsets.source_offset(start), self.offsets.source_offset(end))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::parse::text::{Kind, decode};
  use artic_diagnostics::SourceMap;
  use pretty_assertions::assert_eq;

  /// Parses `value` as an attribute value written at the start of a file.
  fn parse_value(value: &str) -> (Option<XamlNode>, Diagnostics) {
    let file = SourceMap::new().add("test.xaml", value);
    let decoded = decode(value, 0, Kind::Attribute).expect("valid attribute text");
    let mut diagnostics = Diagnostics::new();
    let node = parse(&decoded, file, &mut diagnostics);
    (node, diagnostics)
  }

  /// Parses `value`, panicking on errors, and returns the result as compact text, such as
  /// `Binding("Email", Mode="TwoWay")`.
  fn show(value: &str) -> String {
    let (node, diagnostics) = parse_value(value);
    assert!(diagnostics.is_empty(), "unexpected errors: {diagnostics:?}");
    compact(&node.expect("a node"))
  }

  fn compact(node: &XamlNode) -> String {
    match node {
      XamlNode::Value { text, .. } => format!("{text:?}"),
      XamlNode::MarkupExtension {
        ty, positional, named, ..
      } => {
        let name = match &ty.prefix {
          Some(prefix) => format!("{prefix}:{}", ty.name),
          None => ty.name.clone(),
        };
        let named = named.iter().map(|member| {
          let MemberKey::Property(key) = &member.key else {
            panic!("named arguments are properties");
          };
          format!("{key}={}", compact(&member.value[0]))
        });
        let arguments: Vec<_> = positional.iter().map(compact).chain(named).collect();
        format!("{name}({})", arguments.join(", "))
      }
      XamlNode::Object { .. } => panic!("an extension holds no objects"),
    }
  }

  /// Parses `value`, expecting errors, and returns each one's message, start and the text it
  /// points at.
  fn errors(value: &str) -> Vec<(String, usize, &str)> {
    let (_, diagnostics) = parse_value(value);
    assert!(!diagnostics.is_empty(), "expected errors in {value:?}");
    diagnostics
      .iter()
      .map(|error| {
        (
          error.message.clone(),
          error.span.start,
          &value[error.span.start..error.span.end],
        )
      })
      .collect()
  }

  // Arguments given only by position, including none at all.
  #[test]
  fn positional_only() {
    assert_eq!(show("{Binding Email}"), r#"Binding("Email")"#);
    assert_eq!(show("{x:Null}"), "x:Null()");
    assert_eq!(show("{A one, two}"), r#"A("one", "two")"#);
  }

  // After the name, `:` is an ordinary character: only the name has a prefix.
  #[test]
  fn colons_after_the_name_are_text() {
    assert_eq!(
      show("{x:Static local::CONSTANTS::MAX}"),
      r#"x:Static("local::CONSTANTS::MAX")"#
    );
  }

  // Arguments given only by name.
  #[test]
  fn named_only() {
    assert_eq!(
      show("{Binding Path=Name, Mode=TwoWay}"),
      r#"Binding(Path="Name", Mode="TwoWay")"#
    );
  }

  // Positional arguments, then named ones.
  #[test]
  fn mixed() {
    assert_eq!(show("{Binding Email, Mode=TwoWay}"), r#"Binding("Email", Mode="TwoWay")"#);
  }

  // An argument can be an extension, which can hold extensions too.
  #[test]
  fn nested_two_levels() {
    assert_eq!(
      show("{Binding Source={StaticResource Key={x:Static a::B}}, Mode=OneWay}"),
      r#"Binding(Source=StaticResource(Key=x:Static("a::B")), Mode="OneWay")"#
    );
    assert_eq!(
      show("{Binding RelativeSource={RelativeSource TemplatedParent}}"),
      r#"Binding(RelativeSource=RelativeSource("TemplatedParent"))"#
    );
  }

  // `{}` makes the rest of a value literal text, braces included; in an argument it stops at the
  // next `,` or `}` outside balanced braces.
  #[test]
  fn escape() {
    assert_eq!(show("{}{0}"), r#""{0}""#);
    assert_eq!(
      show("{Binding Total, StringFormat={}{0:N2}, Mode=OneWay}"),
      r#"Binding("Total", StringFormat="{0:N2}", Mode="OneWay")"#
    );
  }

  // Quotes protect commas and braces, and `\` escapes the next character, quoted or not.
  #[test]
  fn quoted_values() {
    assert_eq!(
      show("{Binding Text, FallbackValue='a, b {c}'}"),
      r#"Binding("Text", FallbackValue="a, b {c}")"#
    );
    assert_eq!(show(r#"{A "it's", 'say \'hi\''}"#), r#"A("it's", "say 'hi'")"#);
    assert_eq!(show(r"{A a\,b\}c}"), r#"A("a,b}c")"#);
  }

  // Quotes written as entities are quotes: the grammar reads the decoded value.
  #[test]
  fn entity_quotes_are_quotes() {
    assert_eq!(show("{A &quot;a, b&quot;}"), r#"A("a, b")"#);
  }

  // Plain values are trimmed, and keep the spaces inside them.
  #[test]
  fn plain_values_are_trimmed() {
    assert_eq!(
      show("{Binding  Some Name , Path = A.B }"),
      r#"Binding("Some Name", Path="A.B")"#
    );
  }

  // Every part points at the source text it came from, past entities.
  #[test]
  fn argument_spans_are_exact() {
    let value = "{Binding Source={StaticResource Accent}, Mode='Two Way', Path=a&amp;b}";
    let (node, _) = parse_value(value);
    let at = |span: Span| &value[span.start..span.end];

    let Some(XamlNode::MarkupExtension { ty, named, span, .. }) = &node else {
      panic!("an extension");
    };
    assert_eq!(at(*span), value);
    assert_eq!(at(ty.span), "Binding");
    let spans: Vec<_> = named
      .iter()
      .map(|member| (at(member.span), at(member.value[0].span())))
      .collect();
    assert_eq!(
      spans,
      [
        ("Source", "{StaticResource Accent}"),
        ("Mode", "Two Way"),
        ("Path", "a&amp;b")
      ]
    );
    let XamlNode::MarkupExtension { ty, positional, .. } = &named[0].value[0] else {
      panic!("a nested extension");
    };
    assert_eq!((at(ty.span), at(positional[0].span())), ("StaticResource", "Accent"));
  }

  // Each malformed extension reports one error, at the problem.
  #[test]
  fn errors_point_at_the_problem() {
    let cases = [
      ("{Binding Email", "markup extension `{Binding` is never closed", 14, ""),
      ("{A 'x", "quoted value is never closed", 5, ""),
      ("{ A}", "expected a markup extension name", 1, ""),
      ("{A x,}", "expected an argument", 5, ""),
      ("{A ,x}", "expected an argument", 3, ""),
      ("{A Mode=}", "expected a value after `=`", 8, ""),
      ("{A =x}", "expected an argument name before `=`", 3, ""),
      ("{A 'n'=x}", "expected an argument name before `=`", 4, "n"),
      ("{A a b=x}", "expected an argument name before `=`", 3, "a b"),
      ("{A Mode=x, y}", "positional argument after a named one", 11, "y"),
      ("{A 'x' y}", "expected `,` or `}`, found `y`", 7, "y"),
      ("{x:Null}x", "unexpected text after the markup extension", 8, "x"),
    ];
    for (value, message, start, text) in cases {
      assert_eq!(errors(value), [(message.to_owned(), start, text)], "in {value:?}");
    }
  }

  // Errors about two places point at both: the unclosed brace's opening, the named argument a
  // positional one follows.
  #[test]
  fn errors_point_back_to_their_cause() {
    let (_, diagnostics) = parse_value("{Binding Email");
    let opened = &diagnostics.iter().next().expect("one error").labels[0];
    assert_eq!((opened.span.start, opened.span.end), (0, "{Binding".len()));

    let value = "{A Mode=x, y}";
    let (_, diagnostics) = parse_value(value);
    let named = &diagnostics.iter().next().expect("one error").labels[0];
    assert_eq!(&value[named.span.start..named.span.end], "Mode");
  }

  // A named argument given twice is reported, and the extension is kept without the repeat.
  #[test]
  fn duplicate_named_arguments() {
    let (node, diagnostics) = parse_value("{A M=1, M=2}");

    assert_eq!(compact(&node.expect("parsing continues")), r#"A(M="1")"#);
    let error = diagnostics.iter().next().expect("one error");
    assert_eq!((error.span.start, error.span.end), (8, 9));
    assert_eq!(error.labels[0].message, "first set here");
  }
}
