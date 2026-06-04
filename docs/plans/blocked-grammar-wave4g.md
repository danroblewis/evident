# Blocked: wave-4g — the sole remaining `test_hello` gap (Blocker 2c)

After wave 4g, the self-hosted compile of the full flattened `test_hello`
SUCCEEDS (exit 0, ~14 min, 1436 ticks) and the emitted `.smt2` matches
bootstrap's in every way EXCEPT one renderer line. This note records that
single blocker precisely, and why it is a design gap rather than a
mechanical one. See `docs/plans/grammar-wave4g.md` for the wave's wins and
`docs/plans/blocked-grammar-wave4d.md` for the chain this descends from
(Blocker 2's "second facet").

## The one failing line

`test_hello`'s `hello` claim has:

```evident
effects ∈ Seq(Effect) = ⟨LibCall("libc", "puts", ⟨ArgStr("hello world")⟩), Exit(0)⟩
```

Self-hosted emits:

```
(assert (= (select effects 0) (LibCall "libc" "puts" (seq.unit (ArgStr "hello world")))))
```

Bootstrap emits:

```
(LibCall "libc" "puts" (__Cell_LibArg (ArgStr "hello world") __Empty_LibArg))
```

The `Effect` datatype (correctly, both ways) types the third field as
`(LibCall__f2 __SeqOf_LibArg)`. So `(seq.unit (ArgStr …))` — which has Z3
sort `(Seq LibArg)` — is a sort mismatch against `__SeqOf_LibArg`, and the
kernel rejects it:

```
Error: (error "… unknown constant LibCall (String String (Seq LibArg))
  declared: (declare-fun LibCall (String String __SeqOf_LibArg) Effect)")
```

This is the ONLY thing between `test_hello` and a clean `--semantic`
exit 0. (Declaration-order and assert-form differences are semantically
equivalent and already accepted by `--semantic`.)

## Why it is a DESIGN gap, not a mechanical one

The in-expression Seq-literal renderer is
`compiler/translate_ctor.ev::ListThread3.seq_wrapped`:

```
seq_wrapped = (n = 1 ? "(seq.unit " ++ o1 ++ ")"
            : n = 2 ? "(seq.++ (seq.unit " ++ o1 ++ ") (seq.unit " ++ o2 ++ "))"
            : …)
```

It is **element-type-agnostic** — it knows the rendered children (`o1`,
`o2`, …) but NOT that this Seq's element type is `LibArg`, so it cannot
name `__Cell_LibArg`/`__Empty_LibArg`. The same `seq_wrapped` is shared
across every nested-Seq position (RenderExprL1/L2/L3, both the ctor-arg
`cseq` and the whole-expr `sseq` slots), so it cannot simply be rewritten
to cons form.

A CORRECT fix has to thread the expected element type to the renderer:
the constructor application `LibCall(…, …, ⟨…⟩)` knows (from the `Effect`
enum's variant signature) that its third argument is `Seq(LibArg)`, and
must pass `LibArg` down so the Seq renders as
`(__Cell_LibArg <e> (__Cell_LibArg … __Empty_LibArg))`. That is a
cross-pass change (ctor renderer ← enum field-type table), and is its own
wave.

A LibArg-hardcoded `seq_wrapped` would make `test_hello` pass today
(the only Seq-in-ctor in the corpus is `Seq(LibArg)`), but it is exactly
the kind of fragile, non-element-type-driven special-case this project
avoids — it would silently mis-render any future `Seq(T≠LibArg)` payload.
**Not done.** Flagged for the element-type-threading wave instead.

## Recommended next step

Thread the ctor's variant field-types into `translate_ctor.ev` (a small
"field-type → element sort" lookup keyed off the constructor name + arg
index, populated from the enum decls the driver already parses) and make
`seq_wrapped` emit `__Cell_<T>`/`__Empty_<T>` when the position's element
sort is a `__SeqOf_<T>`. Then `test_hello` should reach a clean
`--semantic` exit 0 — and, since the datatypes/selection/structure are
already byte/semantically aligned, that would be the first full-corpus
self-host smoke pass. After that, only the structural Blockers 4 (L4
work-stack walker) and 5 (per-tick solve cost) remain — both kernel-side.
