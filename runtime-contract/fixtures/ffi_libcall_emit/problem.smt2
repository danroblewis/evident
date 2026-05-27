; ffi_libcall_emit — transition relation for one tick of fsm `ffi_demo`
; FSM source: runtime-contract/fixtures/ffi_libcall_emit/source.ev
; Derived from: source.ev  claim sat_emits_libcall
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).
;
; effects_in_smt: true — the emitted FFI effect (a LibCall carrying a
; Seq(FFIArg) payload) is encoded in the effects Seq, so both SMT engines
; decode it from the model. (Capturing the EMITTED effect — what the dispatcher
; would run — not the C side effect; the libffi call is dispatch, not transition.)

; ── Datatype declarations ─────────────────────────────────────────────────────
; FFIArg first (referenced by Effect's LibCall payload field as (Seq FFIArg)).

(declare-datatypes ((FFIArg 0))
  (((ArgInt    (ArgInt_0    Int))
    (ArgBool   (ArgBool_0   Bool))
    (ArgStr    (ArgStr_0    String))
    (ArgReal   (ArgReal_0   Real))
    (ArgHandle (ArgHandle_0 Int)))))

(declare-datatypes
  ((FState 0) (Effect 0))
  (((FStart) (FDone))
   ((NoEffect)
    (Print   (Print_0   String))
    (Println (Println_0 String))
    (Exit    (Exit_0    Int))
    (IntToStr (IntToStr_0 Int))
    ; LibCall(library, symbol, signature, args) — the FFI effect.
    (LibCall (LibCall_0 String) (LibCall_1 String) (LibCall_2 String)
             (LibCall_3 (Seq FFIArg))))))

; ── FSM variables ──────────────────────────────────────────────────────────────

(declare-const state      FState)
(declare-const state_next FState)
(declare-const effects    (Seq Effect))

; ── Transition constraints ──────────────────────────────────────────────────────

; state_next = match state { FStart => FDone ; FDone => FDone }
(assert (= state_next
  (ite (is-FStart state) FDone FDone)))

; effects = match state
;   FStart => ⟨LibCall("libc", "abs", "i(i)", ⟨ArgInt(-7)⟩)⟩
;   FDone  => ⟨⟩
(assert (= effects
  (ite (is-FStart state)
       (seq.unit (LibCall "libc" "abs" "i(i)" (seq.unit (ArgInt (- 7)))))
       (as seq.empty (Seq Effect)))))
