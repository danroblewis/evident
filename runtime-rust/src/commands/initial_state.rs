//! Tiny JSON reader for the `--initial-state` CLI flag.
//!
//! Supports just the subset Evident needs to seed a first-frame
//! `given` map: a top-level object whose values are int / bool /
//! string / homogeneous-array. No nested objects (Evident's
//! `Value::Composite` isn't carried as a top-level given anyway),
//! no floats (no `Value::Real` yet).
//!
//! Hand-rolled to avoid pulling in `serde_json` for ~100 lines
//! of parsing. If the supported shape grows, consider switching.
//!
//! Output format mirrors what the executor's plugin matcher
//! produces: a `HashMap<String, Value>` of the bindings to inject
//! into the first frame's `given`.

use std::collections::HashMap;

use evident_runtime::Value;

/// Parse a JSON file's top-level object into a `given` map.
///
/// Accepted JSON shape:
/// ```json
/// {
///   "world.score": 42,
///   "world.player_alive": true,
///   "world.name": "Alice",
///   "world.lives": [3, 2, 1]
/// }
/// ```
///
/// Each top-level key becomes a `given` entry; the value type
/// determines the `Value` variant. Returns a clear error string
/// pointing at byte offsets if the JSON is malformed.
pub fn parse_initial_state(src: &str) -> Result<HashMap<String, Value>, String> {
    let mut p = Parser { src, pos: 0 };
    p.skip_whitespace();
    p.expect('{')?;
    let mut out = HashMap::new();
    p.skip_whitespace();
    if p.peek() == Some('}') {
        p.bump();
        return Ok(out);
    }
    loop {
        p.skip_whitespace();
        let key = p.parse_string()?;
        p.skip_whitespace();
        p.expect(':')?;
        p.skip_whitespace();
        let value = p.parse_value()?;
        out.insert(key, value);
        p.skip_whitespace();
        match p.peek() {
            Some(',') => { p.bump(); }
            Some('}') => { p.bump(); break; }
            _ => return Err(format!("expected ',' or '}}' at offset {}", p.pos)),
        }
    }
    Ok(out)
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> { self.src[self.pos..].chars().next() }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() { self.bump(); } else { break; }
        }
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        match self.peek() {
            Some(actual) if actual == c => { self.bump(); Ok(()) }
            Some(actual) => Err(format!("expected {c:?} at offset {}, got {actual:?}", self.pos)),
            None => Err(format!("expected {c:?} at offset {}, got EOF", self.pos)),
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut s = String::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(s),
                Some('\\') => match self.bump() {
                    Some('"')  => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some('n')  => s.push('\n'),
                    Some('t')  => s.push('\t'),
                    Some('/')  => s.push('/'),
                    Some(c) => return Err(format!("unknown escape \\{c:?} at offset {}", self.pos)),
                    None    => return Err(format!("unterminated escape at offset {}", self.pos)),
                },
                Some(c) => s.push(c),
                None    => return Err(format!("unterminated string at offset {}", self.pos)),
            }
        }
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some('"') => Ok(Value::Str(self.parse_string()?)),
            Some('t') | Some('f') => self.parse_bool(),
            Some('-') | Some('0'..='9') => self.parse_number(),
            Some('[') => self.parse_array(),
            Some(c)   => Err(format!("unexpected {c:?} at offset {}", self.pos)),
            None      => Err(format!("expected value at offset {} (got EOF)", self.pos)),
        }
    }

    fn parse_bool(&mut self) -> Result<Value, String> {
        if self.src[self.pos..].starts_with("true") {
            self.pos += 4;
            Ok(Value::Bool(true))
        } else if self.src[self.pos..].starts_with("false") {
            self.pos += 5;
            Ok(Value::Bool(false))
        } else {
            Err(format!("expected true/false at offset {}", self.pos))
        }
    }

    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some('-') { self.bump(); }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() { self.bump(); } else { break; }
        }
        let s = &self.src[start..self.pos];
        s.parse::<i64>()
            .map(Value::Int)
            .map_err(|e| format!("bad integer {s:?} at offset {start}: {e}"))
    }

    fn parse_array(&mut self) -> Result<Value, String> {
        self.expect('[')?;
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.bump();
            // Empty arrays default to SeqInt — first non-empty
            // element would otherwise determine the variant.
            return Ok(Value::SeqInt(Vec::new()));
        }
        // Detect element kind from first value, then collect homogeneously.
        let first = self.parse_value()?;
        let kind = match &first { Value::Int(_) => 0, Value::Bool(_) => 1, Value::Str(_) => 2, _ => 3 };
        let mut ints  = Vec::new();
        let mut bools = Vec::new();
        let mut strs  = Vec::new();
        match first {
            Value::Int(n)  => ints.push(n),
            Value::Bool(b) => bools.push(b),
            Value::Str(s)  => strs.push(s),
            _ => return Err(format!("nested arrays not supported (offset {})", self.pos)),
        }
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(',') => { self.bump(); }
                Some(']') => { self.bump(); break; }
                _ => return Err(format!("expected ',' or ']' in array at offset {}", self.pos)),
            }
            self.skip_whitespace();
            let v = self.parse_value()?;
            match (kind, v) {
                (0, Value::Int(n))  => ints.push(n),
                (1, Value::Bool(b)) => bools.push(b),
                (2, Value::Str(s))  => strs.push(s),
                _ => return Err(format!("array elements must all be the same type (offset {})", self.pos)),
            }
        }
        Ok(match kind {
            0 => Value::SeqInt(ints),
            1 => Value::SeqBool(bools),
            2 => Value::SeqStr(strs),
            _ => unreachable!(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_object() {
        let src = r#"{"world.score": 42, "world.alive": true, "world.name": "Alice"}"#;
        let r = parse_initial_state(src).unwrap();
        assert_eq!(r.get("world.score"),  Some(&Value::Int(42)));
        assert_eq!(r.get("world.alive"),  Some(&Value::Bool(true)));
        assert_eq!(r.get("world.name"),   Some(&Value::Str("Alice".to_string())));
    }

    #[test]
    fn parses_arrays() {
        let src = r#"{"world.lives": [3, 2, 1], "world.flags": [true, false]}"#;
        let r = parse_initial_state(src).unwrap();
        assert_eq!(r.get("world.lives"),  Some(&Value::SeqInt(vec![3, 2, 1])));
        assert_eq!(r.get("world.flags"),  Some(&Value::SeqBool(vec![true, false])));
    }

    #[test]
    fn empty_object() {
        let r = parse_initial_state("{}").unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn rejects_unknown_token() {
        assert!(parse_initial_state(r#"{"x": null}"#).is_err());
    }

    #[test]
    fn rejects_mixed_array() {
        assert!(parse_initial_state(r#"{"x": [1, "two"]}"#).is_err());
    }

    #[test]
    fn negative_int() {
        let r = parse_initial_state(r#"{"x": -42}"#).unwrap();
        assert_eq!(r.get("x"), Some(&Value::Int(-42)));
    }

    #[test]
    fn handles_whitespace_and_newlines() {
        let src = "{\n  \"a\": 1,\n  \"b\": 2\n}\n";
        let r = parse_initial_state(src).unwrap();
        assert_eq!(r.get("a"), Some(&Value::Int(1)));
        assert_eq!(r.get("b"), Some(&Value::Int(2)));
    }
}
