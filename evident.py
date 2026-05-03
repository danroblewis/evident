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

# ---------------------------------------------------------------------------
# ANSI colour helpers — disabled automatically when not writing to a terminal
# ---------------------------------------------------------------------------

def _tty():
    return hasattr(sys.stdout, 'isatty') and sys.stdout.isatty()

def _c(text, *codes):
    if not _tty():
        return str(text)
    return '\033[' + ';'.join(str(c) for c in codes) + 'm' + str(text) + '\033[0m'

def _green(t):  return _c(t, 92)
def _red(t):    return _c(t, 91)
def _cyan(t):   return _c(t, 96)
def _yellow(t): return _c(t, 93)
def _blue(t):   return _c(t, 94)
def _dim(t):    return _c(t, 2)
def _bold(t):   return _c(t, 1)

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
    lines = [f'  {_blue(k)} = {_yellow(v)}' for k, v in cleaned.items()]
    return '\n'.join(lines)


def _schema_vars(schema, schemas=None, _visited=None):
    """Return [(name, type_name, source_schema)] tracing through .. passthroughs."""
    from runtime.src.ast_types import (
        MembershipConstraint, Identifier, InlineEnumExpr,
        MultiMembershipDecl, PassthroughItem,
    )
    if _visited is None:
        _visited = set()
    if schema.name in _visited:
        return []
    _visited.add(schema.name)

    seen_vars = set()
    result = []

    def _add(name, type_name, source):
        if name not in seen_vars:
            seen_vars.add(name)
            result.append((name, type_name, source))

    for param in schema.params:
        t = param.set.name if isinstance(param.set, Identifier) else '?'
        for n in param.names:
            _add(n, t, schema.name)

    for item in schema.body:
        if isinstance(item, PassthroughItem) and schemas and item.name in schemas:
            for (n, t, src) in _schema_vars(schemas[item.name], schemas, _visited):
                _add(n, t, src)
        elif isinstance(item, MembershipConstraint) and item.op == '∈' \
                and isinstance(item.left, Identifier):
            n = item.left.name
            if isinstance(item.right, InlineEnumExpr):
                t = ' | '.join(item.right.variants)
            elif isinstance(item.right, Identifier):
                t = item.right.name
            else:
                t = '?'
            _add(n, t, schema.name)
        elif isinstance(item, MultiMembershipDecl):
            t = item.set.name if isinstance(item.set, Identifier) else '?'
            for n in item.names:
                _add(n, t, schema.name)

    return result


def _print_vars(schema, schemas=None):
    """Print variables with types, grouped by the sub-claim they come from."""
    vars_ = _schema_vars(schema, schemas)
    if not vars_:
        print(_dim('  (no declared variables)'))
        return
    width = max(len(n) for n, _, _ in vars_)
    current_src = None
    for name, type_name, src in vars_:
        if src != current_src:
            label = schema.name if src == schema.name else f'..{src}'
            print(_dim(f'  -- {label}'))
            current_src = src
        print(f'    {_blue(name):<{width + 9}}  ∈  {_cyan(type_name)}')


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

def cmd_execute(args):
    """Run schema main as a constraint automaton against stdin/stdout."""
    from runtime.src.executor import EvidentExecutor
    executor = EvidentExecutor()
    executor.load(args.file)
    try:
        executor.run()
    except KeyboardInterrupt:
        pass
    return 0


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
                print(f'{_cyan(name)}: {_green("Satisfied")}')
                if r.bindings:
                    print(_fmt_bindings(r.bindings, args.json))
            else:
                print(f'{_cyan(name)}: {_red("Unsatisfiable")}')
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
        if r.satisfied:
            print(f'{_green("✓")}  {_cyan(name)}')
        else:
            print(f'{_red("✗")}  {_cyan(name)}')
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
            print(f'{_cyan(args.schema)}: {_green("Satisfied")}')
            print(_fmt_bindings(r.bindings))
        return 0
    else:
        if args.json:
            print(json.dumps({'satisfied': False}))
        else:
            print(f'{_cyan(args.schema)}: {_red("Unsatisfiable")}')
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
        print(f'{_cyan(args.schema)}: {_bold(len(results))} samples')
        for i, r in enumerate(results, 1):
            print(f'  [{i}] {_fmt_bindings(r.bindings).strip()}')
    return 0 if results else 1


def cmd_repl(args):
    """Interactive Evident session."""
    # Enable readline: arrow keys, history, line editing.
    # Graceful fallback on Windows or if readline is unavailable.
    _history_file = Path.home() / '.evident_history'
    try:
        import readline as _rl

        # Load previous session history
        try:
            _rl.read_history_file(_history_file)
        except FileNotFoundError:
            pass
        _rl.set_history_length(1000)

        # Tab completion for schema names and keywords
        _KEYWORDS = ['import', 'quit', 'exit', 'schemas', 'sample', 'check', 'help', 'vars']

        def _completer(text, state):
            options = [k for k in (_KEYWORDS + list(rt.schemas.keys()))
                       if k.startswith(text)]
            return options[state] if state < len(options) else None

        _rl.set_completer(_completer)
        _rl.parse_and_bind('tab: complete')

        def _save_history():
            try:
                _rl.write_history_file(_history_file)
            except Exception:
                pass

        import atexit
        atexit.register(_save_history)
    except ImportError:
        pass   # Windows without pyreadline — input() still works, just no history

    from runtime.src.runtime import EvidentRuntime
    rt = EvidentRuntime()
    for f in (args.files or []):
        rt.load_file(f)
        print(_dim(f'Loaded {f} ({len(rt.schemas)} schemas)'))

    print('Evident interactive. Commands: import "file.ev" | ? Schema | quit')
    print('(↑↓ history, tab completion)')
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
        if line in ('help', '?', 'h'):
            print("""
Commands:
  ? SchemaName [k=v ...]  Query — prints bindings or Unsatisfiable
  vars SchemaName         List variables and their types
  import "file.ev"        Load an Evident source file into the session
  schemas                 List all loaded schema names
  sample Schema [N] [k=v] Print N diverse samples (default 5)
  check                   Report SAT/UNSAT for every loaded schema
  help                    Show this message
  quit / exit             Exit the REPL

Keyboard shortcuts:
  ↑ / ↓                   Navigate history
  ← / →                   Move cursor
  Tab                     Complete schema names and keywords
  Ctrl-R                  Search history
  Ctrl-C / Ctrl-D         Exit
""".strip())
            continue
        if line.startswith('sample'):
            parts = line.split()
            name  = parts[1] if len(parts) > 1 else None
            # remaining tokens: optional integer N and key=val pairs
            rest  = parts[2:]
            n     = 5
            given = {}
            for tok in rest:
                if '=' in tok:
                    given.update(_parse_given([tok]))
                else:
                    try:
                        n = int(tok)
                    except ValueError:
                        pass
            if not name:
                print('Usage: sample SchemaName [N] [key=val ...]')
            elif name not in rt.schemas:
                print(f'Unknown schema {name!r}. Try: schemas')
            else:
                sys.path.insert(0, str(Path(__file__).parent / 'ide' / 'backend'))
                try:
                    from sampler import random_seed_sample
                    src = '\n'.join(Path(f).read_text() for f in (args.files or []))
                    results = random_seed_sample(src, name, given, n)
                    print(f'{_cyan(name)}: {_bold(len(results))} samples')
                    for i, r in enumerate(results, 1):
                        print(f'  [{i}] {_fmt_bindings(r.bindings).strip()}')
                except Exception as e:
                    print(f'Error: {e}')
            continue
        if line == 'check':
            for name in rt.schemas:
                r = rt.query(name)
                print((_green('✓') if r.satisfied else _red('✗')) + f'  {_cyan(name)}')
            continue
        if line.startswith('import '):
            path = line[7:].strip().strip('"\'')
            try:
                rt.load_file(path)
                print(_dim(f'Loaded {path} ({len(rt.schemas)} schemas)'))
            except Exception as e:
                print(f'Error: {e}')
            continue
        if line.startswith('?'):
            parts = line[1:].strip().split()
            name  = parts[0] if parts else ''
            given = _parse_given(p for p in parts[1:] if '=' in p)
            if name in rt.schemas:
                r = rt.query(name, given=given)
                if r.satisfied:
                    print(_green('Satisfied'))
                    print(_fmt_bindings(r.bindings))
                else:
                    print(_red('Unsatisfiable'))
            else:
                print(f'Unknown schema {name!r}. Try: schemas')
            continue
        if line.startswith('vars'):
            name = line[4:].strip()
            if name not in rt.schemas:
                print(f'Unknown schema {name!r}. Try: schemas')
            else:
                _print_vars(rt.schemas[name], rt.schemas)
            continue
        if line.startswith('schemas'):
            print(', '.join(rt.schemas.keys()) or '(none)')
            continue
        print(f'Unknown command. Type  help  for a list of commands.')
    return 0


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main():
    p = argparse.ArgumentParser(prog='evident', description='Evident constraint language runtime')
    sub = p.add_subparsers(dest='cmd', required=True)

    # execute (automaton mode)
    ex = sub.add_parser('execute', help='run schema main as a constraint automaton (reads stdin, writes stdout)')
    ex.add_argument('file', help='Evident program with schema main')

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
    dispatch = {'execute': cmd_execute, 'run': cmd_run, 'check': cmd_check,
                'query': cmd_query, 'sample': cmd_sample, 'repl': cmd_repl}
    sys.exit(dispatch[args.cmd](args))


if __name__ == '__main__':
    main()
