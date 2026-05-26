//! GLSL functionizer (macOS, headless CGL): transpiles pure scalar Int/Bool `Z3Program`s to GLSL;
//! runs a 1×N draw pass, reads back results via `RGBA32I` texture + `glReadPixels`.

use std::collections::{BTreeMap, HashMap};
use std::ffi::{c_void, CStr, CString};
use std::os::raw::{c_char, c_int, c_uint};
use std::ptr;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard, OnceLock};

use z3::ast::{Ast, Dynamic};
use z3::AstKind;
use z3_sys::DeclKind;

use crate::core::{DatatypeRegistry, EnumRegistry, Value, Z3Program, Z3Step};

// CGL FFI (headless GL context on macOS)

type CGLPixelFormatObj = *mut c_void;
type CGLContextObj = *mut c_void;
type CGLError = c_int;
type CGLPixelFormatAttribute = c_int;

const KCGL_PFA_OPENGL_PROFILE: CGLPixelFormatAttribute = 99;
const KCGL_PFA_COLOR_SIZE: CGLPixelFormatAttribute = 8;
const KCGL_PFA_DEPTH_SIZE: CGLPixelFormatAttribute = 12;
const KCGL_PFA_ACCELERATED: CGLPixelFormatAttribute = 73;
/// 3.2-core CGL selector; driver reports GL 4.1 / GLSL 4.10 on Apple Silicon.
const KCGL_OGL_PROFILE_3_2_CORE: c_int = 0x3200;

#[link(name = "OpenGL", kind = "framework")]
extern "C" {
    fn CGLChoosePixelFormat(
        attribs: *const CGLPixelFormatAttribute,
        pix: *mut CGLPixelFormatObj,
        npix: *mut c_int,
    ) -> CGLError;
    fn CGLCreateContext(
        pix: CGLPixelFormatObj,
        share: CGLContextObj,
        ctx: *mut CGLContextObj,
    ) -> CGLError;
    fn CGLSetCurrentContext(ctx: CGLContextObj) -> CGLError;
    fn CGLErrorString(error: CGLError) -> *const c_char;
}

extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

fn cgl_err(e: CGLError) -> String {
    if e == 0 {
        return "kCGLNoError".into();
    }
    unsafe {
        let s = CGLErrorString(e);
        if s.is_null() {
            format!("CGLError({e})")
        } else {
            CStr::from_ptr(s).to_string_lossy().into_owned()
        }
    }
}

// Process-wide GL context (mutex-serialized)

/// Wraps raw CGL context; `Send` is sound because it's only dereferenced
/// while the owning `Mutex` is held and the context is current on that thread.
struct GlState {
    ctx: CGLContextObj,
}
unsafe impl Send for GlState {}

static GL: OnceLock<Result<Mutex<GlState>, String>> = OnceLock::new();

fn init_gl() -> Result<Mutex<GlState>, String> {
    let attribs: [CGLPixelFormatAttribute; 9] = [
        KCGL_PFA_OPENGL_PROFILE,
        KCGL_OGL_PROFILE_3_2_CORE,
        KCGL_PFA_COLOR_SIZE,
        24,
        KCGL_PFA_DEPTH_SIZE,
        0,
        KCGL_PFA_ACCELERATED,
        0, // accelerated takes no value; 0 here, list terminator next
        0,
    ];
    unsafe {
        let mut pix: CGLPixelFormatObj = ptr::null_mut();
        let mut npix: c_int = 0;
        let e = CGLChoosePixelFormat(attribs.as_ptr(), &mut pix, &mut npix);
        if e != 0 || pix.is_null() {
            return Err(format!(
                "CGLChoosePixelFormat failed: {} (no headless GL context available)",
                cgl_err(e)
            ));
        }
        let mut ctx: CGLContextObj = ptr::null_mut();
        let e = CGLCreateContext(pix, ptr::null_mut(), &mut ctx);
        if e != 0 || ctx.is_null() {
            return Err(format!("CGLCreateContext failed: {}", cgl_err(e)));
        }
        let e = CGLSetCurrentContext(ctx);
        if e != 0 {
            return Err(format!("CGLSetCurrentContext failed: {}", cgl_err(e)));
        }
        let path = CString::new("/System/Library/Frameworks/OpenGL.framework/OpenGL").unwrap();
        let handle = dlopen(path.as_ptr(), 0x1 /* RTLD_LAZY */);
        gl::load_with(|name| {
            let cname = CString::new(name).unwrap();
            let mut p = dlsym(handle, cname.as_ptr());
            if p.is_null() {
                p = dlsym(ptr::null_mut(), cname.as_ptr());
            }
            p as *const c_void
        });
        Ok(Mutex::new(GlState { ctx }))
    }
}

/// Holds the GL mutex and keeps the context current; detaches on drop so
/// a later `gl_session` on a different thread can re-attach.
struct GlSession {
    _guard: MutexGuard<'static, GlState>,
}

impl Drop for GlSession {
    fn drop(&mut self) {
        unsafe {
            CGLSetCurrentContext(ptr::null_mut());
        }
    }
}

/// Acquire the process-wide GL context (initializing on first use), or `Err`
/// if no headless context is available.
fn gl_session() -> Result<GlSession, String> {
    let cell = GL.get_or_init(init_gl);
    match cell {
        Ok(m) => {
            let guard = m.lock().map_err(|_| "GL context mutex poisoned".to_string())?;
            unsafe {
                CGLSetCurrentContext(guard.ctx);
            }
            Ok(GlSession { _guard: guard })
        }
        Err(e) => Err(e.clone()),
    }
}

// Scalar kind

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalarKind {
    Int,
    Bool,
}

fn scalar_kind(sort_name: &str) -> Option<ScalarKind> {
    match sort_name {
        "Int" => Some(ScalarKind::Int),
        "Bool" => Some(ScalarKind::Bool),
        _ => None,
    }
}

// Z3 AST → GLSL expression string

/// Emit a GLSL `int` expression for `expr`. `env` maps Z3 variable names to
/// GLSL identifiers (`inN` = input uniform, `vN` = earlier output local).
fn emit_glsl(expr: &Dynamic, env: &HashMap<String, String>) -> Option<String> {
    match expr.kind() {
        AstKind::Numeral => {
            let i = expr.as_int().and_then(|x| x.as_i64())?;
            // GLSL `int` is 32-bit; refuse literals that don't fit.
            i32::try_from(i).ok()?;
            Some(format!("{i}"))
        }
        AstKind::App => {
            let decl = expr.safe_decl().ok()?;
            let kind = decl.kind();
            let ch: Vec<Dynamic> = expr.children();
            match kind {
                DeclKind::TRUE => Some("1".to_string()),
                DeclKind::FALSE => Some("0".to_string()),
                DeclKind::UNINTERPRETED => {
                    if !ch.is_empty() {
                        return None; // only 0-arity refs supported
                    }
                    env.get(&decl.name()).cloned()
                }
                DeclKind::ADD => fold(&ch, env, " + ", "0"),
                DeclKind::SUB => fold(&ch, env, " - ", "0"),
                DeclKind::MUL => fold(&ch, env, " * ", "1"),
                DeclKind::AND => fold(&ch, env, " & ", "1"),
                DeclKind::OR => fold(&ch, env, " | ", "0"),
                DeclKind::UMINUS => {
                    if ch.len() != 1 {
                        return None;
                    }
                    Some(format!("(-{})", emit_glsl(&ch[0], env)?))
                }
                DeclKind::NOT => {
                    if ch.len() != 1 {
                        return None;
                    }
                    // Bool is 0/1; XOR with 1 flips it (matches Cranelift `bxor`).
                    Some(format!("({} ^ 1)", emit_glsl(&ch[0], env)?))
                }
                DeclKind::IDIV | DeclKind::DIV => binop(&ch, env, "/"),
                DeclKind::MOD | DeclKind::REM => binop(&ch, env, "%"),
                DeclKind::LT => cmp(&ch, env, "<"),
                DeclKind::LE => cmp(&ch, env, "<="),
                DeclKind::GT => cmp(&ch, env, ">"),
                DeclKind::GE => cmp(&ch, env, ">="),
                DeclKind::EQ => cmp(&ch, env, "=="),
                DeclKind::ITE => {
                    if ch.len() != 3 {
                        return None;
                    }
                    let c = emit_glsl(&ch[0], env)?;
                    let t = emit_glsl(&ch[1], env)?;
                    let e = emit_glsl(&ch[2], env)?;
                    Some(format!("(({c}) != 0 ? {t} : {e})"))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn fold(ch: &[Dynamic], env: &HashMap<String, String>, sep: &str, empty: &str) -> Option<String> {
    if ch.is_empty() {
        return Some(empty.to_string());
    }
    let parts: Option<Vec<String>> = ch.iter().map(|c| emit_glsl(c, env)).collect();
    Some(format!("({})", parts?.join(sep)))
}

fn binop(ch: &[Dynamic], env: &HashMap<String, String>, op: &str) -> Option<String> {
    if ch.len() != 2 {
        return None;
    }
    Some(format!(
        "({} {op} {})",
        emit_glsl(&ch[0], env)?,
        emit_glsl(&ch[1], env)?
    ))
}

/// Comparison yields `int` 0/1 to compose with bitwise logic and ITE, like Cranelift.
fn cmp(ch: &[Dynamic], env: &HashMap<String, String>, op: &str) -> Option<String> {
    if ch.len() != 2 {
        return None;
    }
    Some(format!(
        "(({} {op} {}) ? 1 : 0)",
        emit_glsl(&ch[0], env)?,
        emit_glsl(&ch[1], env)?
    ))
}

fn collect_free_consts(d: &Dynamic, out: &mut BTreeMap<String, String>) {
    if d.kind() == AstKind::App {
        if let Ok(decl) = d.safe_decl() {
            if decl.kind() == DeclKind::UNINTERPRETED && d.num_children() == 0 {
                out.insert(decl.name(), format!("{}", d.get_sort()));
                return;
            }
        }
        for c in d.children() {
            collect_free_consts(&c, out);
        }
    }
}

// GL helpers

unsafe fn compile_shader(src: &str, kind: c_uint) -> Result<c_uint, String> {
    let s = gl::CreateShader(kind);
    let csrc = CString::new(src).map_err(|_| "shader source had NUL".to_string())?;
    gl::ShaderSource(s, 1, [csrc.as_ptr()].as_ptr(), ptr::null());
    gl::CompileShader(s);
    let mut ok: c_int = 0;
    gl::GetShaderiv(s, gl::COMPILE_STATUS, &mut ok);
    if ok == 0 {
        let mut len: c_int = 0;
        gl::GetShaderiv(s, gl::INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; len.max(1) as usize];
        gl::GetShaderInfoLog(s, len, ptr::null_mut(), buf.as_mut_ptr() as *mut c_char);
        gl::DeleteShader(s);
        return Err(String::from_utf8_lossy(&buf).into_owned());
    }
    Ok(s)
}

unsafe fn link_program(vs: c_uint, fs: c_uint) -> Result<c_uint, String> {
    let p = gl::CreateProgram();
    gl::AttachShader(p, vs);
    gl::AttachShader(p, fs);
    gl::LinkProgram(p);
    let mut ok: c_int = 0;
    gl::GetProgramiv(p, gl::LINK_STATUS, &mut ok);
    if ok == 0 {
        let mut len: c_int = 0;
        gl::GetProgramiv(p, gl::INFO_LOG_LENGTH, &mut len);
        let mut buf = vec![0u8; len.max(1) as usize];
        gl::GetProgramInfoLog(p, len, ptr::null_mut(), buf.as_mut_ptr() as *mut c_char);
        return Err(String::from_utf8_lossy(&buf).into_owned());
    }
    Ok(p)
}

/// Fullscreen triangle generated from `gl_VertexID` — no vertex buffer needed.
const VERTEX_SRC: &str = "#version 330 core\n\
    void main() {\n\
      vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);\n\
      gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);\n\
    }\n";

// The compiled artifact

struct CompiledShader {
    program: c_uint,
    fbo: c_uint,
    tex: c_uint,
    vao: c_uint,
    /// Inputs: Evident var name → (uniform location, kind).
    inputs: Vec<(String, c_int, ScalarKind)>,
    /// Outputs in pixel order: Evident var name → kind.
    outputs: Vec<(String, ScalarKind)>,
}

impl super::CompiledFunction for CompiledShader {
    fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        let _sess = gl_session().ok()?;
        let n = self.outputs.len() as c_int;
        unsafe {
            gl::UseProgram(self.program);
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::Viewport(0, 0, n, 1);

            for (name, loc, kind) in &self.inputs {
                // Missing input → 0 sentinel; mistyped → None (falls through to Z3).
                let v: i32 = match given.get(name) {
                    Some(Value::Int(i)) => *i as i32,
                    Some(Value::Bool(b)) => *b as i32,
                    Some(_) => return None,
                    None => 0,
                };
                let _ = kind;
                gl::Uniform1i(*loc, v);
            }

            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 3);

            let mut buf = vec![0i32; (n as usize) * 4];
            gl::ReadPixels(
                0,
                0,
                n,
                1,
                gl::RGBA_INTEGER,
                gl::INT,
                buf.as_mut_ptr() as *mut c_void,
            );
            if gl::GetError() != 0 {
                return None;
            }

            let mut out = HashMap::with_capacity(self.outputs.len());
            for (i, (name, kind)) in self.outputs.iter().enumerate() {
                let raw = buf[i * 4]; // value packed in the .r channel
                let val = match kind {
                    ScalarKind::Int => Value::Int(raw as i64),
                    ScalarKind::Bool => Value::Bool(raw != 0),
                };
                out.insert(name.clone(), val);
            }
            Some(out)
        }
    }
}

impl Drop for CompiledShader {
    fn drop(&mut self) {
        // Best-effort cleanup; leak on poisoned mutex (process tearing down).
        if let Ok(_sess) = gl_session() {
            unsafe {
                gl::DeleteProgram(self.program);
                gl::DeleteFramebuffers(1, &self.fbo);
                gl::DeleteTextures(1, &self.tex);
                gl::DeleteVertexArrays(1, &self.vao);
            }
        }
    }
}

// The strategy

/// GLSL functionizer. Opt-in via `EvidentRuntime::with_functionizer(Box::new(GlslFunctionizer::new()?))`.
pub struct GlslFunctionizer {
    _private: (),
}

impl GlslFunctionizer {
    /// Initialize the headless GL context; `Err` if unavailable (no GL sandbox, non-macOS).
    pub fn new() -> Result<Self, String> {
        let _sess = gl_session()?;
        Ok(GlslFunctionizer { _private: () })
    }
}

impl super::Functionizer for GlslFunctionizer {
    fn name(&self) -> &'static str {
        "glsl"
    }

    fn compile(
        &self,
        program: &Z3Program,
        _enums: &EnumRegistry,
        _datatypes: &DatatypeRegistry,
    ) -> Option<Rc<dyn super::CompiledFunction>> {
        let trace = std::env::var("EVIDENT_GLSL_TRACE").is_ok();

        // ── Refuse anything outside the supported shape ─────────────
        if program.steps.is_empty() {
            return None;
        }
        if !program.checks.is_empty() || !program.predicates.is_empty() {
            if trace {
                eprintln!("[glsl] bail: program has checks/predicates (conditional body)");
            }
            return None;
        }

        let mut outputs: Vec<(String, ScalarKind)> = Vec::with_capacity(program.steps.len());
        let mut output_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut step_exprs: Vec<(String, &Dynamic)> = Vec::new();
        for step in &program.steps {
            let Z3Step::Scalar { var, expr } = step else {
                if trace {
                    eprintln!("[glsl] bail: non-scalar step for {}", step.var());
                }
                return None;
            };
            let Some(kind) = scalar_kind(&format!("{}", expr.get_sort())) else {
                if trace {
                    eprintln!(
                        "[glsl] bail: output {var} has unsupported sort {}",
                        expr.get_sort()
                    );
                }
                return None;
            };
            outputs.push((var.clone(), kind));
            output_names.insert(var.clone());
            step_exprs.push((var.clone(), expr));
        }

        // Inputs = free 0-arity consts (minus outputs), must be Int- or Bool-sorted.
        let mut free: BTreeMap<String, String> = BTreeMap::new();
        for (_, expr) in &step_exprs {
            collect_free_consts(expr, &mut free);
        }
        let mut inputs: Vec<(String, ScalarKind)> = Vec::new();
        for (name, sort) in &free {
            if output_names.contains(name) {
                continue;
            }
            let Some(kind) = scalar_kind(sort) else {
                if trace {
                    eprintln!("[glsl] bail: input {name} has unsupported sort {sort}");
                }
                return None;
            };
            inputs.push((name.clone(), kind));
        }

        // inputs → `inN` uniforms; outputs → `vN` locals (later outputs may ref earlier ones).
        let mut env: HashMap<String, String> = HashMap::new();
        for (i, (name, _)) in inputs.iter().enumerate() {
            env.insert(name.clone(), format!("in{i}"));
        }
        let mut output_exprs: Vec<String> = Vec::with_capacity(step_exprs.len());
        for (i, (var, expr)) in step_exprs.iter().enumerate() {
            let Some(code) = emit_glsl(expr, &env) else {
                if trace {
                    eprintln!("[glsl] bail: cannot emit GLSL for {var} = {expr}");
                }
                return None;
            };
            output_exprs.push(code);
            env.insert((*var).clone(), format!("v{i}"));
        }

        let mut frag = String::from("#version 330 core\n");
        for i in 0..inputs.len() {
            frag.push_str(&format!("uniform int in{i};\n"));
        }
        frag.push_str("out ivec4 o;\nvoid main() {\n");
        frag.push_str("    int idx = int(gl_FragCoord.x);\n");
        for (i, code) in output_exprs.iter().enumerate() {
            frag.push_str(&format!("    int v{i} = {code};\n"));
        }
        frag.push_str("    int sel = 0;\n");
        for i in 0..output_exprs.len() {
            let kw = if i == 0 { "if" } else { "else if" };
            frag.push_str(&format!("    {kw} (idx == {i}) sel = v{i};\n"));
        }
        frag.push_str("    o = ivec4(sel, 0, 0, 0);\n}\n");

        if trace {
            eprintln!("[glsl] fragment shader:\n{frag}");
        }

        let _sess = gl_session().ok()?;
        let n = outputs.len() as c_int;
        unsafe {
            let vs = compile_shader(VERTEX_SRC, gl::VERTEX_SHADER).ok().or_else(|| {
                if trace {
                    eprintln!("[glsl] bail: vertex shader compile failed");
                }
                None
            })?;
            let fs = match compile_shader(&frag, gl::FRAGMENT_SHADER) {
                Ok(s) => s,
                Err(e) => {
                    if trace {
                        eprintln!("[glsl] bail: fragment shader compile failed:\n{e}");
                    }
                    gl::DeleteShader(vs);
                    return None;
                }
            };
            let program_id = match link_program(vs, fs) {
                Ok(p) => p,
                Err(e) => {
                    if trace {
                        eprintln!("[glsl] bail: link failed:\n{e}");
                    }
                    gl::DeleteShader(vs);
                    gl::DeleteShader(fs);
                    return None;
                }
            };
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);

            // 1×N RGBA32I texture: exact i32 readback, one output per pixel column.
            let mut tex = 0;
            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA32I as i32,
                n,
                1,
                0,
                gl::RGBA_INTEGER,
                gl::INT,
                ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);

            let mut fbo = 0;
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                tex,
                0,
            );
            if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                if trace {
                    eprintln!("[glsl] bail: framebuffer incomplete");
                }
                gl::DeleteProgram(program_id);
                gl::DeleteFramebuffers(1, &fbo);
                gl::DeleteTextures(1, &tex);
                return None;
            }

            let mut vao = 0;
            gl::GenVertexArrays(1, &mut vao);

            let mut input_locs: Vec<(String, c_int, ScalarKind)> = Vec::with_capacity(inputs.len());
            for (i, (name, kind)) in inputs.iter().enumerate() {
                let uname = CString::new(format!("in{i}")).unwrap();
                let loc = gl::GetUniformLocation(program_id, uname.as_ptr());
                input_locs.push((name.clone(), loc, *kind));
            }

            Some(Rc::new(CompiledShader {
                program: program_id,
                fbo,
                tex,
                vao,
                inputs: input_locs,
                outputs,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use z3::{Config, Context};

    /// `emit_glsl` produces the expected string for `3*x + 5`.
    #[test]
    fn emits_affine_glsl() {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let x = z3::ast::Int::new_const(&ctx, "input");
        let expr = z3::ast::Int::add(
            &ctx,
            &[&(&x * &z3::ast::Int::from_i64(&ctx, 3)), &z3::ast::Int::from_i64(&ctx, 5)],
        );
        let mut env = HashMap::new();
        env.insert("input".to_string(), "in0".to_string());
        let code = emit_glsl(&Dynamic::from_ast(&expr), &env).expect("emit");
        // Z3 may order the sum either way; just assert the pieces appear.
        assert!(code.contains("in0"), "got {code}");
        assert!(code.contains('*'), "got {code}");
        assert!(code.contains('5'), "got {code}");
    }

    /// A String-sorted output is refused at the emit boundary.
    #[test]
    fn refuses_unsupported_sort() {
        assert_eq!(scalar_kind("String"), None);
        assert_eq!(scalar_kind("Int"), Some(ScalarKind::Int));
        assert_eq!(scalar_kind("Bool"), Some(ScalarKind::Bool));
    }
}
