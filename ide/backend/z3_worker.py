"""
Subprocess worker for Z3 computations.

Reads a JSON request from stdin, runs the computation, writes JSON to stdout.
Running in a subprocess isolates Z3's global state from the web server process.

Usage:
    python z3_worker.py ranges < request.json
    python z3_worker.py sample < request.json
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent))
sys.path.insert(0, str(Path(__file__).parent))


def _run_ranges(req: dict) -> dict:
    from ranges import compute_ranges
    ranges = compute_ranges(req["source"], req["schema"], req.get("given", {}))
    return {"ranges": ranges}


def _run_sample(req: dict) -> dict:
    from sampler import blocking_clause_sample, random_seed_sample
    strategy = req.get("strategy", "random")
    n = req.get("n", 5)
    source = req["source"]
    schema = req["schema"]
    given = req.get("given", {})

    if strategy == "blocking":
        samples = blocking_clause_sample(source, schema, given, n)
    else:
        samples = random_seed_sample(source, schema, given, n)

    satisfied = [s.bindings for s in samples if s.satisfied]
    return {"samples": satisfied, "count": len(satisfied)}


def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "usage: z3_worker.py <ranges|sample>"}))
        sys.exit(1)

    command = sys.argv[1]
    req = json.loads(sys.stdin.read())

    try:
        if command == "ranges":
            result = _run_ranges(req)
        elif command == "sample":
            result = _run_sample(req)
        else:
            result = {"error": f"unknown command: {command}"}
    except Exception as e:
        result = {"error": str(e)}

    print(json.dumps(result))


if __name__ == "__main__":
    main()
