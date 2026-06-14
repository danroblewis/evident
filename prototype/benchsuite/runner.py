"""The combinatorial sweep: task × scale × encoding × tactic-sequence."""
import sys
from . import tactics, harness
from .tasks import TASKS


def run(task_names, max_len, reps, timeout_ms, progress=True):
    seqs = list(tactics.sequences(max_len))
    enc_scale = sum(len(TASKS[t].encodings) * len(TASKS[t].scales) for t in task_names)
    total = enc_scale * len(seqs)
    if progress:
        print(f"tasks={task_names}  sequences/case={len(seqs)}  "
              f"(enc×scale)={enc_scale}  → {total} solves ×{reps} reps", file=sys.stderr)
    def row(name, scale, enc, combo, combo_len, result, tac_ms, solve_ms, rlimit, note=None):
        r = {
            "task": name, "scale": scale, "encoding": enc.name,
            "theories": "+".join(enc.theories),
            "combo": combo, "combo_len": combo_len,
            "result": result, "ok": result == enc.expected,
            "tactic_ms": round(tac_ms, 2), "solve_ms": round(solve_ms, 2),
            "total_ms": round(tac_ms + solve_ms, 2), "rlimit": rlimit,
        }
        if note:
            r["note"] = note
        return r

    rows, done = [], 0
    for name in task_names:
        tk = TASKS[name]
        for scale in tk.scales:
            for enc in tk.encodings:
                # Solve-only encoding (Fixedpoint/RecFunction): one direct call, no
                # tactic sweep — these engines don't take Goal+tactics.
                if enc.solve is not None:
                    try:
                        m = enc.solve(scale, timeout_ms)
                    except Exception as ex:        # never let the suite die on an engine
                        m = {"result": "unknown", "min_ms": 0.0, "rlimit": None,
                             "note": f"{type(ex).__name__}: {ex}"}
                    rows.append(row(name, scale, enc, "(none)", 0,
                                    m["result"], 0.0, m.get("min_ms", 0.0),
                                    m.get("rlimit"), m.get("note")))
                    done += 1
                    if progress:
                        print(f"  {name}/{enc.name} N={scale}: solve-only "
                              f"→ {m['result']}", file=sys.stderr)
                    continue
                base = enc.build(scale)
                for seq in seqs:
                    g, tac_ms, err = tactics.apply(base, seq)
                    if err:
                        m = {"result": err, "min_ms": 0.0, "rlimit": None}
                    else:
                        m = harness.solve(g, reps, timeout_ms)
                    rows.append(row(name, scale, enc, tactics.seq_str(seq), len(seq),
                                    m["result"], tac_ms, m["min_ms"], m["rlimit"]))
                    done += 1
                if progress:
                    print(f"  {name}/{enc.name} N={scale}: {done}/{total}", file=sys.stderr)
    return rows
