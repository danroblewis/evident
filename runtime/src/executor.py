"""
Constraint automaton executor.

Runs an Evident program as a constraint automaton:
  1. Load stdlib/io.ev (I/O trait schemas) and the user program.
  2. Find schema main and identify its port variables (∈ Stdin / ∈ Stdout)
     and state variable pairs (foo ∈ T  +  foo_next ∈ T).
  3. Initialize state: Nat → 0, String → "", Bool → False.
  4. Step loop:
       a. Read one character from stdin.
       b. Populate all Stdin-typed variable fields as given values.
       c. Populate current state fields as given values.
       d. Solve schema main.
       e. Write dst.out to stdout.
       f. Advance state: state_next.* → state.* for next step.
       g. On EOF: run one final step to flush partial output, then stop.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any


# ── Port schema names ─────────────────────────────────────────────────────────
# Schemas whose variables are driven by the runtime, not the solver.

# Streaming: character-by-character (step loop)
INPUT_SCHEMAS  = {'Stdin', 'CharInput'}
OUTPUT_SCHEMAS = {'Stdout', 'Stderr', 'CharOutput'}

# Batch: entire stdin/stdout in one solve
BATCH_INPUT_SCHEMAS  = {'StdinLines', 'StdinAll', 'StdinChunks'}
BATCH_OUTPUT_SCHEMAS = {'StdoutLines', 'StdoutAll'}

IO_SCHEMAS = INPUT_SCHEMAS | OUTPUT_SCHEMAS | BATCH_INPUT_SCHEMAS | BATCH_OUTPUT_SCHEMAS


# ── Type-based default initial values ─────────────────────────────────────────

def _default_for_type(type_name: str) -> Any:
    if type_name in ('Nat', 'Int'):
        return 0
    if type_name == 'Real':
        return 0.0
    if type_name == 'Bool':
        return False
    if type_name == 'String':
        return ''
    return None


# ── Executor ─────────────────────────────────────────────────────────────────

class EvidentExecutor:
    """
    Runs schema main as a constraint automaton against stdin/stdout.
    """

    STDLIB_PATH = Path(__file__).parent.parent.parent / 'stdlib' / 'io.ev'

    def __init__(self):
        from .runtime import EvidentRuntime
        self.rt = EvidentRuntime()

    def load(self, program_path: str) -> None:
        """Load the user program. stdlib is pulled in via import statements in the program."""
        self.rt.load_file(program_path)

    def load_source(self, source: str, load_stdlib: bool = True) -> None:
        """Load stdlib then the given source string."""
        if load_stdlib and self.STDLIB_PATH.exists():
            self.rt.load_file(str(self.STDLIB_PATH))
        self.rt.load_source(source)

    # ── Schema inspection ─────────────────────────────────────────────────────

    def _collect_vars(self, schema_name: str,
                      visited: set) -> dict[str, str]:
        """
        Recursively collect {var_name: type_name} from a schema and any
        schemas it includes via .. passthrough.  Follows passthrough chains
        so that port and state variables declared in sub-schemas (e.g.
        LineReader) are visible when inspecting schema main.
        """
        from .ast_types import (MembershipConstraint, Identifier,
                                MultiMembershipDecl, PassthroughItem)
        if schema_name in visited:
            return {}
        visited.add(schema_name)

        schema = self.rt.schemas.get(schema_name)
        if schema is None:
            return {}

        declared: dict[str, str] = {}
        for item in schema.body:
            if (isinstance(item, MembershipConstraint) and item.op == '∈'
                    and isinstance(item.left, Identifier)
                    and isinstance(item.right, Identifier)):
                declared[item.left.name] = item.right.name
            elif isinstance(item, MultiMembershipDecl):
                type_name = (item.set.name
                             if isinstance(item.set, Identifier) else 'unknown')
                for name in item.names:
                    declared[name] = type_name
            elif isinstance(item, PassthroughItem):
                # Follow the passthrough — don't override explicit declarations
                sub = self._collect_vars(item.name, visited)
                for v, t in sub.items():
                    if v not in declared:
                        declared[v] = t
        return declared

    def _inspect_main(self) -> tuple[dict, dict, dict]:
        """
        Scan schema main (following .. passthrough chains) and return:
          input_vars  : {var_name: schema_type}   — ∈ Stdin / ∈ CharInput
          output_vars : {var_name: schema_type}   — ∈ Stdout / ∈ CharOutput
          state_pairs : {base_var: (next_var, schema_type)}
        """
        if self.rt.schemas.get('main') is None:
            raise RuntimeError("No 'schema main' found in program.")

        declared = self._collect_vars('main', set())

        input_vars  = {v: t for v, t in declared.items()
                       if t in INPUT_SCHEMAS or t in BATCH_INPUT_SCHEMAS}
        output_vars = {v: t for v, t in declared.items()
                       if t in OUTPUT_SCHEMAS or t in BATCH_OUTPUT_SCHEMAS}

        non_io = {v: t for v, t in declared.items() if t not in IO_SCHEMAS}
        state_pairs: dict[str, tuple[str, str]] = {}
        for var, type_name in non_io.items():
            next_var = f'{var}_next'
            if next_var in non_io and non_io[next_var] == type_name:
                state_pairs[var] = (next_var, type_name)

        return input_vars, output_vars, state_pairs

    # ── Given construction ────────────────────────────────────────────────────

    def _stdin_given(self, var: str, char: str, eof: bool) -> dict:
        """Build given values for an ∈ Stdin variable."""
        return {
            f'{var}.fd':        0,
            f'{var}.open':      not eof,
            f'{var}.blocking':  True,
            f'{var}.available': 0 if eof else 1,
            f'{var}.eof':       eof,
            f'{var}.char':      char,
            # CharInput / Readable fields already covered above
        }

    def _stdout_given(self, var: str) -> dict:
        """Build given values for the non-output fields of ∈ Stdout."""
        return {
            f'{var}.fd':          1,
            f'{var}.open':        True,
            f'{var}.blocking':    True,
            f'{var}.send_buffer': 0,
            f'{var}.buffer_size': 8192,
            f'{var}.buffered':    0,
            f'{var}.flushed':     True,
        }

    def _state_given(self, var: str, state: dict) -> dict:
        """Convert a {field: value} state dict to dotted given keys."""
        return {f'{var}.{k}': v for k, v in state.items()}

    def _initial_state(self, type_name: str) -> dict:
        """
        Produce an initial state dict for a schema type by examining its body.
        Falls back to type-default values (0, "", False).
        """
        schema = self.rt.schemas.get(type_name)
        if schema is None:
            return {}
        from .ast_types import (MembershipConstraint, Identifier,
                                MultiMembershipDecl)
        state: dict[str, Any] = {}
        for item in schema.body:
            if (isinstance(item, MembershipConstraint) and item.op == '∈'
                    and isinstance(item.left, Identifier)
                    and isinstance(item.right, Identifier)):
                default = _default_for_type(item.right.name)
                if default is not None:
                    state[item.left.name] = default
            elif isinstance(item, MultiMembershipDecl):
                type_name_inner = (item.set.name
                                   if isinstance(item.set, Identifier) else None)
                if type_name_inner:
                    for name in item.names:
                        default = _default_for_type(type_name_inner)
                        if default is not None:
                            state[name] = default
        return state

    # ── Step extraction ───────────────────────────────────────────────────────

    def _extract_output(self, bindings: dict, output_vars: dict) -> str:
        """Collect output strings from all output port variables."""
        result = []
        for var in output_vars:
            out = bindings.get(f'{var}.out', '')
            if out:
                result.append(str(out))
        return ''.join(result)

    def _advance_state(self,
                       bindings: dict,
                       state_pairs: dict) -> dict[str, dict]:
        """
        Extract next-state values from bindings and return updated current-state
        dicts keyed by base variable name.
        """
        new_states: dict[str, dict] = {}
        for base_var, (next_var, _type) in state_pairs.items():
            new_state: dict[str, Any] = {}
            prefix = f'{next_var}.'
            for key, val in bindings.items():
                if key.startswith(prefix):
                    field = key[len(prefix):]
                    # Strip sequence/string display artefacts — keep raw values
                    new_state[field] = val
            new_states[base_var] = new_state
        return new_states

    # ── Main execution loop ───────────────────────────────────────────────────

    def _run_batch(self, input_stream, output_stream, batch_in, batch_out) -> None:
        """
        Batch execution: read all input, solve once, write all output.
        Used when schema main declares StdinLines, StdinAll, or StdoutLines.
        """
        given: dict = {}

        for var, schema_type in batch_in.items():
            if schema_type == 'StdinLines':
                lines = [line.rstrip('\n') for line in input_stream]
                given[f'{var}.lines'] = lines
            elif schema_type == 'StdinAll':
                given[f'{var}.content'] = input_stream.read()
            elif schema_type == 'StdinChunks':
                # chunk_size must be a given or a concrete constraint
                # For now, read as lines and let the schema constrain chunk_size
                lines = [line.rstrip('\n') for line in input_stream]
                given[f'{var}.chunks'] = lines

        result = self.rt.query('main', given=given)
        if not result.satisfied:
            import sys as _sys
            print("schema main: UNSAT — no solution for the given input.",
                  file=_sys.stderr)
            return

        for var, schema_type in batch_out.items():
            if schema_type in ('StdoutLines',):
                i = 0
                while True:
                    key = f'{var}.lines.{i}'
                    if key not in result.bindings:
                        break
                    val = result.bindings[key]
                    if val is not None:
                        output_stream.write(str(val) + '\n')
                    i += 1
            elif schema_type == 'StdoutAll':
                val = result.bindings.get(f'{var}.content', '')
                if val:
                    output_stream.write(str(val))

    def initialize(self) -> None:
        """Set up state for step-by-step execution via step_line()."""
        self._input_vars, self._output_vars, self._state_pairs = self._inspect_main()
        self._current_states: dict[str, dict] = {
            base_var: self._initial_state(type_name)
            for base_var, (_next_var, type_name) in self._state_pairs.items()
        }

    def step_line(self, command: str) -> dict:
        """
        Feed a complete command line character by character and return the
        state and output after line_ready fires (on the newline).

        Call initialize() before the first step_line().
        Returns {'state': {field: value, ...}, 'output': str, 'bindings': dict}
        """
        output = ''
        final_bindings: dict = {}

        for char in (command + '\n'):
            given: dict[str, Any] = {}
            for var in self._input_vars:
                given.update(self._stdin_given(var, char, False))
            for var in self._output_vars:
                given.update(self._stdout_given(var))
            for base_var, state in self._current_states.items():
                given.update(self._state_given(base_var, state))

            result = self.rt.query('main', given=given)
            if not result.satisfied:
                continue

            out = self._extract_output(result.bindings, self._output_vars)
            if out:
                output += out

            new_states = self._advance_state(result.bindings, self._state_pairs)
            for base_var in self._current_states:
                if base_var in new_states and new_states[base_var]:
                    self._current_states[base_var] = new_states[base_var]

            final_bindings = result.bindings

            if result.bindings.get('line_ready') is True:
                break

        flat_state = {}
        for s in self._current_states.values():
            flat_state.update(s)

        return {'state': flat_state, 'output': output.strip(), 'bindings': final_bindings}

    def run(self, input_stream=None, output_stream=None) -> None:
        """
        Execute schema main. Detects batch vs streaming mode from the
        declared port schemas and dispatches accordingly.

        Batch mode  (StdinLines, StdinAll, StdoutLines, StdoutAll):
            Reads all input at once, solves once, writes all output.

        Streaming mode (Stdin, Stdout via ..LineReader / ..LineWriter):
            Reads one character at a time, solves per step.
        """
        if input_stream is None:
            input_stream = sys.stdin
        if output_stream is None:
            output_stream = sys.stdout

        input_vars, output_vars, state_pairs = self._inspect_main()

        # Dispatch to batch mode if any batch I/O schemas are declared
        batch_in  = {v: t for v, t in input_vars.items()  if t in BATCH_INPUT_SCHEMAS}
        batch_out = {v: t for v, t in output_vars.items() if t in BATCH_OUTPUT_SCHEMAS}
        if batch_in or batch_out:
            return self._run_batch(input_stream, output_stream, batch_in, batch_out)

        # Fall through to streaming mode

        if not input_vars and not output_vars:
            raise RuntimeError(
                "schema main has no Stdin or Stdout variables. "
                "Declare e.g. src ∈ Stdin, dst ∈ Stdout."
            )

        # Initialize state for each state pair
        current_states: dict[str, dict] = {
            base_var: self._initial_state(type_name)
            for base_var, (_next_var, type_name) in state_pairs.items()
        }

        eof = False
        while True:
            # Read one character from the input stream
            char = input_stream.read(1)
            if char == '':
                eof = True

            # Build given dict
            given: dict[str, Any] = {}

            for var in input_vars:
                given.update(self._stdin_given(var, char, eof))

            for var in output_vars:
                given.update(self._stdout_given(var))

            for base_var, state in current_states.items():
                given.update(self._state_given(base_var, state))

            # Solve
            result = self.rt.query('main', given=given)

            if not result.satisfied:
                # Try to continue — unsatisfied steps are silently skipped
                # (could happen if constraints are incomplete)
                if eof:
                    break
                current_states = self._advance_state(result.bindings, state_pairs)
                continue

            # Write output
            out = self._extract_output(result.bindings, output_vars)
            if out:
                output_stream.write(out)
                output_stream.flush()

            # Advance state
            new_states = self._advance_state(result.bindings, state_pairs)
            for base_var in current_states:
                if base_var in new_states and new_states[base_var]:
                    current_states[base_var] = new_states[base_var]

            if eof:
                break
