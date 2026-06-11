//! Minimal JSON value + parser + serializer — just enough for LSP traffic.
//! No external dependencies (keeps the tool a single self-contained binary).

use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(BTreeMap<String, J>),
}

impl J {
    pub fn obj() -> J {
        J::Obj(BTreeMap::new())
    }
    pub fn set(mut self, k: &str, v: J) -> J {
        if let J::Obj(m) = &mut self {
            m.insert(k.to_string(), v);
        }
        self
    }
    pub fn get(&self, k: &str) -> Option<&J> {
        match self {
            J::Obj(m) => m.get(k),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            J::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            J::Num(n) => Some(*n as i64),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&Vec<J>> {
        match self {
            J::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn to_string(&self) -> String {
        let mut s = String::new();
        self.write(&mut s);
        s
    }

    fn write(&self, out: &mut String) {
        match self {
            J::Null => out.push_str("null"),
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            J::Str(s) => write_str(s, out),
            J::Arr(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            J::Obj(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_str(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

// ── parser ──────────────────────────────────────────────────────────────

pub fn parse(s: &str) -> Option<J> {
    let chars: Vec<char> = s.chars().collect();
    let mut p = Parser { c: chars, i: 0 };
    p.ws();
    let v = p.value()?;
    Some(v)
}

struct Parser {
    c: Vec<char>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.c.get(self.i).copied();
        self.i += 1;
        c
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.i += 1;
        }
    }
    fn value(&mut self) -> Option<J> {
        self.ws();
        match self.peek()? {
            '{' => self.object(),
            '[' => self.array(),
            '"' => self.string().map(J::Str),
            't' => self.lit("true", J::Bool(true)),
            'f' => self.lit("false", J::Bool(false)),
            'n' => self.lit("null", J::Null),
            _ => self.number(),
        }
    }
    fn lit(&mut self, word: &str, val: J) -> Option<J> {
        for wc in word.chars() {
            if self.bump()? != wc {
                return None;
            }
        }
        Some(val)
    }
    fn object(&mut self) -> Option<J> {
        self.bump(); // {
        let mut m = BTreeMap::new();
        self.ws();
        if self.peek() == Some('}') {
            self.bump();
            return Some(J::Obj(m));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            if self.bump()? != ':' {
                return None;
            }
            let v = self.value()?;
            m.insert(k, v);
            self.ws();
            match self.bump()? {
                ',' => continue,
                '}' => break,
                _ => return None,
            }
        }
        Some(J::Obj(m))
    }
    fn array(&mut self) -> Option<J> {
        self.bump(); // [
        let mut a = Vec::new();
        self.ws();
        if self.peek() == Some(']') {
            self.bump();
            return Some(J::Arr(a));
        }
        loop {
            let v = self.value()?;
            a.push(v);
            self.ws();
            match self.bump()? {
                ',' => continue,
                ']' => break,
                _ => return None,
            }
        }
        Some(J::Arr(a))
    }
    fn string(&mut self) -> Option<String> {
        self.ws();
        if self.bump()? != '"' {
            return None;
        }
        let mut s = String::new();
        loop {
            match self.bump()? {
                '"' => break,
                '\\' => {
                    let e = self.bump()?;
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        '/' => s.push('/'),
                        'b' => s.push('\u{08}'),
                        'f' => s.push('\u{0c}'),
                        'u' => {
                            let mut code = 0u32;
                            for _ in 0..4 {
                                let h = self.bump()?;
                                code = code * 16 + h.to_digit(16)?;
                            }
                            if let Some(ch) = char::from_u32(code) {
                                s.push(ch);
                            }
                        }
                        _ => return None,
                    }
                }
                c => s.push(c),
            }
        }
        Some(s)
    }
    fn number(&mut self) -> Option<J> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c=='-'||c=='+'||c=='.'||c=='e'||c=='E') {
            self.i += 1;
        }
        let txt: String = self.c[start..self.i].iter().collect();
        txt.parse::<f64>().ok().map(J::Num)
    }
}
