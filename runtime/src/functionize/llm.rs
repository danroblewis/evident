//! LLM functionizer: generate → compile via `rustc` → validate against held-out Z3 pairs.
//! Scalar I/O only; compiled code runs native (validation guards correctness, not safety).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use z3::ast::{Ast, Bool, Dynamic};
use z3::{AstKind, Context};
use z3_sys::DeclKind;

use crate::core::{EnumRegistry, Value, Z3Program, Z3Step};
use super::{CompiledFunction, Functionizer};

const N_SAMPLES: usize = 24;
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";

/// Value shapes marshaled across the FFI boundary. Classified from Z3 sort name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScalarTy { Int, Bool, Str }

impl ScalarTy {
    fn from_sort_str(s: &str) -> Option<ScalarTy> {
        match s {
            "Int" => Some(ScalarTy::Int),
            "Bool" => Some(ScalarTy::Bool),
            "String" => Some(ScalarTy::Str),
            _ => None,
        }
    }
    fn rust_param(&self) -> &'static str {
        match self { ScalarTy::Int => "i64", ScalarTy::Bool => "bool", ScalarTy::Str => "&str" }
    }
    fn rust_ret(&self) -> &'static str {
        match self { ScalarTy::Int => "i64", ScalarTy::Bool => "bool", ScalarTy::Str => "String" }
    }
}

/// Produces Rust source for a function. Generic so tests can inject a deterministic mock.
pub trait CodeGenerator {
    /// Return Rust source containing `fn compute(..)`, or `None` to refuse.
    fn generate(&self, prompt: &str) -> Option<String>;
}

/// Calls the Anthropic Messages API. Key from `ANTHROPIC_API_KEY`; never logged.
/// With no key, `generate` returns `None` without a network call.
pub struct AnthropicGenerator {
    api_key: Option<String>,
    model:   String,
}

impl AnthropicGenerator {
    pub fn from_env() -> Self {
        let key = std::env::var("ANTHROPIC_API_KEY").ok()
            .filter(|k| !k.is_empty());
        AnthropicGenerator { api_key: key, model: DEFAULT_MODEL.to_string() }
    }
    pub fn new(api_key: Option<String>) -> Self {
        AnthropicGenerator { api_key, model: DEFAULT_MODEL.to_string() }
    }
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string(); self
    }
}

impl CodeGenerator for AnthropicGenerator {
    fn generate(&self, prompt: &str) -> Option<String> {
        let key = self.api_key.as_ref()?;
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 1024,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let resp = ureq::post("https://api.anthropic.com/v1/messages")
            .set("x-api-key", key)
            .set("anthropic-version", "2023-06-01")
            .set("content-type", "application/json")
            .send_json(body);
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                if trace() { eprintln!("[fz/llm] API request failed: {e}"); }
                return None;
            }
        };
        let json: serde_json::Value = resp.into_json().ok()?;
        let text = json.get("content")?.get(0)?.get("text")?.as_str()?;
        Some(extract_code_block(text))
    }
}

/// Extract the first fenced code block, or the whole string if no fences.
fn extract_code_block(text: &str) -> String {
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let after = match after.find('\n') { Some(nl) => &after[nl + 1..], None => after };
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    text.trim().to_string()
}

/// LLM-generated-function functionizer strategy. See module docs.
pub struct LlmFunctionizer {
    generator: Box<dyn CodeGenerator>,
}

impl LlmFunctionizer {
    pub fn new() -> Self {
        LlmFunctionizer { generator: Box::new(AnthropicGenerator::from_env()) }
    }
    pub fn with_generator(generator: Box<dyn CodeGenerator>) -> Self {
        LlmFunctionizer { generator }
    }
}

impl Default for LlmFunctionizer {
    fn default() -> Self { Self::new() }
}

impl Functionizer for LlmFunctionizer {
    fn name(&self) -> &'static str { "llm" }

    fn compile(&self, program: &Z3Program, _enums: &EnumRegistry,
               _datatypes: &crate::core::DatatypeRegistry)
        -> Option<Rc<dyn CompiledFunction>>
    {
        let plan = Plan::from_program(program)?;
        let samples = plan.sample(program)?;
        let (train, holdout) = split_samples(&samples);

        let prompt = plan.build_prompt(train);
        let user_code = self.generator.generate(&prompt)?;

        let source = plan.assemble_source(&user_code);
        let compiled = compile_cdylib(&plan, &source)?;

        // Validation gate: 100% match required on held-out pairs; any miss → reject.
        for (inputs, expected) in holdout {
            let Some(got) = compiled.call(inputs) else {
                if trace() { eprintln!("[fz/llm] validation: call returned None"); }
                return None;
            };
            for (name, _) in &plan.outputs {
                if got.get(name) != expected.get(name) {
                    if trace() {
                        eprintln!("[fz/llm] validation miss on {name}: got {:?} want {:?}",
                            got.get(name), expected.get(name));
                    }
                    return None;
                }
            }
        }
        if trace() {
            eprintln!("[fz/llm] accepted: {} inputs, {} outputs, validated on {} held-out pairs",
                plan.inputs.len(), plan.outputs.len(), holdout.len());
        }
        Some(Rc::new(compiled))
    }
}

/// Typed I/O signature (stable sorted order) plus Z3 const handles for sampling.
/// `rust_inputs`/`rust_outputs` are collision-free Rust identifiers for codegen.
struct Plan<'ctx> {
    inputs:  Vec<(String, ScalarTy)>,
    outputs: Vec<(String, ScalarTy)>,
    rust_inputs:  Vec<String>,
    rust_outputs: Vec<String>,
    input_consts:  Vec<Dynamic<'ctx>>,
    output_consts: Vec<Dynamic<'ctx>>,
}

impl<'ctx> Plan<'ctx> {
    fn from_program(program: &Z3Program<'ctx>) -> Option<Plan<'ctx>> {
        let mut out_pairs: Vec<(String, ScalarTy, Dynamic<'ctx>)> = Vec::new();
        let mut output_names: HashSet<String> = HashSet::new();
        let ctx: &'ctx Context = match program.steps.first() {
            Some(Z3Step::Scalar { expr, .. }) => expr.get_ctx(),
            _ => return None,
        };
        for step in &program.steps {
            let Z3Step::Scalar { var, expr } = step else { return None };
            let ty = ScalarTy::from_sort_str(&format!("{}", expr.get_sort()))?;
            out_pairs.push((var.clone(), ty, expr.clone()));
            output_names.insert(var.clone());
        }

        let mut free: BTreeMap<String, Dynamic<'ctx>> = BTreeMap::new();
        for (_, _, expr) in &out_pairs {
            collect_free_consts(expr, &output_names, &mut free);
        }
        for (l, r) in &program.checks {
            collect_free_consts(l, &output_names, &mut free);
            collect_free_consts(r, &output_names, &mut free);
        }
        for p in &program.predicates {
            collect_free_consts(&Dynamic::from_ast(p), &output_names, &mut free);
        }

        let mut inputs: Vec<(String, ScalarTy)> = Vec::new();
        let mut input_consts: Vec<Dynamic<'ctx>> = Vec::new();
        for (name, dyn_const) in &free {
            let ty = ScalarTy::from_sort_str(&format!("{}", dyn_const.get_sort()))?;
            inputs.push((name.clone(), ty));
            input_consts.push(dyn_const.clone());
        }

        out_pairs.sort_by(|a, b| a.0.cmp(&b.0)); // stable wire order
        let outputs: Vec<(String, ScalarTy)> =
            out_pairs.iter().map(|(n, t, _)| (n.clone(), *t)).collect();
        let output_consts: Vec<Dynamic<'ctx>> = out_pairs.iter()
            .map(|(n, t, _)| make_const(ctx, n, *t)).collect();

        let rust_inputs  = unique_idents(inputs.iter().map(|(n, _)| n.as_str()), 'a');
        let rust_outputs = unique_idents(outputs.iter().map(|(n, _)| n.as_str()), 'o');

        Some(Plan { inputs, outputs, rust_inputs, rust_outputs, input_consts, output_consts })
    }

    /// Draw up to `N_SAMPLES` distinct input→output pairs; Z3 is the oracle.
    fn sample(&self, program: &Z3Program<'ctx>) -> Option<Vec<Sample>> {
        let ctx = self.output_consts.first()
            .or_else(|| self.input_consts.first())
            .map(|d| d.get_ctx())?;
        let solver = z3::Solver::new(ctx);
        // Map each step's var to its sorted output slot, then assert output = defining expr.
        let out_slot: HashMap<&str, usize> = self.outputs.iter()
            .enumerate().map(|(i, (n, _))| (n.as_str(), i)).collect();
        for step in &program.steps {
            let Z3Step::Scalar { var, expr } = step else { return None };
            let slot = *out_slot.get(var.as_str())?;
            solver.assert(&self.output_consts[slot]._eq(expr));
        }
        for (l, r) in &program.checks { solver.assert(&l._eq(r)); }
        for p in &program.predicates { solver.assert(p); }

        let mut samples: Vec<Sample> = Vec::new();
        for _ in 0..N_SAMPLES {
            use z3::SatResult;
            if !matches!(solver.check(), SatResult::Sat) { break; }
            let Some(model) = solver.get_model() else { break };

            let mut inputs: HashMap<String, Value> = HashMap::new();
            let mut neqs: Vec<Bool<'ctx>> = Vec::new();
            let mut ok = true;
            for ((name, ty), c) in self.inputs.iter().zip(&self.input_consts) {
                let Some(mv) = model.eval(c, true) else { ok = false; break };
                let Some(v) = dyn_to_value(&mv, *ty) else { ok = false; break };
                inputs.insert(name.clone(), v);
                neqs.push(c._eq(&mv).not());
            }
            if !ok { break; }

            let mut outputs: HashMap<String, Value> = HashMap::new();
            for ((name, ty), c) in self.outputs.iter().zip(&self.output_consts) {
                let Some(mv) = model.eval(c, true) else { ok = false; break };
                let Some(v) = dyn_to_value(&mv, *ty) else { ok = false; break };
                outputs.insert(name.clone(), v);
            }
            if !ok { break; }

            samples.push((inputs, outputs));

            // Force the next model to differ in ≥1 input; no inputs = constant fn.
            if neqs.is_empty() { break; }
            let refs: Vec<&Bool<'ctx>> = neqs.iter().collect();
            solver.assert(&Bool::or(ctx, &refs));
        }
        if samples.is_empty() { None } else { Some(samples) }
    }

    fn signature(&self) -> String {
        let params: Vec<String> = self.inputs.iter().zip(&self.rust_inputs)
            .map(|((_, ty), id)| format!("{id}: {}", ty.rust_param()))
            .collect();
        let ret = match self.outputs.len() {
            1 => self.outputs[0].1.rust_ret().to_string(),
            _ => format!("({})",
                self.outputs.iter().map(|(_, t)| t.rust_ret())
                    .collect::<Vec<_>>().join(", ")),
        };
        format!("fn compute({}) -> {ret}", params.join(", "))
    }

    fn build_prompt(&self, train: &[Sample]) -> String {
        let mut p = String::new();
        p.push_str("You are given a pure function specification by example. \
            Write a single Rust function with EXACTLY this signature:\n\n    ");
        p.push_str(&self.signature());
        p.push_str("\n\nRules:\n\
            - Deterministic; standard library only; no I/O; must not panic.\n\
            - Do NOT write `main`, tests, external imports, or any other function.\n\
            - Return ONLY the `compute` function inside a ```rust code block.\n\n\
            It must satisfy every one of these input → output examples:\n\n");
        for (inputs, outputs) in train {
            p.push_str("    ");
            p.push_str(&self.example_call(inputs, outputs));
            p.push('\n');
        }
        p
    }

    fn example_call(&self, inputs: &HashMap<String, Value>, outputs: &HashMap<String, Value>) -> String {
        let args: Vec<String> = self.inputs.iter()
            .map(|(n, ty)| value_literal(inputs.get(n), *ty))
            .collect();
        let rhs = match self.outputs.len() {
            1 => value_literal(outputs.get(&self.outputs[0].0), self.outputs[0].1),
            _ => format!("({})", self.outputs.iter()
                .map(|(n, ty)| value_literal(outputs.get(n), *ty))
                .collect::<Vec<_>>().join(", ")),
        };
        format!("compute({}) == {rhs}", args.join(", "))
    }

    fn assemble_source(&self, user_code: &str) -> String {
        let mut s = String::new();
        s.push_str("#![allow(warnings)]\n\n");
        s.push_str(user_code);
        s.push_str("\n\n// ---- host-generated FFI shim (not from the LLM) ----\n");
        s.push_str(SHIM_HELPERS);

        let mut call = String::new();
        call.push_str("#[no_mangle]\npub extern \"C\" fn ev_llm_call(\
            in_ptr: *const u8, in_len: usize, out_len: *mut usize) -> *mut u8 {\n");
        call.push_str("    let __in: &[u8] = if in_ptr.is_null() { &[] } \
            else { unsafe { std::slice::from_raw_parts(in_ptr, in_len) } };\n");
        call.push_str("    let mut __p = 0usize;\n");
        for ((_, ty), id) in self.inputs.iter().zip(&self.rust_inputs) {
            let rd = match ty { ScalarTy::Int => "__ev_rd_i64", ScalarTy::Bool => "__ev_rd_bool",
                                ScalarTy::Str => "__ev_rd_str" };
            call.push_str(&format!("    let {id} = {rd}(__in, &mut __p);\n"));
        }
        let arg_exprs: Vec<String> = self.inputs.iter().zip(&self.rust_inputs)
            .map(|((_, ty), id)| if *ty == ScalarTy::Str { format!("&{id}") } else { id.clone() })
            .collect();
        let bind = match self.rust_outputs.len() {
            1 => self.rust_outputs[0].clone(),
            _ => format!("({})", self.rust_outputs.join(", ")),
        };
        call.push_str(&format!(
            "    let __res = std::panic::catch_unwind(|| compute({}));\n", arg_exprs.join(", ")));
        call.push_str(&format!(
            "    let {bind} = match __res {{ Ok(r) => r, Err(_) => {{ \
            unsafe {{ *out_len = 0; }} return std::ptr::null_mut(); }} }};\n"));
        call.push_str("    let mut __out: Vec<u8> = Vec::new();\n");
        for ((_, ty), id) in self.outputs.iter().zip(&self.rust_outputs) {
            match ty {
                ScalarTy::Int  => call.push_str(&format!("    __ev_wr_i64(&mut __out, {id});\n")),
                ScalarTy::Bool => call.push_str(&format!("    __ev_wr_bool(&mut __out, {id});\n")),
                ScalarTy::Str  => call.push_str(&format!("    __ev_wr_str(&mut __out, &{id});\n")),
            }
        }
        call.push_str("    let __b = __out.into_boxed_slice();\n");
        call.push_str("    unsafe { *out_len = __b.len(); }\n");
        call.push_str("    Box::into_raw(__b) as *mut u8\n}\n");
        s.push_str(&call);
        s
    }
}

type Sample = (HashMap<String, Value>, HashMap<String, Value>);

fn split_samples(samples: &[Sample]) -> (&[Sample], &[Sample]) {
    let total = samples.len();
    if total == 1 { return (samples, samples); }
    let mut split = (total * 3) / 5;
    if split == 0 { split = 1; }
    if split >= total { split = total - 1; }
    (&samples[..split], &samples[split..])
}

type CallFn = unsafe extern "C" fn(*const u8, usize, *mut usize) -> *mut u8;
type FreeFn = unsafe extern "C" fn(*mut u8, usize);

/// `dlopen`ed cdylib from LLM source; holds the library alive and marshals via the wire format.
struct LlmCompiled {
    _lib:    libloading::Library,
    call_fn: CallFn,
    free_fn: FreeFn,
    inputs:  Vec<(String, ScalarTy)>,
    outputs: Vec<(String, ScalarTy)>,
    tmp_dir: std::path::PathBuf,
}

impl Drop for LlmCompiled {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp_dir);
    }
}

impl CompiledFunction for LlmCompiled {
    fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        let mut buf: Vec<u8> = Vec::new();
        for (name, ty) in &self.inputs {
            encode_value(&mut buf, *ty, given.get(name)?)?;
        }
        let mut out_len: usize = 0;
        let ptr = unsafe { (self.call_fn)(buf.as_ptr(), buf.len(), &mut out_len) };
        if ptr.is_null() { return None; }   // compute panicked
        let bytes = unsafe { std::slice::from_raw_parts(ptr, out_len) }.to_vec();
        unsafe { (self.free_fn)(ptr, out_len); }

        let mut p = 0usize;
        let mut out: HashMap<String, Value> = HashMap::new();
        for (name, ty) in &self.outputs {
            out.insert(name.clone(), decode_value(&bytes, &mut p, *ty)?);
        }
        Some(out)
    }
}

fn compile_cdylib(plan: &Plan, source: &str) -> Option<LlmCompiled> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let uniq = format!("{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
    let dir = std::env::temp_dir().join(format!("evident-llm-{uniq}"));
    std::fs::create_dir_all(&dir).ok()?;
    let src_path = dir.join("f.rs");
    std::fs::write(&src_path, source).ok()?;
    let lib_path = dir.join(format!("{}evllm{}",
        std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX));

    let out = std::process::Command::new("rustc")
        .args(["--edition", "2021", "-O", "--crate-type", "cdylib"])
        .arg("-o").arg(&lib_path)
        .arg(&src_path)
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => { if trace() { eprintln!("[fz/llm] rustc spawn failed: {e}"); } return None; }
    };
    if !out.status.success() {
        if trace() {
            eprintln!("[fz/llm] rustc failed:\n{}", String::from_utf8_lossy(&out.stderr));
        }
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }

    // SAFETY: cdylib produced from host-generated source; symbols have the declared C ABI.
    let lib = unsafe { libloading::Library::new(&lib_path) }.ok()?;
    let call_fn: CallFn = unsafe {
        *lib.get::<CallFn>(b"ev_llm_call\0").ok()?
    };
    let free_fn: FreeFn = unsafe {
        *lib.get::<FreeFn>(b"ev_llm_free\0").ok()?
    };
    Some(LlmCompiled {
        _lib: lib, call_fn, free_fn,
        inputs: plan.inputs.clone(),
        outputs: plan.outputs.clone(),
        tmp_dir: dir,
    })
}

// Wire format: flat little-endian, no tags. Int=8B, Bool=1B, Str=u32len+UTF-8.
// Host side here; shim (SHIM_HELPERS) mirrors it.

fn encode_value(buf: &mut Vec<u8>, ty: ScalarTy, v: &Value) -> Option<()> {
    match (ty, v) {
        (ScalarTy::Int,  Value::Int(n))  => buf.extend_from_slice(&n.to_le_bytes()),
        (ScalarTy::Bool, Value::Bool(b)) => buf.push(*b as u8),
        (ScalarTy::Bool, Value::Int(n))  => buf.push((*n != 0) as u8),
        (ScalarTy::Str,  Value::Str(s))  => {
            buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
            buf.extend_from_slice(s.as_bytes());
        }
        _ => return None,
    }
    Some(())
}

fn decode_value(b: &[u8], p: &mut usize, ty: ScalarTy) -> Option<Value> {
    match ty {
        ScalarTy::Int => {
            let end = p.checked_add(8)?;
            let arr: [u8; 8] = b.get(*p..end)?.try_into().ok()?;
            *p = end;
            Some(Value::Int(i64::from_le_bytes(arr)))
        }
        ScalarTy::Bool => {
            let v = *b.get(*p)?;
            *p += 1;
            Some(Value::Bool(v != 0))
        }
        ScalarTy::Str => {
            let lend = p.checked_add(4)?;
            let larr: [u8; 4] = b.get(*p..lend)?.try_into().ok()?;
            let n = u32::from_le_bytes(larr) as usize;
            let send = lend.checked_add(n)?;
            let s = std::str::from_utf8(b.get(lend..send)?).ok()?.to_string();
            *p = send;
            Some(Value::Str(s))
        }
    }
}

/// Decode/encode helpers + `ev_llm_free` export; `__ev_`-prefixed to avoid colliding with `compute`.
const SHIM_HELPERS: &str = r#"
fn __ev_rd_i64(b: &[u8], p: &mut usize) -> i64 {
    let mut a = [0u8; 8]; a.copy_from_slice(&b[*p..*p + 8]); *p += 8; i64::from_le_bytes(a)
}
fn __ev_rd_bool(b: &[u8], p: &mut usize) -> bool { let v = b[*p] != 0; *p += 1; v }
fn __ev_rd_str(b: &[u8], p: &mut usize) -> String {
    let mut l = [0u8; 4]; l.copy_from_slice(&b[*p..*p + 4]); *p += 4;
    let n = u32::from_le_bytes(l) as usize;
    let s = String::from_utf8_lossy(&b[*p..*p + n]).into_owned(); *p += n; s
}
fn __ev_wr_i64(o: &mut Vec<u8>, v: i64) { o.extend_from_slice(&v.to_le_bytes()); }
fn __ev_wr_bool(o: &mut Vec<u8>, v: bool) { o.push(v as u8); }
fn __ev_wr_str(o: &mut Vec<u8>, v: &str) {
    o.extend_from_slice(&(v.len() as u32).to_le_bytes()); o.extend_from_slice(v.as_bytes());
}
#[no_mangle]
pub extern "C" fn ev_llm_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() { return; }
    unsafe { drop(Box::from_raw(std::slice::from_raw_parts_mut(ptr, len) as *mut [u8])); }
}
"#;

/// Collect 0-arity uninterpreted consts not in `outputs` into `acc` (first occurrence wins).
fn collect_free_consts<'ctx>(
    e: &Dynamic<'ctx>,
    outputs: &HashSet<String>,
    acc: &mut BTreeMap<String, Dynamic<'ctx>>,
) {
    if e.kind() == AstKind::App {
        if let Ok(decl) = e.safe_decl() {
            if decl.kind() == DeclKind::UNINTERPRETED && e.num_children() == 0 {
                let name = decl.name();
                if !outputs.contains(&name) {
                    acc.entry(name).or_insert_with(|| e.clone());
                }
                return;
            }
        }
        for c in e.children() {
            collect_free_consts(&c, outputs, acc);
        }
    }
}

fn make_const<'ctx>(ctx: &'ctx Context, name: &str, ty: ScalarTy) -> Dynamic<'ctx> {
    match ty {
        ScalarTy::Int  => Dynamic::from_ast(&z3::ast::Int::new_const(ctx, name)),
        ScalarTy::Bool => Dynamic::from_ast(&z3::ast::Bool::new_const(ctx, name)),
        ScalarTy::Str  => Dynamic::from_ast(&z3::ast::String::new_const(ctx, name)),
    }
}

fn dyn_to_value(d: &Dynamic, ty: ScalarTy) -> Option<Value> {
    match ty {
        ScalarTy::Int  => d.as_int().and_then(|i| i.as_i64()).map(Value::Int),
        ScalarTy::Bool => d.as_bool().and_then(|b| b.as_bool()).map(Value::Bool),
        ScalarTy::Str  => d.as_string().and_then(|s| s.as_string()).map(Value::Str),
    }
}

fn value_literal(v: Option<&Value>, ty: ScalarTy) -> String {
    match (ty, v) {
        (ScalarTy::Int,  Some(Value::Int(n)))  => n.to_string(),
        (ScalarTy::Bool, Some(Value::Bool(b))) => b.to_string(),
        (ScalarTy::Bool, Some(Value::Int(n)))  => (*n != 0).to_string(),
        (ScalarTy::Str,  Some(Value::Str(s)))  => format!("{s:?}"),
        (ScalarTy::Int, _)  => "0".to_string(),
        (ScalarTy::Bool, _) => "false".to_string(),
        (ScalarTy::Str, _)  => "\"\"".to_string(),
    }
}

fn unique_idents<'a>(names: impl Iterator<Item = &'a str>, prefix: char) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for (i, name) in names.enumerate() {
        let mut id: String = name.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        let bad_start = id.is_empty()
            || id.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true);
        if bad_start || is_rust_keyword(&id) {
            id = format!("{prefix}{i}_{id}");
        }
        while seen.contains(&id) {
            id = format!("{id}_{i}");
        }
        seen.insert(id.clone());
        out.push(id);
    }
    out
}

fn is_rust_keyword(s: &str) -> bool {
    matches!(s,
        "as"|"break"|"const"|"continue"|"crate"|"else"|"enum"|"extern"|"false"|"fn"|
        "for"|"if"|"impl"|"in"|"let"|"loop"|"match"|"mod"|"move"|"mut"|"pub"|"ref"|
        "return"|"self"|"Self"|"static"|"struct"|"super"|"trait"|"true"|"type"|
        "unsafe"|"use"|"where"|"while"|"async"|"await"|"dyn")
}

fn trace() -> bool { std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_code_block_strips_fences() {
        let txt = "Here you go:\n```rust\nfn compute() {}\n```\nDone.";
        assert_eq!(extract_code_block(txt), "fn compute() {}");
        assert_eq!(extract_code_block("fn compute() {}"), "fn compute() {}");
    }

    #[test]
    fn wire_roundtrip() {
        let mut b = Vec::new();
        encode_value(&mut b, ScalarTy::Int, &Value::Int(-7)).unwrap();
        encode_value(&mut b, ScalarTy::Str, &Value::Str("hi".into())).unwrap();
        encode_value(&mut b, ScalarTy::Bool, &Value::Bool(true)).unwrap();
        let mut p = 0;
        assert_eq!(decode_value(&b, &mut p, ScalarTy::Int), Some(Value::Int(-7)));
        assert_eq!(decode_value(&b, &mut p, ScalarTy::Str), Some(Value::Str("hi".into())));
        assert_eq!(decode_value(&b, &mut p, ScalarTy::Bool), Some(Value::Bool(true)));
    }

    #[test]
    fn anthropic_generator_refuses_without_key() {
        let g = AnthropicGenerator::new(None);
        assert!(g.generate("anything").is_none());
    }

    #[test]
    fn unique_idents_dedup_and_sanitize() {
        let ids = unique_idents(["a.b", "a_b", "3x", "type"].into_iter(), 'a');
        assert_eq!(ids.len(), 4);
        // distinct
        let set: HashSet<_> = ids.iter().collect();
        assert_eq!(set.len(), 4);
        // no leading digit / keyword left bare
        assert!(!ids.iter().any(|i| i == "type"));
        assert!(!ids.iter().any(|i| i.chars().next().unwrap().is_ascii_digit()));
    }
}
