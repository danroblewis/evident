# runtime/src/effect_dispatch.rs — Z3-replaceability
**What it does:** Dispatches `Effect` variants (Println, LibCall, FFIOpen, FFILookup, ReadLine, ParseInt, Exit, Spawn, …) to OS/IO/libffi. Manages `DispatchContext` with stdin/stdout handles, lib/sym caches, replay mode, and exit-request flag.
**Criticality:** critical
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Pure IO/control-flow: each Effect arm either prints, calls dlopen/dlsym/libffi, reads stdin, or sets exit state. There is no search over a solution space; there is nothing to "find" — we execute, not satisfy. A Z3 solve cannot replace OS calls or libffi dispatch. Matches the documented "Effect→IO" characterization exactly.
**Change made:** none
