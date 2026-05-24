//! Integration tests for the LLM functionizer (`functionize/llm.rs`).
//!
//! Demo problem: a small string-manipulation claim
//!
//! ```evident
//! claim greet
//!     name ∈ String
//!     greeting ∈ String = "Hello, " ++ name
//! ```
//!
//! which the default Cranelift functionizer refuses (string concat is
//! a gap) and which Z3 solves — so Z3 can act as the sampling oracle
//! while the LLM produces a fast native function.
//!
//! Most tests inject a deterministic `CodeGenerator` so the full
//! pipeline (sample → prompt → codegen → `rustc` cdylib → dlopen →
//! validate → call) runs offline, with no API key and no network.
//! `real_api_roundtrip` exercises the live Anthropic path and is a
//! no-op unless `ANTHROPIC_API_KEY` is set.
//!
//! Requires `rustc` on `PATH` (always true under `cargo test`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use z3::ast::Dynamic;
use z3::{Config, Context};

use evident_runtime::functionize::llm::{AnthropicGenerator, CodeGenerator, LlmFunctionizer};
use evident_runtime::functionize::Functionizer;
use evident_runtime::translate::{DatatypeRegistry, EnumRegistry};
use evident_runtime::z3_eval::{Z3Program, Z3Step};
use evident_runtime::{EvidentRuntime, Value};

/// The `greet` claim, as a hand-built `Z3Program`: `greeting =
/// concat("Hello, ", name)`. Borrows `ctx`.
fn greet_program(ctx: &Context) -> Z3Program<'_> {
    let name = z3::ast::String::new_const(ctx, "name");
    let prefix = z3::ast::String::from_str(ctx, "Hello, ").unwrap();
    let greeting = z3::ast::String::concat(ctx, &[&prefix, &name]);
    Z3Program {
        steps: vec![Z3Step::Scalar {
            var: "greeting".to_string(),
            expr: Dynamic::from_ast(&greeting),
        }],
        checks: vec![],
        predicates: vec![],
    }
}

/// Correct implementation — matches Z3 on every input.
const GOOD_SRC: &str = r#"fn compute(name: &str) -> String { format!("Hello, {}", name) }"#;
/// Wrong implementation — fails validation on (almost) every input.
const BAD_SRC: &str = r#"fn compute(name: &str) -> String { String::from("WRONG") }"#;

struct FixedGen(&'static str);
impl CodeGenerator for FixedGen {
    fn generate(&self, _prompt: &str) -> Option<String> { Some(self.0.to_string()) }
}

/// Records how many times the generator was consulted.
struct CountingGen { src: &'static str, calls: Arc<AtomicUsize> }
impl CodeGenerator for CountingGen {
    fn generate(&self, _prompt: &str) -> Option<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Some(self.src.to_string())
    }
}

// ─────────────────────────── acceptance #4 ─────────────────────────

#[test]
fn refuses_without_api_key() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let program = greet_program(&ctx);
    let enums = EnumRegistry::new();
    let datatypes: DatatypeRegistry = std::cell::RefCell::new(std::collections::HashMap::new());

    // No key → the real generator declines, so compile falls through.
    let fz = LlmFunctionizer::with_generator(Box::new(AnthropicGenerator::new(None)));
    assert!(fz.compile(&program, &enums, &datatypes).is_none(),
        "with no API key, compile must return None (fall through)");
}

// ─────────────────────────── acceptance #5 ─────────────────────────

#[test]
fn good_output_validates_and_runs() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let program = greet_program(&ctx);
    let enums = EnumRegistry::new();
    let datatypes: DatatypeRegistry = std::cell::RefCell::new(std::collections::HashMap::new());

    let fz = LlmFunctionizer::with_generator(Box::new(FixedGen(GOOD_SRC)));
    let compiled = fz.compile(&program, &enums, &datatypes)
        .expect("a correct compute() should pass validation and return a callable");

    for who in ["World", "", "Ada Lovelace", "z3"] {
        let mut given = HashMap::new();
        given.insert("name".to_string(), Value::Str(who.to_string()));
        let out = compiled.call(&given).expect("call should succeed");
        assert_eq!(out.get("greeting"),
            Some(&Value::Str(format!("Hello, {who}"))),
            "compiled fn wrong for name={who:?}");
    }
}

// ─────────────────────────── acceptance #6 ─────────────────────────

#[test]
fn bad_output_fails_validation() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let program = greet_program(&ctx);
    let enums = EnumRegistry::new();
    let datatypes: DatatypeRegistry = std::cell::RefCell::new(std::collections::HashMap::new());

    // A function that's wrong everywhere must be rejected by the
    // validation gate — compile returns None (graceful fallback).
    let fz = LlmFunctionizer::with_generator(Box::new(FixedGen(BAD_SRC)));
    assert!(fz.compile(&program, &enums, &datatypes).is_none(),
        "hallucinated logic must fail validation and return None");
}

// ─────────────────────── acceptance #3 (opt-in wiring) ─────────────

#[test]
fn runtime_routes_query_through_llm_functionizer() {
    let calls = Arc::new(AtomicUsize::new(0));
    let gen = CountingGen { src: GOOD_SRC, calls: calls.clone() };
    let mut rt = EvidentRuntime::with_functionizer(
        Box::new(LlmFunctionizer::with_generator(Box::new(gen))));
    rt.load_source(
        "claim greet\n    name ∈ String\n    greeting ∈ String = \"Hello, \" ++ name\n")
        .expect("load");

    let mut given = HashMap::new();
    given.insert("name".to_string(), Value::Str("World".to_string()));
    let r = rt.query("greet", &given).expect("query");

    assert!(r.satisfied);
    assert_eq!(r.bindings.get("greeting"), Some(&Value::Str("Hello, World".to_string())));
    assert!(calls.load(Ordering::SeqCst) >= 1,
        "the runtime should have routed extraction to the LLM functionizer");
}

#[test]
fn bad_llm_output_still_yields_correct_query_via_fallback() {
    // Even with a wrong LLM function, the query result is correct
    // because validation rejects it and the runtime falls back to the
    // full Z3 solve — proving the gate protects correctness end-to-end.
    let mut rt = EvidentRuntime::with_functionizer(
        Box::new(LlmFunctionizer::with_generator(Box::new(FixedGen(BAD_SRC)))));
    rt.load_source(
        "claim greet\n    name ∈ String\n    greeting ∈ String = \"Hello, \" ++ name\n")
        .expect("load");

    let mut given = HashMap::new();
    given.insert("name".to_string(), Value::Str("World".to_string()));
    let r = rt.query("greet", &given).expect("query");

    assert!(r.satisfied);
    assert_eq!(r.bindings.get("greeting"), Some(&Value::Str("Hello, World".to_string())),
        "fallback must produce the correct answer, not the hallucinated one");
}

// ───────────────────────── live API (gated) ────────────────────────

#[test]
fn real_api_roundtrip() {
    if std::env::var("ANTHROPIC_API_KEY").map(|k| k.is_empty()).unwrap_or(true) {
        eprintln!("real_api_roundtrip: ANTHROPIC_API_KEY unset — skipping live call");
        return;
    }
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let program = greet_program(&ctx);
    let enums = EnumRegistry::new();
    let datatypes: DatatypeRegistry = std::cell::RefCell::new(std::collections::HashMap::new());

    let fz = LlmFunctionizer::new();
    let compiled = fz.compile(&program, &enums, &datatypes)
        .expect("the live LLM should produce a function that passes validation");

    let mut given = HashMap::new();
    given.insert("name".to_string(), Value::Str("World".to_string()));
    let out = compiled.call(&given).expect("call");
    assert_eq!(out.get("greeting"), Some(&Value::Str("Hello, World".to_string())));
}
