//! `CVal` — the engine-neutral contract value.
//!
//! Mirrors the tagged-JSON `Value` encoding in `FORMAT.md` §3, but is owned by
//! this crate so it depends on neither `evident_runtime::Value` (strategy 2) nor
//! `runtime_smt::Value` (strategy 1). Each engine adapter converts its native
//! model values into `CVal` (or compares via [`CVal::canonical`]) so the matrix
//! runner can diff every engine against one golden representation.

use serde_json::Value as Json;

/// A decoded contract value. The subset that appears in `meta.json`'s `given` /
/// `expect` blocks across the 15 fixtures, plus the seq/set shapes the format
/// documents (kept for completeness even where unused today).
#[derive(Debug, Clone, PartialEq)]
pub enum CVal {
    Int(i64),
    Bool(bool),
    Real(f64),
    Str(String),
    SeqInt(Vec<i64>),
    SeqBool(Vec<bool>),
    SeqStr(Vec<String>),
    SetInt(Vec<i64>),
    SetBool(Vec<bool>),
    SetStr(Vec<String>),
    /// `{"enum":"Name","variant":"Ctor","fields":[…]}`. `enum_name` is kept for
    /// provenance; canonical comparison uses only `variant` + `fields` (two
    /// structurally-equal values of the same shape are the same vertex — the
    /// FSM state/effect identity the contract checks).
    Enum {
        enum_name: String,
        variant: String,
        fields: Vec<CVal>,
    },
    /// `{"seq_enum":"Name","elems":[…]}` — element enum named once.
    SeqEnum(Vec<CVal>),
    /// `{"composite":{field:Value,…}}` — bare field names.
    Composite(Vec<(String, CVal)>),
    /// `{"seq_composite":[{field:Value},…]}`.
    SeqComposite(Vec<Vec<(String, CVal)>>),
}

impl CVal {
    /// Parse one tagged-JSON object into a `CVal`. Panics (with `ctx`) on an
    /// unknown / malformed tag so a bad fixture fails loudly — same policy as
    /// the original `behavior_contract.rs` loader.
    pub fn from_tagged_json(j: &Json, ctx: &str) -> CVal {
        let obj = j
            .as_object()
            .unwrap_or_else(|| panic!("{ctx}: value must be a tagged object, got {j}"));
        let int = |v: &Json| v.as_i64().unwrap_or_else(|| panic!("{ctx}: expected int, got {v}"));

        if let Some(v) = obj.get("int") {
            return CVal::Int(int(v));
        }
        if let Some(v) = obj.get("bool") {
            return CVal::Bool(v.as_bool().unwrap_or_else(|| panic!("{ctx}: bool")));
        }
        if let Some(v) = obj.get("real") {
            return CVal::Real(v.as_f64().unwrap_or_else(|| panic!("{ctx}: real")));
        }
        if let Some(v) = obj.get("str") {
            return CVal::Str(v.as_str().unwrap_or_else(|| panic!("{ctx}: str")).to_string());
        }
        if let Some(Json::Array(a)) = obj.get("seq_int") {
            return CVal::SeqInt(a.iter().map(int).collect());
        }
        if let Some(Json::Array(a)) = obj.get("seq_bool") {
            return CVal::SeqBool(a.iter().map(|v| v.as_bool().unwrap()).collect());
        }
        if let Some(Json::Array(a)) = obj.get("seq_str") {
            return CVal::SeqStr(a.iter().map(|v| v.as_str().unwrap().to_string()).collect());
        }
        if let Some(Json::Array(a)) = obj.get("set_int") {
            return CVal::SetInt(a.iter().map(int).collect());
        }
        if let Some(Json::Array(a)) = obj.get("set_bool") {
            return CVal::SetBool(a.iter().map(|v| v.as_bool().unwrap()).collect());
        }
        if let Some(Json::Array(a)) = obj.get("set_str") {
            return CVal::SetStr(a.iter().map(|v| v.as_str().unwrap().to_string()).collect());
        }
        if obj.contains_key("seq_enum") {
            let elems = match obj.get("elems") {
                Some(Json::Array(a)) => a.iter().map(|v| CVal::from_tagged_json(v, ctx)).collect(),
                _ => Vec::new(),
            };
            return CVal::SeqEnum(elems);
        }
        if let Some(name) = obj.get("enum").and_then(|v| v.as_str()) {
            let variant = obj
                .get("variant")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("{ctx}: enum missing variant"))
                .to_string();
            let fields = match obj.get("fields") {
                Some(Json::Array(a)) => a.iter().map(|v| CVal::from_tagged_json(v, ctx)).collect(),
                _ => Vec::new(),
            };
            return CVal::Enum { enum_name: name.to_string(), variant, fields };
        }
        if let Some(Json::Object(m)) = obj.get("composite") {
            return CVal::Composite(
                m.iter().map(|(k, v)| (k.clone(), CVal::from_tagged_json(v, ctx))).collect(),
            );
        }
        if let Some(Json::Array(a)) = obj.get("seq_composite") {
            let v = a
                .iter()
                .map(|el| {
                    el.as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), CVal::from_tagged_json(v, ctx)))
                        .collect()
                })
                .collect();
            return CVal::SeqComposite(v);
        }
        panic!("{ctx}: unrecognized tagged value: {j}");
    }

    /// A canonical, engine-independent string form used for equality in the
    /// matrix diff. Chosen so that BOTH engines' native renderings collapse to
    /// the same string:
    ///   * `Enum` nullary → `Done`; applied → `Count(4)` / `Println("hi")`.
    ///   * `Int` → `5`; `Bool` → `true`; `Str` → `"hi"` (Rust-debug escaped).
    ///   * `SeqEnum` → `[Println("hi"), Exit(0)]`.
    /// String payloads are quoted+escaped exactly like `expected_effects.txt`
    /// (FORMAT.md §6, backslash escaping), so effect-line comparison is direct.
    pub fn canonical(&self) -> String {
        match self {
            CVal::Int(n) => n.to_string(),
            CVal::Bool(b) => b.to_string(),
            CVal::Real(r) => r.to_string(),
            CVal::Str(s) => format!("{s:?}"),
            CVal::SeqInt(xs) => format!("[{}]", join(xs.iter().map(|x| x.to_string()))),
            CVal::SeqBool(xs) => format!("[{}]", join(xs.iter().map(|x| x.to_string()))),
            CVal::SeqStr(xs) => format!("[{}]", join(xs.iter().map(|x| format!("{x:?}")))),
            CVal::SetInt(xs) => {
                let mut v: Vec<i64> = xs.clone();
                v.sort_unstable();
                format!("{{{}}}", join(v.iter().map(|x| x.to_string())))
            }
            CVal::SetBool(xs) => format!("{{{}}}", join(xs.iter().map(|x| x.to_string()))),
            CVal::SetStr(xs) => {
                let mut v: Vec<String> = xs.clone();
                v.sort();
                format!("{{{}}}", join(v.iter().map(|x| format!("{x:?}"))))
            }
            CVal::Enum { variant, fields, .. } => {
                if fields.is_empty() {
                    variant.clone()
                } else {
                    format!("{variant}({})", join(fields.iter().map(|f| f.canonical())))
                }
            }
            CVal::SeqEnum(elems) => format!("[{}]", join(elems.iter().map(|e| e.canonical()))),
            CVal::Composite(fs) => {
                let mut fs: Vec<_> = fs.clone();
                fs.sort_by(|a, b| a.0.cmp(&b.0));
                format!("{{{}}}", join(fs.iter().map(|(k, v)| format!("{k}: {}", v.canonical()))))
            }
            CVal::SeqComposite(rows) => {
                let render = |row: &Vec<(String, CVal)>| {
                    let mut r = row.clone();
                    r.sort_by(|a, b| a.0.cmp(&b.0));
                    format!("{{{}}}", join(r.iter().map(|(k, v)| format!("{k}: {}", v.canonical()))))
                };
                format!("[{}]", join(rows.iter().map(render)))
            }
        }
    }

    /// Render a single effect value as one `expected_effects.txt` line
    /// (FORMAT.md §6): `Variant`, `Variant(payload)`. Same as `canonical` for
    /// an `Enum`, exposed by name so effect-diffing reads clearly.
    pub fn effect_line(&self) -> String {
        self.canonical()
    }
}

fn join(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enum_nullary_and_applied_canonical() {
        let done = CVal::from_tagged_json(
            &json!({"enum":"S","variant":"Done","fields":[]}),
            "t",
        );
        assert_eq!(done.canonical(), "Done");
        let count4 = CVal::from_tagged_json(
            &json!({"enum":"S","variant":"Count","fields":[{"int":4}]}),
            "t",
        );
        assert_eq!(count4.canonical(), "Count(4)");
    }

    #[test]
    fn effect_line_matches_format_grammar() {
        let p = CVal::from_tagged_json(
            &json!({"enum":"Effect","variant":"Println","fields":[{"str":"hi there"}]}),
            "t",
        );
        assert_eq!(p.effect_line(), "Println(\"hi there\")");
        let e = CVal::from_tagged_json(
            &json!({"enum":"Effect","variant":"Exit","fields":[{"int":42}]}),
            "t",
        );
        assert_eq!(e.effect_line(), "Exit(42)");
    }

    #[test]
    fn scalar_and_seq_canonical() {
        assert_eq!(CVal::Int(8).canonical(), "8");
        assert_eq!(CVal::Bool(true).canonical(), "true");
        let seq = CVal::from_tagged_json(
            &json!({"seq_enum":"Effect","elems":[
                {"enum":"Effect","variant":"IntToStr","fields":[{"int":7}]}]}),
            "t",
        );
        assert_eq!(seq.canonical(), "[IntToStr(7)]");
    }
}
