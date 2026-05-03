# Pre-Rewrite TODO: Things to Implement Before the Rust Rewrite

This document captures the features and design questions that should be
resolved in the Python prototype before committing to a Rust implementation.
The goal is not to implement everything perfectly, but to understand the design
well enough that the Rust version gets it right the first time.

---

## 1. CLI Cleanup

### Remove `evident run` (or rename)

`evident run` executes `?` query statements in a file. The REPL does this
interactively and is the right home for exploratory queries. For production
programs, `evident execute` (streaming automaton) and `evident batch` (batch
mode) are the correct entrypoints. `evident run` adds surface area without a
clear unique role.

**Decision needed:** Remove it, or repurpose it as `evident eval` (evaluate
all schemas and print results) to support scripted pipelines.

---

## 2. File I/O

We have `Stdin`/`Stdout` in `stdlib/io.ev` and the streaming executor handles
them. The logical extension is `File`-based equivalents:

```
schema FileLines(path ∈ String)
    lines ∈ Seq(String)

schema FileAll(path ∈ String)
    content ∈ String

schema FileWriter(path ∈ String)
    lines ∈ Seq(String)
```

The executor would need to:
- Detect file-based schemas in schema main
- Open the file at the given path before solving
- Provide its content as the sequence given

The `path` field introduces a configuration problem: how does the program
receive the path at runtime? This is the **argument parsing** question (§4).

---

## 3. Sockets

`stdlib/io.ev` already defines `UnixSocket` and `TcpSocket` schemas. The
executor doesn't yet support them. Implementing socket I/O would make Evident
programs capable of acting as servers, clients, or both.

Key design questions:
- **Connection lifecycle**: Who connects/listens? Probably schema main declares
  `src ∈ TcpSocket` and the executor opens the connection before the first solve.
- **Server mode**: A program that listens for connections needs a loop that
  accepts a new socket per connection — this is a multi-instance problem (§6).
- **Bidirectional**: Sockets are both readable and writable. How does schema
  main declare both input and output on the same socket?

---

## 4. Argument Parsing

Programs need runtime configuration — file paths, port numbers, flags. In Unix
this comes from `argv`. The natural Evident representation:

```
schema Args
    argv ∈ Seq(String)     -- all arguments
    argc ∈ Nat
    argc = #argv

schema Flag(name ∈ String)
    present ∈ Bool
    value   ∈ String       -- if the flag takes a value
```

Then schema main would declare `args ∈ Args` and the executor provides
`argv.0`, `argv.1`, etc. from `sys.argv`.

A more ergonomic design might define named flags explicitly:

```
schema main
    port  ∈ Nat
    host  ∈ String
    files ∈ Seq(String)
    
    -- @arg port  --port <n>
    -- @arg host  --host <s>
    -- @arg files ...
```

Where `-- @arg` annotations tell the executor how to populate those variables
from the command line. This is pure executor configuration — the schema body
stays clean constraint logic.

---

## 5. Multiple Files / Dynamic Inputs

This is the hardest design problem on the list.

**The scenario**: A program receives a list of file paths from argv, needs to
process each file with the same constraint logic, and produce one output per
input.

**Option A: Batch over a Seq**
Collect all file contents into `files ∈ Seq(Seq(String))` and let the
constraint system operate on the whole collection. This works if the processing
logic can be expressed with `∀ i ∈ {0..#files-1}: ...`. Avoids any dynamic
schema changes.

**Option B: Multiple schema main instances**
The executor runs schema main once per input file, treating it like a for-each.
Declared with something like:

```
schema main
    src  ∈ FileLines     -- one instance per file in argv
    dst  ∈ StdoutLines
    nd   ∈ NumberedDocument
    ...
```

The executor creates N instances of schema main, one per file in argv. Each
solve is independent.

**Option C: Stateful schema composition**
The constraint system itself maintains a "current schema" that can be extended
or modified at runtime. This is the most powerful but also the hardest to
specify — it blurs the line between the constraint description and the
execution model.

**Design recommendation**: Start with Option A (Seq of inputs), which is
expressible today. Option B (for-each executor) is a natural extension. Option
C should be deferred to post-rewrite.

---

## 6. Process Management

Spawn and communicate with subprocesses. This would let Evident programs
orchestrate other programs, use existing Unix tools in pipelines, or act as
supervisors.

The schema model:

```
schema Process
    command ∈ String
    args    ∈ Seq(String)
    stdin   ∈ Stdout    -- our output → process stdin
    stdout  ∈ Stdin     -- process stdout → our input
    stderr  ∈ Stdin     -- process stderr → our input
    exit_code ∈ Int
```

The executor would `subprocess.run` (or `Popen`) when it sees `p ∈ Process`
in schema main. This is higher-risk because process management involves timing
and resource cleanup — design carefully before the rewrite.

---

## 7. Testing Framework for Constraint Systems

Testing Evident programs is different from testing regular software. The
constraint solver replaces computation, so traditional unit tests don't
translate directly. Three distinct testing modes are needed:

### 7a. Schema correctness tests (what we have now)
Verify that a schema is SAT or UNSAT for given inputs. The conformance suite
in `tests/conformance/` does this.

### 7b. Property tests (sample-based)
Verify that ALL sampled solutions from a schema satisfy expected properties.
This is more powerful than individual query tests:

```python
# Every valid Schedule has slot > 0 and slot + duration ≤ budget
samples = sample('ValidSchedule', n=1000)
for s in samples:
    assert s['slot'] > 0
    assert s['slot'] + s['task.duration'] <= s['budget']
```

This uses the sampler to generate test vectors and the constraint system
itself as the oracle for checking them. It's a form of property-based testing
where Evident generates the test data.

### 7c. Execution tests (FSM traces)
Test streaming programs (`evident execute`) as finite automata. An execution
test is a sequence of (input_line, expected_output_line) pairs:

```yaml
program: programs/ev-nl.ev
traces:
  - input: "hello\n"
    output: "1\thello\n"
  - input: "world\n"
    output: "2\tworld\n"
  - input: ""   # EOF
    output: ""
```

The test runner feeds each input step and checks the output matches. This
tests the full program behavior including state transitions.

### 7d. Invariant tests
Declare properties that must hold for all reachable states of a program:

```
-- All steps: line_num ≥ 1
-- All steps: state.partial does not contain "\n"
-- After EOF: output is empty
```

These are temporal properties (like TLA+ invariants) that should hold at
every step of execution.

---

## 8. Remaining Language Features

Before the rewrite, these language features should be implemented and covered
by the conformance suite:

- **`∀ x ∈ seq`** — quantify directly over sequence elements (currently broken;
  the quantifier treats Seq as a Z3 Array, hits `domain()` error)
- **`index_of` and `sub_str`** — Z3 has `IndexOf` and `SubString`; expose them
  to enable `number_of` extraction from numbered lines
- **`⊴` and `⪯`** — contiguous and scattered subsequence operators
- **`str_to_int` as a constraint** — currently works via juxtaposition but
  edge cases need conformance tests

---

## 9. What We Understand Well Enough to Rewrite

These are fully designed and tested — a Rust port can proceed confidently:

- **Parser**: Grammar + normalizer + transformer. Language spec in `spec/`.
  Conformance fixtures in `parser/tests/fixtures/`.
- **Constraint translation**: Sorts, instantiation, translate, quantifiers, sets.
  Well-tested via conformance suite.
- **String/sequence operations**: ++, #, ∋, ⊑, ⊒, int_to_str, Seq(T).
- **Streaming executor**: Schema main + StdinLines/StdoutLines/Stdin/Stdout.
  Tested via programs/ test suite.
- **Notation system**: Parse-time AST rewrites with positional holes.
- **Batch execution**: Seq decomposition shim, StdinLines/StdoutLines.

---

## 10. What We Don't Understand Well Enough Yet

These need more design work in the prototype first:

- **Multiple inputs per schema main** — see §5
- **Socket lifecycle** — connection management, server vs client
- **Process management** — timing, cleanup, error handling
- **Argument parsing** — annotation syntax vs explicit schema fields
- **Dynamic schema composition** — whether and how schemas can compose
  based on runtime values
- **Testing framework** — exact format for execution traces and property tests
