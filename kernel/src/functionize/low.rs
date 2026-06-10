//! One-time lowering of step ASTs to a native IR (`LExpr`), evaluated per
//! tick with zero Z3 FFI.
//!
//! WHY (measured 2026-06-10, compiler2 driver on fixture-001): the AST
//! interpreter (`eval.rs`) re-pays per node per tick: 3–6 Z3 FFI calls, a
//! symbol decode + String alloc per variable read, a full datatype-sort
//! rescan per accessor (`accessor_field_index`), and two `env::var` calls
//! per recognizer. At ~1,900 steps × ~38k ticks that was 16.4 s of func
//! time (~0.5 ms/tick). Lowering resolves all of it once at load:
//! variables become slot indices, literals are decoded, accessor field
//! indices and recognizer targets are precomputed. Per-tick evaluation is
//! a pure Rust tree walk over `&[Option<Sv>]`.
//!
//! Semantics: `eval` mirrors `eval.rs::eval_scalar` arm for arm — same
//! Int/Bool coercions, same lazy ITE / short-circuit ∧ ∨, same
//! out-of-range and division-by-zero refusals (`None` ⇒ the tick falls
//! through to Z3). Any AST node whose lowering inputs are unavailable
//! (unsupported op, non-i64 numeral, unknown accessor) lowers to
//! `Unsupported`, which evaluates to `None` — exactly what the legacy
//! interpreter would have returned at runtime for that node, so laziness
//! still skips it on the ticks that never reach it. Lowering is total:
//! it cannot fail, only produce nodes that refuse at eval time.

use std::collections::HashMap;
use z3_sys::*;

use crate::tick::Sv;
use super::{
    accessor_field_index, app_decl_name, ast_app_name, children, decl_kind, numeral_i64,
    recognizer_target,
};

// ── Name interner: variable name ⇄ env slot ─────────────────────

#[derive(Default)]
pub struct Names {
    pub list: Vec<String>,
    ids: HashMap<String, u32>,
}

impl Names {
    pub fn intern(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.ids.get(name) {
            return id;
        }
        let id = self.list.len() as u32;
        self.list.push(name.to_string());
        self.ids.insert(name.to_string(), id);
        id
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }
}

// ── Lowered expression IR ───────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Le,
    Lt,
    Ge,
    Gt,
}

pub enum LExpr {
    /// Literal, decoded once at lower time; eval borrows it.
    Const(Sv),
    Var(u32),
    Add(Vec<LExpr>),
    Mul(Vec<LExpr>),
    Sub(Vec<LExpr>),
    Neg(Box<LExpr>),
    Div(Box<LExpr>, Box<LExpr>),
    Mod(Box<LExpr>, Box<LExpr>),
    Cmp(CmpOp, Box<LExpr>, Box<LExpr>),
    Eq(Box<LExpr>, Box<LExpr>),
    Ne(Box<LExpr>, Box<LExpr>),
    Not(Box<LExpr>),
    And(Vec<LExpr>),
    Or(Vec<LExpr>),
    Implies(Box<LExpr>, Box<LExpr>),
    Ite(Box<LExpr>, Box<LExpr>, Box<LExpr>),
    /// Datatype constructor application (precomputed variant name).
    Ctor(String, Vec<LExpr>),
    /// Datatype recognizer (precomputed target variant name).
    IsVariant(String, Box<LExpr>),
    /// Datatype field read (precomputed 0-based field index).
    Field(usize, Box<LExpr>),
    Select(Box<LExpr>, Box<LExpr>),
    Store(Box<LExpr>, Box<LExpr>, Box<LExpr>),
    StrLen(Box<LExpr>),
    StrConcat(Vec<LExpr>),
    StrSubstr(Box<LExpr>, Box<LExpr>, Box<LExpr>),
    StrIndexOf(Box<LExpr>, Box<LExpr>, Box<LExpr>),
    StrContains(Box<LExpr>, Box<LExpr>),
    StrPrefix(Box<LExpr>, Box<LExpr>),
    StrSuffix(Box<LExpr>, Box<LExpr>),
    StrAt(Box<LExpr>, Box<LExpr>),
    StrReplace(Box<LExpr>, Box<LExpr>, Box<LExpr>),
    StrToInt(Box<LExpr>),
    IntToStr(Box<LExpr>),
    SeqUnit(Box<LExpr>),
    SeqEmpty,
    /// The legacy interpreter would refuse this node at runtime regardless
    /// of values (unsupported op / undecodable literal); evaluates to `None`.
    Unsupported,
}

// ── Lowering (total; runs once at load) ─────────────────────────

pub unsafe fn lower(ctx: Z3_context, a: Z3_ast, names: &mut Names) -> LExpr {
    let kind = Z3_get_ast_kind(ctx, a);
    if kind == AstKind::Numeral {
        return match numeral_i64(ctx, a) {
            Some(n) => LExpr::Const(Sv::Int(n)),
            None => LExpr::Unsupported,
        };
    }
    if kind != AstKind::App {
        return LExpr::Unsupported;
    }

    // String literal (0-arity, non-uninterpreted app of String sort).
    if Z3_get_app_num_args(ctx, Z3_to_app(ctx, a)) == 0 && Z3_is_string(ctx, a) {
        let p = Z3_get_string(ctx, a);
        if !p.is_null() {
            let raw = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
            return LExpr::Const(Sv::Str(crate::tick::unescape_z3_pub(&raw)));
        }
        return LExpr::Unsupported;
    }

    let Some(dk) = decl_kind(ctx, a) else {
        return LExpr::Unsupported;
    };
    let ch = children(ctx, a);
    let low1 = |i: usize, names: &mut Names| Box::new(lower(ctx, ch[i], names));
    macro_rules! lown {
        () => {
            ch.iter().map(|&c| lower(ctx, c, names)).collect::<Vec<_>>()
        };
    }
    match dk {
        DeclKind::TRUE => LExpr::Const(Sv::Bool(true)),
        DeclKind::FALSE => LExpr::Const(Sv::Bool(false)),
        DeclKind::UNINTERPRETED => {
            if !ch.is_empty() {
                return LExpr::Unsupported;
            }
            match ast_app_name(ctx, a) {
                Some(name) => LExpr::Var(names.intern(&name)),
                None => LExpr::Unsupported,
            }
        }
        DeclKind::IDIV if ch.len() == 2 => LExpr::Div(low1(0, names), low1(1, names)),
        DeclKind::MOD if ch.len() == 2 => LExpr::Mod(low1(0, names), low1(1, names)),
        DeclKind::ADD if !ch.is_empty() => LExpr::Add(lown!()),
        DeclKind::MUL if !ch.is_empty() => LExpr::Mul(lown!()),
        DeclKind::SUB if !ch.is_empty() => LExpr::Sub(lown!()),
        DeclKind::UMINUS if ch.len() == 1 => LExpr::Neg(low1(0, names)),
        DeclKind::LE if ch.len() == 2 => LExpr::Cmp(CmpOp::Le, low1(0, names), low1(1, names)),
        DeclKind::LT if ch.len() == 2 => LExpr::Cmp(CmpOp::Lt, low1(0, names), low1(1, names)),
        DeclKind::GE if ch.len() == 2 => LExpr::Cmp(CmpOp::Ge, low1(0, names), low1(1, names)),
        DeclKind::GT if ch.len() == 2 => LExpr::Cmp(CmpOp::Gt, low1(0, names), low1(1, names)),
        DeclKind::EQ | DeclKind::IFF if ch.len() == 2 => {
            LExpr::Eq(low1(0, names), low1(1, names))
        }
        DeclKind::DISTINCT if ch.len() == 2 => LExpr::Ne(low1(0, names), low1(1, names)),
        DeclKind::NOT if ch.len() == 1 => LExpr::Not(low1(0, names)),
        DeclKind::AND => LExpr::And(lown!()),
        DeclKind::OR => LExpr::Or(lown!()),
        DeclKind::IMPLIES if ch.len() == 2 => LExpr::Implies(low1(0, names), low1(1, names)),
        DeclKind::ITE if ch.len() == 3 => {
            LExpr::Ite(low1(0, names), low1(1, names), low1(2, names))
        }
        DeclKind::DT_CONSTRUCTOR => match app_decl_name(ctx, a) {
            Some(name) => LExpr::Ctor(name, lown!()),
            None => LExpr::Unsupported,
        },
        DeclKind::DT_IS | DeclKind::DT_RECOGNISER if ch.len() == 1 => {
            match recognizer_target(ctx, a) {
                Some(want) => LExpr::IsVariant(want, low1(0, names)),
                None => LExpr::Unsupported,
            }
        }
        DeclKind::DT_ACCESSOR if ch.len() == 1 => match accessor_field_index(ctx, a) {
            Some(fi) => LExpr::Field(fi, low1(0, names)),
            None => LExpr::Unsupported,
        },
        DeclKind::SELECT if ch.len() == 2 => LExpr::Select(low1(0, names), low1(1, names)),
        DeclKind::STORE if ch.len() == 3 => {
            LExpr::Store(low1(0, names), low1(1, names), low1(2, names))
        }
        DeclKind::SEQ_LENGTH if ch.len() == 1 => LExpr::StrLen(low1(0, names)),
        DeclKind::SEQ_CONCAT => LExpr::StrConcat(lown!()),
        DeclKind::SEQ_EXTRACT if ch.len() == 3 => {
            LExpr::StrSubstr(low1(0, names), low1(1, names), low1(2, names))
        }
        DeclKind::SEQ_INDEX if ch.len() == 3 => {
            LExpr::StrIndexOf(low1(0, names), low1(1, names), low1(2, names))
        }
        DeclKind::SEQ_CONTAINS if ch.len() == 2 => {
            LExpr::StrContains(low1(0, names), low1(1, names))
        }
        DeclKind::SEQ_PREFIX if ch.len() == 2 => LExpr::StrPrefix(low1(0, names), low1(1, names)),
        DeclKind::SEQ_SUFFIX if ch.len() == 2 => LExpr::StrSuffix(low1(0, names), low1(1, names)),
        DeclKind::SEQ_AT if ch.len() == 2 => LExpr::StrAt(low1(0, names), low1(1, names)),
        DeclKind::SEQ_REPLACE if ch.len() == 3 => {
            LExpr::StrReplace(low1(0, names), low1(1, names), low1(2, names))
        }
        DeclKind::SEQ_UNIT if ch.len() == 1 => LExpr::SeqUnit(low1(0, names)),
        DeclKind::SEQ_EMPTY => LExpr::SeqEmpty,
        DeclKind::STR_TO_INT if ch.len() == 1 => LExpr::StrToInt(low1(0, names)),
        DeclKind::INT_TO_STR if ch.len() == 1 => LExpr::IntToStr(low1(0, names)),
        _ => LExpr::Unsupported,
    }
}

// ── Evaluation (per tick; no FFI) ───────────────────────────────
//
// Returns `Cow<Sv>` so the hot reads are zero-copy: a `Var` borrows the
// slot, a `Field`/`Select` on a borrowed datatype/seq borrows the element,
// `Const` borrows the literal. Owned values appear only where a new value
// is computed. The single clone happens at step binding (`into_owned`),
// and only when the result is still a borrow. (Measured 2026-06-10 on the
// driver: per-Var Sv clones — carried strings + record values — were the
// largest residual cost after the FFI removal.)

use std::borrow::Cow;

/// Per-tick, per-slot string metadata memo. Valid only while the slot's
/// value is unchanged — `exec_step_low` resets an entry on every slot
/// write. Slot-keyed (never pointer-keyed) so a freed-and-reallocated
/// buffer can't alias a stale entry.
#[derive(Clone)]
pub struct StrMeta {
    /// 0 = unknown, 1 = ascii, 2 = non-ascii.
    ascii: u8,
    /// Monotone char→byte cursor for non-ascii strings (the FSM text-scan
    /// pattern reads `pos, pos+1, …`; this makes each read O(delta) instead
    /// of O(pos) — measured 2026-06-10: the driver's 17 cursor reads per
    /// tick on a ~5KB unicode `input` were ~25% of tick time).
    char_idx: u32,
    byte_idx: u32,
    /// Total char count, u32::MAX = unknown.
    chars: u32,
}

impl Default for StrMeta {
    fn default() -> Self {
        StrMeta { ascii: 0, char_idx: 0, byte_idx: 0, chars: u32::MAX }
    }
}

pub fn reset_meta(meta: &mut [StrMeta], slot: usize) {
    if let Some(m) = meta.get_mut(slot) {
        *m = StrMeta::default();
    }
}

/// `Some(slot)` when `e` is a Var — the string ops key their memo on it.
fn var_slot_of(e: &LExpr) -> Option<usize> {
    match e {
        LExpr::Var(id) => Some(*id as usize),
        _ => None,
    }
}

fn ascii_memo(s: &str, meta: &mut [StrMeta], slot: Option<usize>) -> bool {
    if let Some(m) = slot.and_then(|k| meta.get_mut(k)) {
        if m.ascii == 0 {
            m.ascii = if s.is_ascii() { 1 } else { 2 };
        }
        return m.ascii == 1;
    }
    s.is_ascii()
}

#[inline]
fn utf8_adv(b: u8) -> usize {
    if b < 0x80 { 1 } else if b < 0xE0 { 2 } else if b < 0xF0 { 3 } else { 4 }
}

/// Byte index of codepoint `off` in non-ascii `s`, through the slot cursor
/// when available. `None` ⟺ `off > #chars` (off == #chars maps to s.len()).
fn char_to_byte(s: &str, off: usize, meta: &mut [StrMeta], slot: Option<usize>) -> Option<usize> {
    let b = s.as_bytes();
    let (mut ci, mut bi) = match slot.and_then(|k| meta.get(k)) {
        Some(m) if (m.byte_idx as usize) <= b.len() => (m.char_idx as usize, m.byte_idx as usize),
        _ => (0, 0),
    };
    // Bidirectional O(delta) seek: steps execute in topo order, not offset
    // order, so backward jumps are routine (a reset-to-0 here re-walked the
    // whole prefix every backward seek and erased the cursor's win —
    // measured 2026-06-10).
    while ci > off && bi > 0 {
        bi -= 1;
        while bi > 0 && (b[bi] & 0xC0) == 0x80 {
            bi -= 1;
        }
        ci -= 1;
    }
    while ci < off && bi < b.len() {
        bi += utf8_adv(b[bi]);
        ci += 1;
    }
    if let Some(m) = slot.and_then(|k| meta.get_mut(k)) {
        m.char_idx = ci as u32;
        m.byte_idx = bi as u32;
    }
    if ci == off { Some(bi) } else { None }
}

pub fn eval<'a>(e: &'a LExpr, slots: &'a [Option<Sv>], meta: &mut [StrMeta]) -> Option<Cow<'a, Sv>> {
    match e {
        LExpr::Const(v) => Some(Cow::Borrowed(v)),
        LExpr::Var(id) => slots.get(*id as usize)?.as_ref().map(Cow::Borrowed),
        LExpr::Add(es) | LExpr::Mul(es) | LExpr::Sub(es) => {
            let mut it = es.iter();
            let mut acc = as_int(eval(it.next()?, slots, meta)?.as_ref())?;
            for c in it {
                let v = as_int(eval(c, slots, meta)?.as_ref())?;
                acc = match e {
                    LExpr::Add(_) => acc.checked_add(v)?,
                    LExpr::Mul(_) => acc.checked_mul(v)?,
                    LExpr::Sub(_) => acc.checked_sub(v)?,
                    _ => unreachable!(),
                };
            }
            Some(Cow::Owned(Sv::Int(acc)))
        }
        LExpr::Neg(c) => {
            let v = as_int(eval(c, slots, meta)?.as_ref())?;
            Some(Cow::Owned(Sv::Int(v.checked_neg()?)))
        }
        // SMT-LIB Int division/modulo are EUCLIDEAN; div-by-zero refuses
        // (matches eval.rs).
        LExpr::Div(a, b) => {
            let a = as_int(eval(a, slots, meta)?.as_ref())?;
            let b = as_int(eval(b, slots, meta)?.as_ref())?;
            if b == 0 {
                return None;
            }
            Some(Cow::Owned(Sv::Int(a.div_euclid(b))))
        }
        LExpr::Mod(a, b) => {
            let a = as_int(eval(a, slots, meta)?.as_ref())?;
            let b = as_int(eval(b, slots, meta)?.as_ref())?;
            if b == 0 {
                return None;
            }
            Some(Cow::Owned(Sv::Int(a.rem_euclid(b))))
        }
        LExpr::Cmp(op, a, b) => {
            let l = as_int(eval(a, slots, meta)?.as_ref())?;
            let r = as_int(eval(b, slots, meta)?.as_ref())?;
            Some(Cow::Owned(Sv::Bool(match op {
                CmpOp::Le => l <= r,
                CmpOp::Lt => l < r,
                CmpOp::Ge => l >= r,
                CmpOp::Gt => l > r,
            })))
        }
        LExpr::Eq(a, b) => {
            let l = eval(a, slots, meta)?;
            let r = eval(b, slots, meta)?;
            Some(Cow::Owned(Sv::Bool(crate::tick::compare_sv_pub(l.as_ref(), r.as_ref()))))
        }
        LExpr::Ne(a, b) => {
            let l = eval(a, slots, meta)?;
            let r = eval(b, slots, meta)?;
            Some(Cow::Owned(Sv::Bool(!crate::tick::compare_sv_pub(l.as_ref(), r.as_ref()))))
        }
        LExpr::Not(c) => {
            let v = as_bool(eval(c, slots, meta)?.as_ref())?;
            Some(Cow::Owned(Sv::Bool(!v)))
        }
        LExpr::And(es) => {
            for c in es {
                if !as_bool(eval(c, slots, meta)?.as_ref())? {
                    return Some(Cow::Owned(Sv::Bool(false)));
                }
            }
            Some(Cow::Owned(Sv::Bool(true)))
        }
        LExpr::Or(es) => {
            for c in es {
                if as_bool(eval(c, slots, meta)?.as_ref())? {
                    return Some(Cow::Owned(Sv::Bool(true)));
                }
            }
            Some(Cow::Owned(Sv::Bool(false)))
        }
        LExpr::Implies(p, q) => {
            if !as_bool(eval(p, slots, meta)?.as_ref())? {
                return Some(Cow::Owned(Sv::Bool(true)));
            }
            Some(Cow::Owned(Sv::Bool(as_bool(eval(q, slots, meta)?.as_ref())?)))
        }
        // Lazy: only the taken branch is evaluated.
        LExpr::Ite(c, t, f) => {
            if as_bool(eval(c, slots, meta)?.as_ref())? {
                eval(t, slots, meta)
            } else {
                eval(f, slots, meta)
            }
        }
        LExpr::Ctor(name, es) => {
            let mut fields = Vec::with_capacity(es.len());
            for c in es {
                fields.push(eval(c, slots, meta)?.into_owned());
            }
            Some(Cow::Owned(Sv::Datatype(name.clone(), fields)))
        }
        LExpr::IsVariant(want, c) => {
            let v = eval(c, slots, meta)?;
            let Sv::Datatype(actual, _) = v.as_ref() else {
                return None;
            };
            Some(Cow::Owned(Sv::Bool(actual == want)))
        }
        LExpr::Field(fi, c) => match eval(c, slots, meta)? {
            Cow::Borrowed(Sv::Datatype(_, fields)) => fields.get(*fi).map(Cow::Borrowed),
            Cow::Owned(Sv::Datatype(_, fields)) => fields.into_iter().nth(*fi).map(Cow::Owned),
            _ => None,
        },
        // Out-of-range Select returns None — Z3 fallback (see eval.rs note
        // on commit c420fe6).
        LExpr::Select(arr, idx) => {
            let arr = eval(arr, slots, meta)?;
            let idx = as_int(eval(idx, slots, meta)?.as_ref())?;
            if idx < 0 {
                return None;
            }
            match arr {
                Cow::Borrowed(Sv::Seq(elems)) => elems.get(idx as usize).map(Cow::Borrowed),
                Cow::Owned(Sv::Seq(elems)) => elems.into_iter().nth(idx as usize).map(Cow::Owned),
                _ => None,
            }
        }
        LExpr::Store(arr, idx, val) => {
            let mut elems = match eval(arr, slots, meta)?.into_owned() {
                Sv::Seq(es) => es,
                _ => return None,
            };
            let i = as_int(eval(idx, slots, meta)?.as_ref())?;
            let v = eval(val, slots, meta)?.into_owned();
            if i < 0 {
                return None;
            }
            let idx = i as usize;
            while elems.len() <= idx {
                elems.push(Sv::Int(0));
            }
            elems[idx] = v;
            Some(Cow::Owned(Sv::Seq(elems)))
        }
        // String ops below take an ASCII fast path (codepoint == byte; the
        // is_ascii scan is SIMD-cheap). Measured 2026-06-10 on the driver:
        // the collect-all-chars StrSubstr made single-char cursor reads
        // O(|s|) + an alloc — the 16 WsRunLen scan steps alone were ~30%
        // of a tick.
        LExpr::StrLen(c) => {
            let slot = var_slot_of(c);
            let v = eval(c, slots, meta)?;
            let s = as_str(v.as_ref())?;
            if ascii_memo(s, meta, slot) {
                return Some(Cow::Owned(Sv::Int(s.len() as i64)));
            }
            if let Some(m) = slot.and_then(|k| meta.get_mut(k)) {
                if m.chars == u32::MAX {
                    m.chars = s.chars().count() as u32;
                }
                return Some(Cow::Owned(Sv::Int(m.chars as i64)));
            }
            Some(Cow::Owned(Sv::Int(s.chars().count() as i64)))
        }
        LExpr::StrConcat(es) => {
            let mut out = String::new();
            for c in es {
                let v = eval(c, slots, meta)?;
                let Sv::Str(s) = v.as_ref() else {
                    return None;
                };
                out.push_str(s);
            }
            Some(Cow::Owned(Sv::Str(out)))
        }
        LExpr::StrSubstr(sx, off, len) => {
            let slot = var_slot_of(sx);
            let sv = eval(sx, slots, meta)?;
            let s = as_str(sv.as_ref())?;
            let off = as_int(eval(off, slots, meta)?.as_ref())?;
            let len = as_int(eval(len, slots, meta)?.as_ref())?;
            if off < 0 || len < 0 {
                return Some(Cow::Owned(Sv::Str(String::new())));
            }
            if ascii_memo(s, meta, slot) {
                let b = s.as_bytes();
                let n = b.len() as i64;
                if off >= n {
                    return Some(Cow::Owned(Sv::Str(String::new())));
                }
                let end = (off + len).min(n) as usize;
                // Safe: ASCII slicing at any byte index is char-aligned.
                let out = &s[off as usize..end];
                return Some(Cow::Owned(Sv::Str(out.to_string())));
            }
            // Non-ASCII: resolve byte range through the slot cursor.
            let Some(start) = char_to_byte(s, off as usize, meta, slot) else {
                return Some(Cow::Owned(Sv::Str(String::new())));
            };
            let b = s.as_bytes();
            let mut end = start;
            let mut cnt = 0i64;
            while cnt < len && end < b.len() {
                end += utf8_adv(b[end]);
                cnt += 1;
            }
            Some(Cow::Owned(Sv::Str(s[start..end].to_string())))
        }
        LExpr::StrIndexOf(sx, sub, off) => {
            let slot = var_slot_of(sx);
            let sv = eval(sx, slots, meta)?;
            let s = as_str(sv.as_ref())?;
            let subv = eval(sub, slots, meta)?;
            let sub = as_str(subv.as_ref())?;
            let off = as_int(eval(off, slots, meta)?.as_ref())?;
            if off < 0 {
                return Some(Cow::Owned(Sv::Int(-1)));
            }
            if ascii_memo(s, meta, slot) {
                // off == #chars is a legal empty-tail search start (matches
                // the codepoint walk); beyond it returns -1.
                if off > s.len() as i64 {
                    return Some(Cow::Owned(Sv::Int(-1)));
                }
                return Some(Cow::Owned(Sv::Int(match s[off as usize..].find(sub) {
                    Some(b) => off + b as i64,
                    None => -1,
                })));
            }
            // Codepoint offset → byte offset, through the slot cursor.
            let Some(byte_off) = char_to_byte(s, off as usize, meta, slot) else {
                return Some(Cow::Owned(Sv::Int(-1)));
            };
            match s[byte_off..].find(sub) {
                Some(b) => {
                    // Result is a CODEPOINT index: off + chars in the gap.
                    let gap = s[byte_off..byte_off + b].chars().count() as i64;
                    Some(Cow::Owned(Sv::Int(off + gap)))
                }
                None => Some(Cow::Owned(Sv::Int(-1))),
            }
        }
        LExpr::StrContains(s, sub) => {
            let sv = eval(s, slots, meta)?;
            let s = as_str(sv.as_ref())?;
            let subv = eval(sub, slots, meta)?;
            let sub = as_str(subv.as_ref())?;
            Some(Cow::Owned(Sv::Bool(s.contains(sub))))
        }
        LExpr::StrPrefix(a, b) => {
            let av = eval(a, slots, meta)?;
            let a = as_str(av.as_ref())?;
            let bv = eval(b, slots, meta)?;
            let b = as_str(bv.as_ref())?;
            Some(Cow::Owned(Sv::Bool(b.starts_with(a))))
        }
        LExpr::StrSuffix(a, b) => {
            let av = eval(a, slots, meta)?;
            let a = as_str(av.as_ref())?;
            let bv = eval(b, slots, meta)?;
            let b = as_str(bv.as_ref())?;
            Some(Cow::Owned(Sv::Bool(b.ends_with(a))))
        }
        LExpr::StrAt(sx, i) => {
            let slot = var_slot_of(sx);
            let sv = eval(sx, slots, meta)?;
            let s = as_str(sv.as_ref())?;
            let i = as_int(eval(i, slots, meta)?.as_ref())?;
            if i < 0 {
                return Some(Cow::Owned(Sv::Str(String::new())));
            }
            if ascii_memo(s, meta, slot) {
                let out = match s.as_bytes().get(i as usize) {
                    Some(&b) => (b as char).to_string(),
                    None => String::new(),
                };
                return Some(Cow::Owned(Sv::Str(out)));
            }
            let b = s.as_bytes();
            match char_to_byte(s, i as usize, meta, slot) {
                Some(start) if start < b.len() => {
                    let end = start + utf8_adv(b[start]);
                    Some(Cow::Owned(Sv::Str(s[start..end].to_string())))
                }
                _ => Some(Cow::Owned(Sv::Str(String::new()))),
            }
        }
        // Replace FIRST occurrence (SMT-LIB semantics).
        LExpr::StrReplace(s, from, to) => {
            let sv = eval(s, slots, meta)?;
            let s = as_str(sv.as_ref())?;
            let fromv = eval(from, slots, meta)?;
            let from = as_str(fromv.as_ref())?;
            let tov = eval(to, slots, meta)?;
            let to = as_str(tov.as_ref())?;
            Some(Cow::Owned(Sv::Str(s.replacen(from, to, 1))))
        }
        LExpr::StrToInt(c) => {
            let v = eval(c, slots, meta)?;
            let s = as_str(v.as_ref())?;
            Some(Cow::Owned(Sv::Int(
                s.parse::<i64>().ok().filter(|&n| n >= 0).unwrap_or(-1),
            )))
        }
        LExpr::IntToStr(c) => {
            let i = as_int(eval(c, slots, meta)?.as_ref())?;
            Some(Cow::Owned(Sv::Str(if i < 0 { String::new() } else { i.to_string() })))
        }
        LExpr::SeqUnit(c) => Some(Cow::Owned(Sv::Seq(vec![eval(c, slots, meta)?.into_owned()]))),
        LExpr::SeqEmpty => Some(Cow::Owned(Sv::Seq(Vec::new()))),
        LExpr::Unsupported => None,
    }
}

fn as_int(v: &Sv) -> Option<i64> {
    match v {
        Sv::Int(n) => Some(*n),
        Sv::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

fn as_bool(v: &Sv) -> Option<bool> {
    match v {
        Sv::Bool(b) => Some(*b),
        Sv::Int(n) => Some(*n != 0),
        _ => None,
    }
}

fn as_str(v: &Sv) -> Option<&str> {
    match v {
        Sv::Str(s) => Some(s),
        _ => None,
    }
}
