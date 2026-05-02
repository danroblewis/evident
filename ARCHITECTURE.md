# Evident — Architecture

Evident is a constraint programming language backed by the Z3 SMT solver.
Programs are collections of constraints over sets; querying a schema asks whether
a satisfying assignment exists. The solver works bidirectionally — pin any subset
of variables and it solves for the rest.

---

## Pipeline Overview

Source text travels through a normalizer, parser, and multi-phase runtime before
reaching Z3. The IDE sits on top as a thin HTTP layer.

```mermaid
flowchart LR
    SRC["Source text\n(.ev file)"]
    NORM["Normalizer\nnormalizer.py"]
    LARK["Lark Earley\nParser\ngrammar.lark"]
    XFORM["Transformer\ntransformer.py"]
    AST["AST\nast.py"]
    RT["Runtime\nPipeline"]
    Z3["Z3 SMT\nSolver"]
    OUT["QueryResult\n(sat/unsat + bindings)"]

    SRC --> NORM
    NORM --> LARK
    LARK --> XFORM
    XFORM --> AST
    AST --> RT
    RT --> Z3
    Z3 --> OUT
```

---

## Parser

The grammar is the single source of truth for syntax. The normalizer runs first
to make the grammar purely ASCII.

```mermaid
flowchart TD
    subgraph Parser ["parser/src/"]
        N["normalizer.py\n────────────────\n∈ → __IN__\n⇒ → __IMPLIES__\n⟨ → __LSEQ__\n⊑ → __PREFIX__\n/regex/ → string literal\n(runs before Lark)"]
        G["grammar.lark\n────────────────\nLark Earley grammar\n317 lines\nschemas, constraints,\nexpressions, patterns"]
        T["transformer.py\n────────────────\nLark Tree → AST\n100+ visitor methods\nchained comparisons\nregex detection"]
        A["ast.py\n────────────────\nProgram, SchemaDecl\nMembershipConstraint\nArithmeticConstraint\nLogicConstraint\nUniversalConstraint\nExistentialConstraint\nBinaryExpr, SetLiteral\nSeqLiteral, RegexLiteral\n(40+ dataclass nodes)"]
        I["indenter.py\n────────────────\nIndentation-sensitive\nparsing (INDENT/DEDENT)"]
        P["parser.py\n────────────────\nOrchestrates the above\nnormalize → lark.parse\n→ transform"]
    end

    N --> G --> T --> A
    I --> G
    P --> N
```

---

## Runtime Pipeline

Eight modules form an ordered pipeline. Each stage transforms its input and
passes a richer structure to the next.

```mermaid
flowchart TD
    AST["AST (from parser)"]

    subgraph Runtime ["runtime/src/"]
        SR["sorts.py — Phase 1\nSortRegistry\nNat→IntSort, String→StringSort\nEnum→Datatype\nSeq→SeqSort\nAll Z3 sorts live here"]
        IN["instantiate.py — Phase 2\nCreate Z3 constants\nExpand sub-schemas\n(task → task.id, task.duration…)\nHandle passthrough (..SubSchema)\nReturns Environment + type constraints"]
        TR["translate.py — Phase 3\nAST → Z3 expressions\nRegex → z3.InRe\n++ → z3.Concat\nstarts_with → z3.PrefixOf\nint_to_str → z3.IntToStr\nField access interception"]
        SE["sets.py — Phase 4\nSet(T) = Array(T, Bool)\nUnion, intersection, difference\nvia Z3 Lambda expressions"]
        QU["quantifiers.py — Phase 5\n∀/∃ translation\nFinite domains: unroll to And/Or\nSymbolic: ForAll/Exists\nCardinality: PbLe/PbGe/PbEq"]
        CO["compose.py — Phase 6\nSchema composition\nnames-match relational join\nSlot renames (x ↦ parent_x)"]
        FP["fixedpoint.py — Phase 7\nForward implication rules\nA, B ⇒ C via Z3 Fixedpoint\n(spacer/PDR backend)"]
        EV["evaluate.py — Phase 8\nEvidentSolver\nRuns Z3 solver\nExtracts model to Python\nExpandsseq/string bindings"]
        EN["env.py\nImmutable Environment\nname → Z3 expr\nbind() / lookup()"]
        AT["ast_types.py\nRe-exports parser AST\n(ensures single class\nidentity for isinstance)"]
        EVI["evidence.py\nDerivation trees\nStructured proof records\nHow claims were established"]
        RT["runtime.py\nEvidentRuntime (top-level API)\nload_source / query\nSchema + rule registry\nImport cycle detection"]
    end

    AST --> SR --> IN --> TR --> SE --> QU --> CO --> FP --> EV --> RT
    EN -.->|"shared by all phases"| IN
    EN -.-> TR
    EN -.-> QU
    AT -.->|"re-exports AST types"| IN
    AT -.-> TR
    EVI --> RT
```

---

## IDE Architecture

The IDE is a single-page app backed by a FastAPI server. Z3 operations that could
crash the server process (sampling, range-finding) run in an isolated subprocess.

```mermaid
flowchart TB
    subgraph Browser ["Browser (ide/frontend/)"]
        ME["Monaco Editor\neditor.js\n────────────────\nAuto-substitution:\n'in' → ∈, '>>' → ⟩\nLive parse (500ms debounce)\nError decorations"]
        EL["evident-lang.js\nMonarch tokenizer\nSyntax highlighting\nDark theme"]
        SP["schema-panel.js\nSchema selector\nVariable binding inputs"]
        SA["samples.js\nSample accumulation\nUnique-key dedup\nCSV export\nTable rendering"]
        SC["scatter.js\n2D scatter / strip / count bars\nD3-based\nTooltips\n@plot annotations"]
        EX["examples.js\nFile browser modal\nBuilt-in examples\nSaved programs"]
    end

    subgraph Server ["FastAPI Server (ide/backend/main.py)"]
        PE["/parse\nParse source → schema list\nError locations"]
        EV["/evaluate\nQuery schema with given\nReturn sat + bindings"]
        SA2["/sample\n→ subprocess\nBlocking clause / random seed\nN diverse assignments"]
        RG["/ranges\n→ subprocess (cached)\nMin/max per variable"]
        FI["/files /examples\nList + save programs\nBuilt-in example loader"]
    end

    subgraph Worker ["Z3 Subprocess (z3_worker.py)"]
        SM["sampler.py\nrandom_seed_sample\nblocking_clause_sample\ngrid_sample"]
        RA["ranges.py\ncompute_ranges\n────────────────\nStage 1: Z3 Optimize\n(500ms timeout)\nStage 2: Iterative\ntightening (12 iters)"]
    end

    ME -->|"POST /parse"| PE
    ME -->|"POST /evaluate"| EV
    SA -->|"POST /sample"| SA2
    SC -->|"@plot annotations"| SA2
    SA2 -->|"subprocess"| SM
    RG -->|"subprocess"| RA
    SM --> RA

    PE --> ME
    EV --> ME
    SA2 --> SA
    SA --> SC
```

---

## Language Features Map

```mermaid
mindmap
  root((Evident))
    Types
      Nat
      Int
      Real
      Bool
      String
      Enum[type Color = Red : Green : Blue]
      Seq[Seq⟨T⟩]
    Schemas
      schema[schema Name]
      claim[claim Name]
      params[params x ∈ Nat, y ∈ Nat]
      passthrough[..SubSchema]
      rename[..Sub ⟨x ↦ y⟩]
    Constraints
      Membership
        in[x ∈ S]
        not_in[x ∉ S]
        contains[s ∋ t]
        subset[S ⊆ T]
        regex[s ∈ /pattern/]
      Arithmetic
        eq[x = y]
        compare[x < y ≤ z]
        chained[0 < x < 100]
      String
        prefix[s ⊑ t]
        suffix[s ⊒ t]
        concat[s ++ t]
        length[#s]
        int_to_str[int_to_str n]
      Logic
        and[P ∧ Q]
        or[P ∨ Q]
        implies[P ⇒ Q]
        not[¬P]
      Quantifiers
        forall[∀ x ∈ S : P]
        exists[∃ x ∈ S : P]
        unique[∃! x ∈ S : P]
        none[¬∃ x ∈ S : P]
    Expressions
      Set[{1, 2, 3}]
      Range[{1..100}]
      Seq[⟨a, b, c⟩]
      Tuple[⟨a, b⟩]
      Comprehension[{x ∈ S : P}]
      FieldAccess[task.duration]
```

---

## Directory Map: Active vs. Artifact

```mermaid
flowchart TD
    ROOT["evident/"]

    subgraph ACTIVE ["● Active Code"]
        P["parser/src/\n5 Python files\ngrammar + AST + transform"]
        R["runtime/src/\n14 Python files\nZ3 backend pipeline"]
        IB["ide/backend/\n4 Python files\nFastAPI + sampler + ranges"]
        IF["ide/frontend/\n7 JS files + HTML\nMonaco + D3 IDE"]
        IE["ide/examples/\n.ev example files\nloaded by IDE"]
        PT["parser/tests/\nruntime/tests/\nide/tests/\n~600 test cases"]
        CLI["evident.py\nCLI entry point"]
        SP["spec/\n10 .md files\nLanguage specification"]
    end

    subgraph DOC ["📄 Documentation"]
        DD["docs/design/\n21 design docs\narchitectural rationale"]
        DR["docs/research/\n12 research notes\nbackground exploration"]
        EX["examples/\n19 .md files\nlanguage design examples"]
        RM["README.md\nARCHITECTURE.md"]
    end

    subgraph ART ["🗑 Artifacts / Ephemeral"]
        PC["__pycache__/\nPython bytecode\n(auto-generated)"]
        PY[".pytest_cache/\npytest state"]
        PR["programs/\nuser-saved .ev files\n(runtime data, not code)"]
    end

    ROOT --> ACTIVE
    ROOT --> DOC
    ROOT --> ART
```

---

## What You'd Need to Rebuild to Switch Languages

The implementation has three largely independent layers. Each has a different
porting cost.

```mermaid
flowchart LR
    subgraph Easy ["🟢 Easy to Port\n(language-agnostic or trivial)"]
        G["grammar.lark\nLark-specific syntax but\nEarley grammars are standard.\nPort: rewrite for target parser\nor use Lark via subprocess"]
        N["normalizer.py\n~80 lines of string replacement.\nPort: 1–2 hours in any language"]
        A["ast.py\nSimple data types / structs.\nPort: 1 day in any language"]
        FE["ide/frontend/\nAlready JS — runs in any browser.\nNo porting needed"]
    end

    subgraph Medium ["🟡 Medium Effort\n(logic to rewrite, no exotic deps)"]
        T["transformer.py\n~730 lines of tree-walking.\nPort: 2–3 days — mostly\nmechanical visitor pattern"]
        IB["ide/backend/\nFastAPI HTTP endpoints.\nPort: 1–2 days in any\nweb framework"]
        SM["sampler.py / ranges.py\nSampling logic + Z3 Optimize.\nPort: 1 day (if Z3 bindings exist)"]
    end

    subgraph Hard ["🔴 Hard\n(deep Z3 integration)"]
        RT["runtime/ (10 modules)\n~3,500 lines of Z3 API usage.\nSorts, quantifiers, sets,\nsequences, fixedpoint,\nmodel extraction.\nPort: 2–4 weeks"]
    end

    subgraph Z3Lang ["Z3 Bindings by Language"]
        PY2["Python ✅ (current)"]
        TS["TypeScript / JS\nz3.wasm — full Z3 in browser\nNo server needed!"]
        RS["Rust\nz3-sys / z3 crates\nStrong type system"]
        CS["C# / F#\nMicrosoft.Z3 (official)"]
        JV["Java\ncom.microsoft.z3"]
        OC["OCaml\nvia C bindings"]
    end

    Easy --> Medium --> Hard
    Hard --> Z3Lang
```

### The Interesting Case: TypeScript + z3.wasm

Z3 ships as a WebAssembly module (`z3.wasm`) with a full TypeScript API.
Porting to TypeScript would mean:

- **No server required** — the entire runtime runs in the browser
- **Frontend stays as-is** (Monaco, D3)
- **Grammar and normalizer** port straightforwardly
- **Runtime modules** are the main effort — same logic, different Z3 API surface

This would make Evident a fully browser-resident tool with no Python dependency.

---

## Key Invariants (What Must Stay True in Any Port)

| Invariant | Why |
|---|---|
| Normalizer runs before parser | Grammar stays purely ASCII; Unicode handled in one place |
| Single AST class identity | `isinstance()` checks break if two module instances define the same class |
| SortRegistry owns all Z3 sorts | Enum variant names must be globally unique; duplicate detection centralised |
| Z3 runs in isolated subprocess | Z3's C library is not thread-safe; server crashes otherwise |
| Sets encoded as `Array(T, Bool)` | Z3 has no native finite-set sort; array theory is the standard encoding |
| Immutable environments | Sharing environments across branches requires no mutation |
| Normalizer handles both Unicode AND word keywords | `in`, `not in`, `subset` etc. map to same `__TOKEN__` as their symbol equivalents |

---

## External Dependencies

| Dependency | Role | Swappable? |
|---|---|---|
| **Z3** | SMT solver — the core engine | No (would need to pick a different solver) |
| **Lark** | Earley parser | Yes — any Earley/GLR parser |
| **FastAPI + Uvicorn** | HTTP server | Yes — any ASGI/web framework |
| **Monaco Editor** | Code editor | Yes — CodeMirror 6, Ace, etc. |
| **D3 v7** | 2D scatter plots | Yes — Vega-Lite, Chart.js, Plotly |
| **Playwright** | E2E tests | Yes — Puppeteer, Selenium |
