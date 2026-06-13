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
    rows, done = [], 0
    for name in task_names:
        tk = TASKS[name]
        for scale in tk.scales:
            for enc in tk.encodings:
                base = enc.build(scale)
                for seq in seqs:
                    g, tac_ms, err = tactics.apply(base, seq)
                    if err:
                        m = {"result": err, "min_ms": 0.0, "rlimit": None}
                    else:
                        m = harness.solve(g, reps, timeout_ms)
                    rows.append({
                        "task": name, "scale": scale, "encoding": enc.name,
                        "theories": "+".join(enc.theories),
                        "combo": tactics.seq_str(seq), "combo_len": len(seq),
                        "result": m["result"], "ok": m["result"] == enc.expected,
                        "tactic_ms": round(tac_ms, 2), "solve_ms": m["min_ms"],
                        "total_ms": round(tac_ms + m["min_ms"], 2), "rlimit": m["rlimit"],
                    })
                    done += 1
                if progress:
                    print(f"  {name}/{enc.name} N={scale}: {done}/{total}", file=sys.stderr)
    return rows
