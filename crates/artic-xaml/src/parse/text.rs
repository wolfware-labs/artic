//! Decoding XML text and attribute values.

/// Why a piece of text couldn't be decoded, and where, as byte offsets into the source.
#[derive(Debug, PartialEq)]
pub struct DecodeError {
  pub start: usize,
  pub end: usize,
  pub message: String,
}

/// Decoded text, and where each of its bytes was written.
#[derive(Debug, PartialEq)]
pub struct Decoded {
  pub text: String,
  pub offsets: OffsetMap,
}

/// Maps byte offsets in decoded text back to the source, past entities and line endings whose
/// decoded length differs from the written one.
#[derive(Debug, PartialEq)]
pub struct OffsetMap {
  /// Where the raw text starts in the source.
  start: usize,
  /// For each entity and each `\r\n`, the offsets just after it: in the decoded text, then in the
  /// raw text. Sorted by both.
  shifts: Vec<(usize, usize)>,
}

impl OffsetMap {
  /// Returns the source offset of byte `decoded` of the decoded text.
  pub fn source_offset(&self, decoded: usize) -> usize {
    let after = self.shifts.partition_point(|&(shift, _)| shift <= decoded);
    let (decoded_base, raw_base) = after.checked_sub(1).map_or((0, 0), |index| self.shifts[index]);
    self.start + raw_base + (decoded - decoded_base)
  }
}

/// How whitespace in the raw text is treated.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
  /// Element text: line endings become `\n`, other whitespace is kept.
  Text,
  /// An attribute value: line endings, tabs and newlines each become one space (XML 1.0 §3.3.3).
  Attribute,
}

/// Decodes `raw`, which starts at byte `offset` of the source: resolves the five predefined
/// entities and character references, and normalizes line endings (and, for attributes,
/// whitespace) the way an XML processor does. Characters written as references are never
/// normalized.
pub fn decode(raw: &str, offset: usize, kind: Kind) -> Result<Decoded, DecodeError> {
  let mut decoded = String::with_capacity(raw.len());
  let mut shifts = Vec::new();
  let mut rest = raw.char_indices().peekable();
  while let Some((index, character)) = rest.next() {
    match character {
      '&' => {
        let end = raw[index..].find(';').map(|position| index + position + 1);
        let Some(end) = end else {
          return Err(DecodeError {
            start: offset + index,
            end: offset + index + 1,
            message: "`&` starts an entity that has no closing `;`; write `&amp;` for a literal `&`".to_owned(),
          });
        };
        let character = resolve(&raw[index..end], offset + index)?;
        decoded.push(character);
        shifts.push((decoded.len(), end));
        while rest.peek().is_some_and(|(next, _)| *next < end) {
          rest.next();
        }
      }
      '\r' => {
        decoded.push(if kind == Kind::Attribute { ' ' } else { '\n' });
        if rest.peek().is_some_and(|(_, next)| *next == '\n') {
          rest.next();
          shifts.push((decoded.len(), index + 2));
        }
      }
      '\n' | '\t' if kind == Kind::Attribute => decoded.push(' '),
      _ => decoded.push(character),
    }
  }
  Ok(Decoded {
    text: decoded,
    offsets: OffsetMap { start: offset, shifts },
  })
}

/// Normalizes line endings in CDATA text, which has no entities.
pub fn normalize_line_endings(raw: &str) -> String {
  raw.replace("\r\n", "\n").replace('\r', "\n")
}

/// Resolves one `&…;` reference, which starts at byte `start` of the source.
fn resolve(reference: &str, start: usize) -> Result<char, DecodeError> {
  let name = &reference[1..reference.len() - 1];
  let error = |message: String| DecodeError {
    start,
    end: start + reference.len(),
    message,
  };
  let character = match name {
    "lt" => Some('<'),
    "gt" => Some('>'),
    "amp" => Some('&'),
    "quot" => Some('"'),
    "apos" => Some('\''),
    _ => {
      let code = if let Some(hex) = name.strip_prefix("#x") {
        u32::from_str_radix(hex, 16).ok()
      } else if let Some(decimal) = name.strip_prefix('#') {
        decimal.parse().ok()
      } else {
        return Err(error(format!(
          "unknown entity `{reference}`; XML only defines `&lt;`, `&gt;`, `&amp;`, `&quot;` and `&apos;`, so write other characters as references like `&#160;`"
        )));
      };
      code.and_then(char::from_u32)
    }
  };
  character.ok_or_else(|| error(format!("`{reference}` is not a valid character reference")))
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // The predefined entities and character references are resolved.
  #[test]
  fn entities_and_references_are_resolved() {
    assert_eq!(
      decode("a &lt; b &amp;&amp; &#65;&#x42; &quot;&apos;", 0, Kind::Text).map(|decoded| decoded.text),
      Ok("a < b && AB \"'".to_owned())
    );
  }

  // A bare `&` is reported at the byte it stands on in the source.
  #[test]
  fn a_bare_ampersand_is_reported_where_it_stands() {
    let error = decode("a & b", 10, Kind::Text).expect_err("a bare `&`");

    assert_eq!((error.start, error.end), (12, 13));
  }

  // Line endings become `\n` in text; in attributes every whitespace character becomes a space,
  // except characters written as references.
  #[test]
  fn whitespace_normalization() {
    let text = |raw, kind| decode(raw, 0, kind).map(|decoded| decoded.text);
    assert_eq!(text("a\r\nb\rc\td", Kind::Text), Ok("a\nb\nc\td".to_owned()));
    assert_eq!(text("a\r\nb\tc\nd&#10;e", Kind::Attribute), Ok("a b c d\ne".to_owned()));
  }

  // Every decoded offset maps to where it was written, past entities and `\r\n`.
  #[test]
  fn offsets_map_back_to_source() {
    let decoded = decode("a&amp;b\r\nc&#233;", 10, Kind::Text).expect("valid text");

    assert_eq!(decoded.text, "a&b\ncé");
    let offsets: Vec<_> = (0..=decoded.text.len())
      .map(|byte| decoded.offsets.source_offset(byte))
      .collect();
    // a, &, b, \n, c, é (2 bytes), end
    assert_eq!(offsets, [10, 11, 16, 17, 19, 20, 21, 26]);
  }

  // An unknown entity is reported at its position in the source, with a hint.
  #[test]
  fn unknown_entity_is_an_error() {
    let error = decode("a &nbsp; b", 10, Kind::Text).expect_err("unknown entity");

    assert_eq!((error.start, error.end), (12, 18));
    assert!(error.message.starts_with("unknown entity `&nbsp;`"));
  }

  // A bare `&` and an invalid code point are rejected.
  #[test]
  fn bad_references_are_errors() {
    assert!(decode("a & b", 0, Kind::Text).is_err());
    assert!(decode("&#xD800;", 0, Kind::Text).is_err());
  }
}
