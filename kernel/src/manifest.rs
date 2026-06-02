//! Parse the `;; manifest:` header block at the top of a kernel `.smt2` file.
//!
//! Required keys in order:
//!   state-fields, effects-name, effect-enum-name, result-enum-name, max-effects

#[derive(Debug)]
pub struct Manifest {
    pub state_fields: Vec<(String, String)>,  // (name, type) — type as text
    pub effects_name: String,
    pub effect_enum_name: String,
    pub result_enum_name: String,
    pub max_effects: usize,
}

const REQUIRED: &[&str] = &[
    "state-fields",
    "effects-name",
    "effect-enum-name",
    "result-enum-name",
    "max-effects",
];

pub fn parse(src: &str) -> Result<Manifest, String> {
    let mut state_fields: Vec<(String, String)> = Vec::new();
    let mut effects_name = String::new();
    let mut effect_enum_name = String::new();
    let mut result_enum_name = String::new();
    let mut max_effects: usize = 0;
    let mut expected_idx = 0usize;

    for (lineno, line) in src.lines().enumerate() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix(";;") else { continue };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix("manifest:") else { continue };
        let rest = rest.trim();

        let (key, value) = match rest.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => return Err(format!("line {}: missing `=` in `{rest}`", lineno + 1)),
        };

        if expected_idx >= REQUIRED.len() {
            return Err(format!("line {}: extra manifest key `{key}`", lineno + 1));
        }
        let expected = REQUIRED[expected_idx];
        if key != expected {
            return Err(format!("line {}: key `{key}` at unexpected position; expected `{expected}`", lineno + 1));
        }
        expected_idx += 1;

        match key {
            "state-fields" => {
                if value.is_empty() {
                    state_fields = Vec::new();
                } else {
                    for pair in value.split_whitespace() {
                        let (n, t) = pair.split_once(':')
                            .ok_or_else(|| format!("line {}: bad state-field pair `{pair}`", lineno + 1))?;
                        state_fields.push((n.to_string(), t.to_string()));
                    }
                }
            }
            "effects-name"     => effects_name = value.to_string(),
            "effect-enum-name" => effect_enum_name = value.to_string(),
            "result-enum-name" => result_enum_name = value.to_string(),
            "max-effects"      => max_effects = value.parse::<usize>()
                .map_err(|e| format!("line {}: max-effects not an int: {e}", lineno + 1))?,
            _ => unreachable!(),
        }
    }

    if expected_idx < REQUIRED.len() {
        return Err(format!("missing required key `{}`", REQUIRED[expected_idx]));
    }
    if effects_name.is_empty() || effect_enum_name.is_empty() || result_enum_name.is_empty() {
        return Err("required key has empty value".to_string());
    }

    Ok(Manifest {
        state_fields,
        effects_name,
        effect_enum_name,
        result_enum_name,
        max_effects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_manifest() {
        let src = "\
;; manifest: state-fields = mode:String count:Int
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 16

(assert true)
";
        let m = parse(src).unwrap();
        assert_eq!(m.state_fields, vec![
            ("mode".to_string(), "String".to_string()),
            ("count".to_string(), "Int".to_string()),
        ]);
        assert_eq!(m.effects_name, "effects");
        assert_eq!(m.effect_enum_name, "Effect");
        assert_eq!(m.result_enum_name, "Result");
        assert_eq!(m.max_effects, 16);
    }

    #[test]
    fn rejects_missing_key() {
        let src = "\
;; manifest: state-fields = mode:String
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
";
        assert!(parse(src).is_err());
    }

    #[test]
    fn rejects_out_of_order() {
        let src = "\
;; manifest: effects-name = effects
;; manifest: state-fields = mode:String
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 16
";
        assert!(parse(src).is_err());
    }

    #[test]
    fn empty_state_fields_allowed() {
        let src = "\
;; manifest: state-fields =
;; manifest: effects-name = effects
;; manifest: effect-enum-name = Effect
;; manifest: result-enum-name = Result
;; manifest: max-effects = 16
";
        let m = parse(src).unwrap();
        assert!(m.state_fields.is_empty());
    }
}
