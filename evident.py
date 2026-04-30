#!/usr/bin/env python3
"""
Evident command-line runtime.

Usage:
    evident run    file.ev [--given k=v ...]   Run all ? queries in the file
    evident check  file.ev                     Report SAT/UNSAT for every schema
    evident query  file.ev Schema [--given k=v ...] [--json]
    evident sample file.ev Schema [-n N] [--json]
    evident repl   [file.ev ...]               Interactive session

Inputs:  assert statements in the file, or --given on the command line
Outputs: ? query results printed to stdout
"""

import argparse
import json
import sys
from pathlib import Path

# Ensure project root on path
sys.path.insert(0, str(Path(__file__).parent))


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _load(files, given_overrides=None):
    from runtime.src.runtime import EvidentRuntime
    rt = EvidentRuntime()
    for f in files:
        rt.load_file(f)
    if given_overrides:
        for pair in given_overrides:
            k, _, v = pair.partition('=')
            try:
                val = int(v)
            except ValueError:
                try:
                    val = float(v)
                except ValueError:
                    val = v
            rt.assert_ground(k.strip(), val)
    return rt


def _fmt_bindings(bindings, as_json=False):
    cleaned = {k: v for k, v in bindings.items() if v is not None}
    if as_json:
        return json.dumps(cleaned, default=str)
    lines = [f'  {k} = {v}' for k, v in cleaned.items()]
    return '\n'.join(lines)


def _parse_given(given_list):
    """Parse ['x=3', 'y=hello'] → dict."""
    result = {}
    for pair in (given_list or []):
        k, _, v = pair.partition('=')
        k = k.strip()
        try:
            result[k] = int(v)
        except ValueError:
            try:
                result[k] = float(v)
            except ValueError:
                result[k] = v.strip()
    return result


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

def cmd_run(args):
    """Execute all ? query statements found in the file(s)."""
    rt = _load(args.files, args.given)
    given = _parse_given(args.given)

    if not rt.pending_queries:
        print('(no ? queries found — use  ?  SchemaName  to query a schema)', file=sys.stderr)
        return 0

    from runtime.src.ast_types import ApplicationConstraint, Identifier
    exit_code = 0
    for qstmt in rt.pending_queries:
        # Extract schema name from the constraint if possible
        c = qstmt.constraint
        name = None
        if isinstance(c, ApplicationConstraint):
            name = c.name
        elif isinstance(c, Identifier):
            name = c.name
        elif hasattr(c, 'name'):
            name = c.name

        if name and name in rt.schemas:
            r = rt.query(name, given=given)
            if r.satisfied:
                print(f'{name}: Satisfied')
                if r.bindings:
                    print(_fmt_bindings(r.bindings, args.json))
            else:
                print(f'{name}: Unsatisfiable')
                exit_code = 1
        else:
            print(f'(cannot resolve query: {c})', file=sys.stderr)

    return exit_code


def cmd_check(args):
    """Report SAT/UNSAT for every schema in the file(s)."""
    rt = _load(args.files)
    exit_code = 0
    for name in rt.schemas:
        r = rt.query(name)
        mark = '✓' if r.satisfied else '✗'
        print(f'{mark}  {name}')
        if not r.satisfied:
            exit_code = 1
    return exit_code


def cmd_query(args):
    """Find one satisfying assignment for a schema."""
    given = _parse_given(args.given)
    rt = _load(args.files, args.given)
    if args.schema not in rt.schemas:
        print(f'error: schema {args.schema!r} not found', file=sys.stderr)
        return 2
    r = rt.query(args.schema, given=given)
    if r.satisfied:
        if args.json:
            print(json.dumps({k: v for k, v in r.bindings.items() if v is not None}, default=str))
        else:
            print(f'{args.schema}: Satisfied')
            print(_fmt_bindings(r.bindings))
        return 0
    else:
        if args.json:
            print(json.dumps({'satisfied': False}))
        else:
            print(f'{args.schema}: Unsatisfiable')
        return 1


def cmd_sample(args):
    """Generate N satisfying assignments."""
    given = _parse_given(args.given)
    rt = _load(args.files)
    if args.schema not in rt.schemas:
        print(f'error: schema {args.schema!r} not found', file=sys.stderr)
        return 2

    sys.path.insert(0, str(Path(__file__).parent / 'ide' / 'backend'))
    from sampler import random_seed_sample

    src = '\n'.join(Path(f).read_text() for f in args.files)
    results = random_seed_sample(src, args.schema, given, args.n)

    if args.json:
        print(json.dumps([{k: v for k, v in r.bindings.items() if v is not None}
                          for r in results], default=str))
    else:
        print(f'{args.schema}: {len(results)} samples')
        for i, r in enumerate(results, 1):
            print(f'  [{i}] {_fmt_bindings(r.bindings).strip()}')
    return 0 if results else 1


def cmd_repl(args):
    """Interactive Evident session."""
    from runtime.src.runtime import EvidentRuntime
    rt = EvidentRuntime()
    for f in (args.files or []):
        rt.load_file(f)
        print(f'Loaded {f} ({len(rt.schemas)} schemas)')

    print('Evident interactive. Commands: import "file.ev" | ? Schema | quit')
    while True:
        try:
            line = input('> ').strip()
        except (EOFError, KeyboardInterrupt):
            print()
            break
        if not line or line.startswith('--'):
            continue
        if line in ('quit', 'exit', 'q'):
            break
        if line.startswith('import '):
            path = line[7:].strip().strip('"\'')
            try:
                rt.load_file(path)
                print(f'Loaded {path} ({len(rt.schemas)} schemas)')
            except Exception as e:
                print(f'Error: {e}')
            continue
        if line.startswith('?'):
            name = line[1:].strip()
            if name in rt.schemas:
                r = rt.query(name)
                if r.satisfied:
                    print(f'Satisfied')
                    print(_fmt_bindings(r.bindings))
                else:
                    print('Unsatisfiable')
            else:
                schemas = list(rt.schemas.keys())
                print(f'Unknown schema {name!r}. Loaded: {schemas}')
            continue
        if line.startswith('schemas'):
            print(', '.join(rt.schemas.keys()) or '(none)')
            continue
        print(f'Unknown command. Try: ? SchemaName | import "file.ev" | schemas | quit')
    return 0


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main():
    p = argparse.ArgumentParser(prog='evident', description='Evident constraint language runtime')
    sub = p.add_subparsers(dest='cmd', required=True)

    # run
    r = sub.add_parser('run', help='execute ? queries in file(s)')
    r.add_argument('files', nargs='+')
    r.add_argument('--given', nargs='*', metavar='k=v', default=[])
    r.add_argument('--json', action='store_true')

    # check
    c = sub.add_parser('check', help='report SAT/UNSAT for all schemas')
    c.add_argument('files', nargs='+')

    # query
    q = sub.add_parser('query', help='find one satisfying assignment')
    q.add_argument('files', nargs='+')
    q.add_argument('schema')
    q.add_argument('--given', nargs='*', metavar='k=v', default=[])
    q.add_argument('--json', action='store_true')

    # sample
    s = sub.add_parser('sample', help='generate N samples')
    s.add_argument('files', nargs='+')
    s.add_argument('schema')
    s.add_argument('-n', type=int, default=5)
    s.add_argument('--given', nargs='*', metavar='k=v', default=[])
    s.add_argument('--json', action='store_true')

    # repl
    rp = sub.add_parser('repl', help='interactive session')
    rp.add_argument('files', nargs='*')

    args = p.parse_args()
    dispatch = {'run': cmd_run, 'check': cmd_check,
                'query': cmd_query, 'sample': cmd_sample, 'repl': cmd_repl}
    sys.exit(dispatch[args.cmd](args))


if __name__ == '__main__':
    main()
