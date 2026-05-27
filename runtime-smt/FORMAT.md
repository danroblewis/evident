# runtime-smt fixture format

A fixture is a single UTF-8 text file (conventionally `.smt2`) that contains:

1. An embedded **metadata block** (JSON, comment-prefixed) that names the FSMs
   and their role assignments.
2. One **`; @transition <name>`** block per FSM containing the SMT-LIB text for
   that FSM's transition relation.

The format is designed so the file is valid SMT-LIB from Z3's point of view
(the metadata lives in `;` comments) while also being machine-parseable by the
engine loader.

---

## Block structure

```
<optional preamble lines — ignored>
; @meta
; <JSON lines, each prefixed with "; ">
; @end
; @transition <fsmname>
<SMT-LIB lines for that FSM's transition>
; @transition <anotherfsmname>
<SMT-LIB lines for the second FSM>
...
```

### Marker lines

| Marker | Meaning |
|--------|---------|
| `; @meta` | Start of JSON metadata block (exclusive). |
| `; @end` | End of JSON metadata block (exclusive). |
| `; @transition <name>` | Start of the named FSM's SMT-LIB transition block. |

Lines before `; @meta` are ignored.  Lines between `; @end` and the first
`; @transition` marker are also ignored (use them for file-level comments or
shared datatype declarations if you wish, though those are usually inside a
transition block).

### JSON body

Each line between `; @meta` and `; @end` has its leading `;` stripped and one
optional following space stripped; the results are concatenated (with newlines)
and parsed as a single JSON object (`Problem`).

The `transition` field of each `FsmSpec` is **not** in the JSON; it is filled
from the `; @transition` block.

### Transition blocks

Each `; @transition <name>` line starts a new block.  All subsequent
non-marker lines (until the next `; @transition` or end-of-file) are that
block's SMT-LIB text.  Leading and trailing blank lines are trimmed; internal
blank lines and indentation are preserved exactly.

Every FSM named in the JSON must have exactly one matching `; @transition`
block.  Every `; @transition` block must name a declared FSM.  Both violations
are errors.

---

## JSON schema

### `Problem`

```json
{
  "fsms":  [ <FsmSpec>, ... ],   // required, non-empty
  "world": [ <WorldVar>, ... ]   // optional, default []
}
```

### `FsmSpec`

```json
{
  "name":         "countdown",         // required; matches ; @transition name
  "state":        [ <StateVar>, ... ], // default []
  "given":        [ <GivenVar>, ... ], // default []
  "effects":      <EffectSpec>,        // default null
  "halt":         <HaltSpec>,          // default null
  "world_writes": [ "varname", ... ],  // default []
  "world_reads":  [ "varname", ... ]   // default []
}
```

`transition` is intentionally absent from the JSON — it is sourced from the
`; @transition` block.

### `StateVar`

```json
{
  "prev": "_count",  // SMT-LIB const name for the previous-tick value
  "next": "count",   // SMT-LIB const name for the current-tick value
  "sort": "Int",     // sort spelling (see below)
  "init": 3          // optional initial value pinned to `prev` on tick 0
}
```

`prev` and `next` must differ.  The engine pins `prev = <prev_value>` as an
assertion each tick; it reads `next` from the model and threads it as the next
tick's `prev`.

### `GivenVar`

```json
{ "name": "key_pressed", "sort": "Bool" }
```

A given variable is pinned from outside the solver each tick (e.g. from
world state or external input).  The engine asserts `name = <value>` before
solving.

### `EffectSpec`

```json
{ "var": "effects" }
```

Names the SMT-LIB const that holds the FSM's emitted effects.  The const is
expected to have sort `(Seq <EffectDatatype>)`.  After solving, the engine
decodes the sequence and dispatches each effect.

### `HaltSpec`

```json
{ "var": "halt" }   // explicit Bool const that signals halt when true
{}                   // present but no named var — halt only via Exit effect
```

Optional.  When present, the engine reads the named Bool const from the model
each tick.  The FSM also halts if any emitted effect is `Exit(code)`.

### `WorldVar`

```json
{
  "name": "tick_count",
  "sort": "Int",
  "init": 0
}
```

Shared world variables.  Multiple FSMs can read a world var; at most one
writes it per tick.  `world_reads`/`world_writes` in each `FsmSpec` name
which world vars that FSM participates in.

---

## Sort spellings

| JSON string | SMT-LIB sort | Notes |
|-------------|--------------|-------|
| `"Int"` or `"Nat"` or `"Pos"` | `Int` | All map to Z3 integer sort |
| `"Bool"` | `Bool` | |
| `"Real"` | `Real` | |
| `"String"` or `"Str"` | `String` | SMT-LIB string theory |
| `"Seq(T)"` | `(Seq T)` | T is any sort spelling, recursive |
| `"Effect"` (any capitalized word) | `Effect` | User-declared datatype |

---

## Literal spellings (`init` values)

Literals in `init` (StateVar, WorldVar) and `given` pins use serde untagged
JSON:

| JSON | Rust `Lit` variant | Notes |
|------|--------------------|-------|
| `true` / `false` | `Bool` | |
| `3` | `Int(3)` | |
| `3.5` | `Real(3.5)` | |
| `"hello"` | `Str("hello")` | Under a `Datatype` sort, treated as a nullary constructor |
| `{"ctor":"Run","args":[5]}` | `Ctor{...}` | Named constructor with args |

---

## Worked example — countdown FSM

```smt2
; This fixture runs a simple 3-step countdown.
; @meta
; {
;   "fsms": [
;     { "name": "countdown",
;       "state": [{"prev":"_count","next":"count","sort":"Int","init":3}],
;       "effects": {"var":"effects"},
;       "halt": {"var":"halt"} }
;   ]
; }
; @end
; @transition countdown
(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)) (Tick))))
(declare-const _count Int)
(declare-const count Int)
(declare-const effects (Seq Effect))
(declare-const halt Bool)
(assert (= count (- _count 1)))
(assert (= halt (<= count 0)))
(assert (= effects (seq.unit (Tick))))
```

What the engine does with this:

- **Tick 0**: pins `_count = 3` (from `init`), solves, reads `count = 2`,
  `halt = false`, dispatches `[Tick]`.
- **Tick 1**: pins `_count = 2`, solves, reads `count = 1`, `halt = false`.
- **Tick 2**: pins `_count = 1`, solves, reads `count = 0`, `halt = true` →
  engine halts.

---

## Multi-FSM example sketch

```smt2
; @meta
; {
;   "fsms": [
;     { "name": "counter",
;       "state": [{"prev":"_n","next":"n","sort":"Int","init":0}] },
;     { "name": "logger",
;       "state": [],
;       "given": [{"name":"current_n","sort":"Int"}],
;       "effects": {"var":"log_effects"} }
;   ]
; }
; @end
; @transition counter
(declare-const _n Int)
(declare-const n Int)
(assert (= n (+ _n 1)))
; @transition logger
(declare-datatypes ((Effect 0)) (((Log (msg String)))))
(declare-const current_n Int)
(declare-const log_effects (Seq Effect))
(assert (= log_effects (seq.unit (Log "tick"))))
```

In a multi-FSM run the engine threads each FSM's state independently and
coordinates world reads/writes via the `world_reads`/`world_writes` fields.
