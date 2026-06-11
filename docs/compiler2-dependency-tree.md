# compiler2 — dependency & composition tree

The self-hosted Evident compiler lives in `compiler2/*.ev`. This document
maps two distinct graphs:

1. **Import / flatten graph** — what `import "…"` pulls in (textual; how the
   single flattened translation unit is assembled).
2. **Composition pipeline** — how `driver_main` (in `driver.ev`) *composes*
   the ~28 components by names-match (`..`-lift or bare-mention). This is the
   real data-flow architecture; imports are just a flat star around it.

> Generated 2026-06-11 from the source tree. `driver.ev` is the hub: it
> imports every component, the translate2 helpers, the legacy `compiler/`
> foundation, and `stdlib/`. Components do **not** import each other — they
> are flattened together and join by shared variable names.

---

## 1. Layered import graph

```mermaid
graph LR
    subgraph EXT["External foundation"]
        K["stdlib/kernel.ev<br/>Effect enum · Build* sugar"]
        Z["stdlib/z3_*.ev<br/>z3_ast · z3_core · z3_seq · z3_datatypes<br/>(Z3 FFI sugar)"]
        LX["compiler/lexer.ev<br/>Token · TokenList"]
        PS["compiler/parser.ev<br/>Expr · Op · EnumDecl AST"]
        AR["compiler/translate_arith.ev<br/>arithmetic translation"]
    end

    subgraph SHARED["compiler2 shared vocabulary"]
        IR["driver_ir.ev<br/>C2Item/C2Items IR · FtiBuffer<br/>Z3SolverCtx · record/enum registry types"]
        LF["lex_fti.ev<br/>FTI lexer rows"]
    end

    subgraph T2["translate2 — Z3-AST building helpers"]
        TB["translate2_bool.ev"]
        TC["translate2_ctor.ev"]
        TS["translate2_seq.ev"]
        TR["translate2_record.ev"]
        TM["translate2_match.ev"]
    end

    HUB["driver.ev<br/><b>driver_main</b> — orchestrator"]

    PS --> IR
    AR --> PS
    PS --> TB & TC & TS & TR
    K --> TB & TS & TR & TM & LF
    Z --> TC

    EXT --> HUB
    SHARED --> HUB
    T2 --> HUB
    CMP["28 driver_* components<br/>(see pipeline below)"] --> HUB
```

`driver.ev` flattens all of the above into one unit; `driver_ir.ev` and the
`translate2_*` helpers depend only on the `compiler/parser.ev` AST + `stdlib`.
The legacy `compiler/` reach (`lexer`/`parser`/`translate_arith`) is the
`legacy_compiler_imports` goalpost (currently **3**, target 0 — deletable
once those types move into compiler2).

---

## 2. Composition pipeline (driver_main)

`driver_main` threads source → tokens → parse → work-items → Z3 effects →
emit. Each box is a component composed in this order. **Bare-mention**
components (encapsulated header interface) are marked `◆`; the rest are
`..`-lift (shared flat namespace).

```mermaid
flowchart LR
    SRC([source .ev bytes])

    subgraph BOOT["Bootstrap"]
        INP["DriverInput<br/>read source path"]
        ZIN["DriverZInit<br/>Z3 lifecycle latch bank"]
    end

    subgraph DECL["Declaration processing"]
        ENU["DriverEnum<br/>enum-decl machine"]
        REC["DriverRecord<br/>record-type registry"]
        BEF["DriverBuildEff<br/>enum/Z3 effect bank"]
    end

    subgraph LEXW["Lex + window"]
        LEX["DriverLex<br/>fossil lexer FSM"]
        WIN["DriverWindow<br/>token window (tok0..7)"]
    end

    subgraph PARSE["Parse dispatch + recognizers"]
        CLS["DriverClassify<br/>line classifier"]
        CIX["DriverClaimIdx<br/>claim index"]
        PRA["DriverPratt<br/>Pratt expr parser"]
        EXD["DriverExprDecomp<br/>expr node decomposition"]
        GRP["DriverGroup ◆<br/>multi-name group walk"]
        POS["DriverPosBind<br/>positional binding"]
        GRD["DriverGuard<br/>conditional inline"]
        QNT["DriverQuant<br/>bounded quantifier"]
        MPN["DriverMatchPin<br/>match-pin walk"]
        CMP["DriverCompose<br/>composition inlining"]
    end

    subgraph SYM["Symbols + registries"]
        SYT["DriverSymtab<br/>FTI symbol table · work-item decode"]
        SYL["DriverSymLookup<br/>name resolution"]
        SET["DriverSetVar<br/>Set(T) registry"]
        LIT["DriverLitMem<br/>literal-collection membership"]
    end

    subgraph LOWER["Lowering + values"]
        RCV["DriverRecVal<br/>record-literal expansion"]
        CAL["DriverCallLower<br/>builtin/ctor call lowering"]
        BRO["DriverBroadcast ◆<br/>record-pin field broadcast"]
    end

    EMI["DriverEmit<br/>serialize → manifest+prelude+body → Exit(0)"]
    OUT([output .smt2])

    SRC --> INP --> ZIN --> ENU --> REC --> BEF --> LEX --> WIN
    WIN --> PARSE
    PARSE --> SYM
    SYM --> LOWER
    LOWER --> EMI --> OUT

    PRA -. drives .-> EXD
    GRP -. broadcasts type over names .-> SYT
    BRO -. re-walks per record field .-> SYT
    CAL -. uses .-> TX["translate2_* helpers"]
    MPN -. uses .-> TX
```

### Phase / `parse_mode` key

The pipeline is a tick-driven FSM; `parse_mode` selects the active recognizer:

| `parse_mode` | Stage | Owner |
|---|---|---|
| phase 0 | lex | DriverLex |
| 0 / 1 / 2 | parse dispatch · skip · claim | driver_main |
| 3 | (emit phase) | DriverEmit |
| 6 | match-pin walk | DriverMatchPin |
| 7 / 8 | literal-collection membership | DriverLitMem |
| 9 | multi-name group walk | DriverGroup ◆ |
| 10 | composition inlining | DriverCompose |
| 12 | positional binding | DriverPosBind |
| 13 | record-decl | DriverRecord |

---

## 3. Components by role

```mermaid
mindmap
  root((compiler2))
    Orchestrator
      driver.ev / driver_main
    Shared vocab
      driver_ir.ev
      lex_fti.ev
    Translate2 helpers
      translate2_bool
      translate2_ctor
      translate2_seq
      translate2_record
      translate2_match
    Bootstrap
      DriverInput
      DriverZInit
      DriverBuildEff
    Lex
      DriverLex
      DriverWindow
    Declarations
      DriverEnum
      DriverRecord
    Parse + recognizers
      DriverClassify
      DriverClaimIdx
      DriverPratt
      DriverExprDecomp
      DriverGroup
      DriverPosBind
      DriverGuard
      DriverQuant
      DriverMatchPin
      DriverCompose
    Symbols + registries
      DriverSymtab
      DriverSymLookup
      DriverSetVar
      DriverLitMem
    Lowering
      DriverRecVal
      DriverCallLower
      DriverBroadcast
    Emit
      DriverEmit
```

---

## Notes

- **Composition is by names-match, not imports.** A component's interface is
  the set of variable names it shares with `driver_main`. `..`-lift exposes
  the whole body to the shared namespace; **bare-mention** (`◆`: DriverGroup,
  DriverBroadcast) hides internals behind a declared header — the encapsulation
  direction. See `docs/plans/claim-headers-interface.md`.
- **`driver_ir.ev` is the keystone type module** — the `C2Item`/`C2Items`
  work-item IR, `FtiBuffer`, `Z3SolverCtx`, and the record/enum/set registry
  types every component reads/writes.
- **The artifact** `compiler.smt2` is the compiled form of this tree; it is
  rebuilt only on the (blocked) wave-5 path. Source edits here are validated
  through the frozen oracle (conformance + units), not by rebuilding the
  artifact.
