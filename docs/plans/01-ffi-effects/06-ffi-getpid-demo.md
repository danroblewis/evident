# Phase 1.6: First end-to-end Evident → FFI demo

## Goal

A program that, when run with `evident execute`, calls libc `getpid`
through FFI and prints the result. Validates the full pipeline:
Effect → dispatcher → libffi → real C call → Result back into the
Evident program → Effect → stdout.

## Prereqs

- Phase 1.5 (FFI wired) — done.

## What to build

`programs/demos/ffi_getpid.ev`:

```evident
import "stdlib/runtime.ev"

-- Three-step program:
--   step 0: open libc
--   step 1: lookup getpid
--   step 2: call getpid; print result; halt

type State =
    Init
    HaveLibc(Handle)
    HaveSym(Handle, Handle)         -- libc, getpid
    Done

claim main(state, state_next ∈ State,
           last_results ∈ Seq(Result),
           effects ∈ Seq(Effect))

    -- step 0: issue FFIOpen, no result yet to inspect
    state = Init ⇒
        (effects = ⟨FFIOpen("libSystem.dylib")⟩  -- Linux: "libc.so.6"
         ∧ ∃ h ∈ Handle :
             last_results[0] = HandleResult(h)
             ⇒ state_next = HaveLibc(h))

    -- step 1: lookup getpid
    state = HaveLibc(h) ⇒
        (effects = ⟨FFILookup(h, "getpid")⟩
         ∧ ∃ sym ∈ Handle :
             last_results[0] = HandleResult(sym)
             ⇒ state_next = HaveSym(h, sym))

    -- step 2: call, print, halt
    state = HaveSym(h, sym) ⇒
        (effects = ⟨FFICall(sym, "i()", ⟨⟩),
                    Println(int_to_string(get_int_result(last_results[0])))⟩
         ∧ state_next = Done)

    state = Done ⇒ (effects = ⟨⟩ ∧ state_next = Done)
```

(`int_to_string` and `get_int_result` are helper claims; this might
need string formatting we don't have yet — see "Notes" below.)

## Files touched

- `programs/demos/ffi_getpid.ev` (new)
- Possibly `stdlib/runtime.ev` for helper claims if not already
  there.

## Test it

```bash
evident execute programs/demos/ffi_getpid.ev
# Should print a positive integer (the runtime's PID).
```

Plus a trace test in the same file using the trace_runner:

```evident
trace getpid_returns_pid "programs/demos/ffi_getpid.ev"
    advance 0.1s
    advance 0.1s
    advance 0.1s
    -- After 3 steps, state should be Done.
```

## Acceptance

- [ ] `evident execute` runs the program end-to-end and prints a PID.
- [ ] All existing tests still pass.
- [ ] LOC: +~30 Evident.

## Notes

The big challenge: **Evident has no built-in int → string
conversion**. The Println effect takes a String. To print
`getpid()` (an Int), you need string formatting.

Three options:
1. **Add `IntToString(Int)` as a built-in effect** in this task.
   Cheap (~10 lines in dispatcher).
2. **Skip printing**, just verify state reached Done with the right
   Int captured in state.
3. **Add string-format FFI library wrapper** (call `snprintf`).
   Complete but larger scope.

Pick option 2 for v1 — the demo's job is to prove the FFI loop
works, not to format output. The trace test verifies state, not
stdout.

If the demo doesn't print, change it to:
```
state = HaveSym(h, sym) ⇒
    (effects = ⟨FFICall(sym, "i()", ⟨⟩)⟩
     ∧ ∃ pid ∈ Int :
         last_results[0] = IntResult(pid)
         ⇒ state_next = Done(pid))
```

And the trace test just asserts state's pid is reasonable.

Add `IntToString` later as part of Phase 1.7 (`stdlib/posix.ev`)
where it'll wrap `snprintf` via FFI.
