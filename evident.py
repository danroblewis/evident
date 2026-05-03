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

def _green(t):   return _c(t, 92)
def _red(t):     return _c(t, 91)
def _cyan(t):    return _c(t, 96)
def _yellow(t):  return _c(t, 93)
def _blue(t):    return _c(t, 94)
def _dim(t):     return _c(t, 2)
def _bold(t):    return _c(t, 1)
def _magenta(t): return _c(t, 95)
def _white(t):   return _c(t, 97)

def _highlight_constraint(text: str) -> str:
    """Syntax-colorize a pretty-printed Evident constraint for terminal display."""
    import re
    result = []
    i = 0
    while i < len(text):
        # String literals  "..."
        if text[i] == '"':
            j = text.index('"', i + 1) + 1 if '"' in text[i+1:] else len(text)
            result.append(_yellow(text[i:j]))
            i = j
            continue
        # Unicode operators — color them white/bold so they pop
        for op in ('∈', '∉', '⊆', '⊇', '∋', '∧', '∨', '¬', '⇒', '∀', '∃',
                   '≠', '≤', '≥', '++', '∪', '∩'):
            if text[i:].startswith(op):
                result.append(_white(_bold(op)))
                i += len(op)
                break
        else:
            # Identifiers: lowercase = blue (variables), uppercase-first = cyan (sets/types)
            m = re.match(r'[A-Za-z_][A-Za-z0-9_]*', text[i:])
            if m:
                word = m.group()
                if word[0].isupper():
                    result.append(_cyan(word))
                else:
                    result.append(_blue(word))
                i += len(word)
            else:
                result.append(text[i])
                i += 1
    return ''.join(result)

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

def _parse_trace_file(path: Path) -> list[dict]:
    """Parse a *.trace.ev file into a list of trace dicts."""
    import re
    traces = []
    current_trace = None
    current_step  = None
    awaiting_assertions = False

    for raw in path.read_text().splitlines():
        line    = raw.rstrip()
        content = line.strip()
        if not content or content.startswith('--'):
            continue
        indent = len(line) - len(line.lstrip())

        if content.startswith('trace '):
            m = re.match(r'trace\s+(\S+)\s+"([^"]+)"', content)
            if m:
                current_trace = {'name': m.group(1), 'program': m.group(2), 'steps': []}
                traces.append(current_trace)
            current_step = None
            awaiting_assertions = False

        elif content.startswith('send ') and current_trace is not None:
            awaiting_assertions = False
            if '=>' in content:
                send_part, _, rest = content.partition('=>')
                cmd = re.search(r'"([^"]*)"', send_part).group(1)
                rest = rest.strip()
                assertions = [rest] if rest else []
                awaiting_assertions = not bool(rest)
            else:
                cmd = re.search(r'"([^"]*)"', content).group(1)
                assertions = []
                awaiting_assertions = True
            current_step = {'cmd': cmd, 'assertions': assertions}
            current_trace['steps'].append(current_step)

        elif awaiting_assertions and current_step is not None and indent > 0:
            current_step['assertions'].append(content)

    return traces


def _check_trace_assertion(assertion: str, result: dict) -> tuple[bool, str]:
    """Check one assertion string against a step result. Returns (passed, detail)."""
    import re
    state  = result['state']
    output = result['output']

    m = re.match(r'(\w+)\s*∋\s*"([^"]*)"', assertion)
    if m:
        key, value = m.group(1), m.group(2)
        actual = output if key == 'output' else str(state.get(key, ''))
        return (value in actual), f'{_dim(assertion)}  actual: {_yellow(repr(actual))}'

    m = re.match(r'(\w+)\s*=\s*"([^"]*)"', assertion)
    if m:
        key, value = m.group(1), m.group(2)
        actual = output if key == 'output' else state.get(key)
        return (actual == value), f'{_dim(assertion)}  actual: {_yellow(repr(actual))}'

    return False, f'unparseable assertion: {assertion!r}'


def _user_bindings(bindings: dict) -> dict:
    """Filter solver bindings down to the ones a user cares about.

    Removes .length (string internals), sequence indices (.0, .1, …),
    and None values.
    """
    result = {}
    for k, v in bindings.items():
        if v is None:
            continue
        tail = k.rsplit('.', 1)[-1]
        if tail == 'length' or tail.isdigit():
            continue
        result[k] = v
    return result


def _claim_body(rt, name: str) -> list[str]:
    """Return the body constraints of a named schema as pretty-printed strings."""
    from runtime.src.ast_types import EvidentBlock, PassthroughItem
    from runtime.src.prettyprint import pretty_constraint

    schema = rt.schemas.get(name)
    if schema is None:
        return []
    lines = []
    for item in schema.body:
        if isinstance(item, (EvidentBlock, PassthroughItem)):
            continue
        try:
            lines.append(pretty_constraint(item))
        except Exception:
            pass
    return lines


def cmd_test(args):
    """Discover and run sat_*/unsat_* claims in test_*.ev files."""
    from runtime.src.runtime import EvidentRuntime

    search_path = Path(args.path) if args.path else Path('.')
    tests_dir = search_path / 'tests'

    if search_path.is_file():
        files       = [search_path] if not search_path.name.endswith('.trace.ev') else []
        trace_files = [search_path] if search_path.name.endswith('.trace.ev') else []
    else:
        files       = sorted(search_path.glob('test_*.ev'))
        trace_files = sorted(search_path.glob('*.trace.ev'))
        if tests_dir.is_dir():
            files       += sorted(tests_dir.glob('test_*.ev'))
            trace_files += sorted(tests_dir.glob('*.trace.ev'))

    if not files and not trace_files:
        print(_dim(f'No test_*.ev or *.trace.ev files found in {search_path}'))
        return 0

    # Each entry: (path, name, ok, expected_sat, detail)
    # detail: None | {'type': 'counterexample', 'bindings': {...}}
    #                 {'type': 'core', 'constraints': [...]}
    #                 {'type': 'error', 'message': str}
    results = []

    for path in files:
        rt = EvidentRuntime()
        try:
            rt.load_file(str(path))
        except Exception as e:
            results.append((path, str(path), False, None,
                            {'type': 'error', 'message': f'Failed to load: {e}'}))
            print(_red('E'), end='', flush=True)
            continue

        for name in rt.schemas:
            if name.startswith('sat_'):
                expected_sat = True
            elif name.startswith('unsat_'):
                expected_sat = False
            else:
                continue

            try:
                r = rt.query(name)
            except Exception as e:
                results.append((path, name, False, expected_sat,
                                {'type': 'error', 'message': str(e)}))
                print(_red('E'), end='', flush=True)
                continue

            ok = r.satisfied == expected_sat
            detail = None
            if not ok:
                if r.satisfied:
                    bindings = _user_bindings(r.bindings)
                    body     = _claim_body(rt, name)
                    detail = {'type': 'counterexample', 'bindings': bindings, 'body': body}
                else:
                    try:
                        core = rt.unsat_core(name)
                        detail = {'type': 'core', 'constraints': core}
                    except Exception:
                        detail = {'type': 'core', 'constraints': []}

            results.append((path, name, ok, expected_sat, detail))
            print(_green('.') if ok else _red('F'), end='', flush=True)

    # ── Trace tests (.trace.ev legacy format + trace decls in test_*.ev) ────────

    def _run_traces(path, trace_list):
        """Execute a list of trace objects (dicts or TraceDecl) and append to results."""
        from runtime.src.executor import EvidentExecutor
        from runtime.src.prettyprint import pretty_constraint

        for trace in trace_list:
            # Normalise: TraceDecl or plain dict (legacy)
            if hasattr(trace, 'steps'):
                name    = trace.name
                program = trace.program
                steps   = [(s.command, s.assertions) for s in trace.steps]
            else:
                name    = trace['name']
                program = trace['program']
                steps   = [(s['cmd'], s['assertions']) for s in trace['steps']]

            exe = EvidentExecutor()
            try:
                exe.load(program)
                exe.initialize()
            except Exception as e:
                results.append((path, name, False, None,
                                {'type': 'error', 'message': f'Failed to load {program}: {e}'}))
                print(_red('E'), end='', flush=True)
                continue

            trace_ok    = True
            fail_detail = []

            for i, (cmd, assertions) in enumerate(steps, 1):
                try:
                    step_result = exe.step_line(cmd)
                except Exception as e:
                    trace_ok = False
                    fail_detail.append(f'step {i} send "{cmd}": executor error: {e}')
                    break

                for assertion in assertions:
                    # assertion may be a Constraint AST node or a plain string
                    if isinstance(assertion, str):
                        passed, detail_str = _check_trace_assertion(assertion, step_result)
                        label = assertion
                    else:
                        label = pretty_constraint(assertion)
                        passed, detail_str = _check_trace_assertion(label, step_result)

                    if not passed:
                        trace_ok = False
                        fail_detail.append(
                            f'step {i} send "{cmd}" failed:\n'
                            f'      {_red("FAIL")} {detail_str}'
                        )

                if not trace_ok:
                    break

            ok = trace_ok
            det = None if ok else {'type': 'trace', 'lines': fail_detail}
            results.append((path, name, ok, True, det))
            print(_green('.') if ok else _red('F'), end='', flush=True)

    # Run trace declarations found inside test_*.ev files (already loaded into rt)
    for path in files:
        rt_for_traces = EvidentRuntime()
        try:
            rt_for_traces.load_file(str(path))
        except Exception:
            pass  # parse errors already reported in constraint section above
        if rt_for_traces.traces:
            _run_traces(path, list(rt_for_traces.traces.values()))

    # Run legacy *.trace.ev files (hand-parsed)
    for path in trace_files:
        try:
            traces = _parse_trace_file(path)
        except Exception as e:
            results.append((path, str(path), False, None,
                            {'type': 'error', 'message': f'Failed to parse: {e}'}))
            print(_red('E'), end='', flush=True)
            continue
        _run_traces(path, traces)

    print()

    failures = [(p, n, exp, d) for p, n, ok, exp, d in results if not ok]
    if failures:
        print()
        print(_bold('FAILURES'))
        print(_dim('─' * 60))
        for path, name, expected_sat, detail in failures:
            print()
            print(f'  {_dim(str(path))} :: {_cyan(name)}')
            if detail and detail['type'] == 'error':
                print(f'    {_red("ERROR")} {detail["message"]}')
            elif detail and detail['type'] == 'counterexample':
                from runtime.src.prettyprint import vars_in_constraint, pretty_constraint
                from runtime.src.ast_types import EvidentBlock, PassthroughItem
                print(f'    got {_red("SAT")}, expected UNSAT')
                print(f'    {_dim("all constraints were satisfied — counterexample:")}')
                schema = rt.schemas.get(name)
                for item in (schema.body if schema else []):
                    if isinstance(item, (EvidentBlock, PassthroughItem)):
                        continue
                    try:
                        text = pretty_constraint(item)
                        refs = vars_in_constraint(item)
                        witnesses = {k: v for k, v in detail['bindings'].items()
                                     if k in refs and v is not None}
                        print(f'      {_highlight_constraint(text)}')
                        for k, v in witnesses.items():
                            print(f'        {_blue(k)} = {_yellow(repr(v) if isinstance(v, str) else v)}')
                    except Exception:
                        pass
            elif detail and detail['type'] == 'trace':
                print(f'    {_red("trace failed")}')
                for line in detail['lines']:
                    for part in line.split('\n'):
                        print(f'    {part}')
            elif detail and detail['type'] == 'core':
                got, exp = 'UNSAT', 'SAT'
                print(f'    got {_red(got)}, expected {exp}', end='')
                if detail['constraints']:
                    print('  — conflicting constraints:\n')
                    for c in detail['constraints']:
                        print(f'      {_highlight_constraint(c)}')
                else:
                    print()
        print()
        print(_dim('─' * 60))

    passed  = sum(1 for (_, _n, ok, _e, _d) in results if ok)
    total   = len(results)
    failed  = total - passed
    parts   = []
    if failed:
        parts.append(_red(f'{failed} failed'))
    parts.append(_green(f'{passed} passed'))
    print('  '.join(parts))
    return 0 if failed == 0 else 1


def cmd_batch(args):
    """
    Batch mode: read all stdin lines as a sequence, query a schema once, write results.

    Reads every line from stdin into a list, passes it as the --in variable
    to the schema, then writes the elements of --out to stdout.

    Example (forward):  cat file.txt    | evident batch nl-batch.ev NumberedDocument --in contents --out lines
    Example (reverse):  cat numbered    | evident batch nl-batch.ev NumberedDocument --in lines    --out contents
    """
    from runtime.src.runtime import EvidentRuntime
    rt = EvidentRuntime()
    rt.load_file(args.file)

    input_lines = [line.rstrip('\n') for line in sys.stdin]
    result = rt.query(args.schema, given={args.input_var: input_lines})

    if not result.satisfied:
        print(f"UNSAT — {args.schema} has no solution for the given input.", file=sys.stderr)
        return 1

    i = 0
    while True:
        key = f'{args.output_var}.{i}'
        if key not in result.bindings:
            break
        val = result.bindings[key]
        if val is not None:
            sys.stdout.write(str(val) + '\n')
        i += 1
    return 0


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

    # batch mode
    bt = sub.add_parser('batch', help='read stdin as a sequence, solve a schema, write a sequence to stdout')
    bt.add_argument('file',       help='Evident program file')
    bt.add_argument('schema',     help='schema to query')
    bt.add_argument('--in',  dest='input_var',  required=True, help='variable to bind to all stdin lines as a sequence')
    bt.add_argument('--out', dest='output_var', required=True, help='sequence variable to extract and write to stdout')

    # execute (automaton mode)
    ex = sub.add_parser('execute', help='run schema main as a constraint automaton (reads stdin, writes stdout)')
    ex.add_argument('file', help='Evident program with schema main')

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

    # test
    te = sub.add_parser('test', help='run sat_*/unsat_* claims in test_*.ev files')
    te.add_argument('path', nargs='?', help='file or directory to search (default: current directory)')

    args = p.parse_args()
    dispatch = {'batch': cmd_batch, 'execute': cmd_execute, 'check': cmd_check,
                'query': cmd_query, 'sample': cmd_sample, 'repl': cmd_repl,
                'test': cmd_test}
    sys.exit(dispatch[args.cmd](args))


if __name__ == '__main__':
    main()
