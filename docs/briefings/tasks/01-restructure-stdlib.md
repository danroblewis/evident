# Task: Move WIP self-hosted compiler files from stdlib/ to compiler/

## Goal

`stdlib/` currently holds two unrelated kinds of code mixed together:
production stdlib (kernel.ev, combinatorics.ev, toposort.ev) and the
work-in-progress self-hosted compiler (lexer.ev, parser.ev, translate.ev,
translate_*.ev, ast.ev if it exists).

Move the WIP self-hosted compiler files to a new top-level `compiler/`
directory. Production stdlib stays in `stdlib/`.

## Why this matters

The freeze in CLAUDE.md requires that we work in `compiler/*.ev` for
the self-hosted compiler. Today those files don't exist; they live
under `stdlib/` and that mixing has caused real confusion (the
previous session thought the WIP translate files were "library
code", which contributed to misjudging the project's state).

This move makes the architectural status of every file visible from
its path: `stdlib/` = ships with the runtime; `compiler/` = part of
the deletion path replacing `bootstrap/`.

## Acceptance

The session is successful when ALL of these are true:

1. `compiler/` directory exists at the repo root and contains:
   - `compiler/lexer.ev` (from stdlib/lexer.ev)
   - `compiler/parser.ev` (from stdlib/parser.ev)
   - `compiler/translate.ev` (from stdlib/translate.ev — the C1 file)
   - `compiler/translate_arith.ev`, `translate_bool.ev`,
     `translate_compose.ev`, `translate_concat.ev`,
     `translate_declare.ev`, `translate_generics.ev`,
     `translate_infer.ev`, `translate_manifest.ev`,
     `translate_match.ev`, `translate_quant.ev`,
     `translate_record.ev`, `translate_seq.ev`,
     `translate_string.ev`
   - `compiler/ast.ev` if `stdlib/ast.ev` exists
   - `compiler/README.md` (described below)

2. `stdlib/` contains only stable runtime library:
   - `stdlib/kernel.ev`
   - `stdlib/combinatorics.ev`
   - `stdlib/toposort.ev`
   - any other library file that was already stable (NOT the
     WIP self-hosted-compiler files)

3. Every `import "stdlib/<wip-file>.ev"` reference is updated to
   `import "compiler/<wip-file>.ev"`. Search for these in
   `tests/`, `scripts/`, `stdlib/`, and `compiler/` and update each.

4. Each file in `compiler/` has a top-line comment of the exact form:
   `-- WIP: replaces bootstrap/runtime/src/<file-or-path>. STATUS: <one-line>.`
   For example, `compiler/lexer.ev` gets:
   `-- WIP: replaces bootstrap/runtime/src/lexer.rs. STATUS: token enum + per-char classifier; not yet driven on real .ev files.`
   You will need to look at `bootstrap/runtime/src/` to pick the right
   replacement target for each file.

5. `compiler/README.md` exists with:
   - One paragraph stating what the compiler/ directory is for and
     what it replaces.
   - A table mapping each `compiler/*.ev` to the bootstrap file it
     replaces.
   - The current status: "WIP — no real driver yet; per-pass
     fixtures only."
   - A pointer to `docs/plans/DELETION-CHECKLIST.md` Phase 2.

6. `./test.sh` passes.

7. The diff DOES NOT touch any file under `bootstrap/`. If you find
   a bootstrap file mentioned in a comment of a WIP file and the
   path was wrong (e.g. mentions `runtime/` from before the rename),
   correcting the comment IS allowed since you're editing the WIP
   file, not bootstrap.

## How to do it

1. `git checkout -b agent-restructure-stdlib origin/freeze-and-restructure`
   (the branch the coordinator created).
2. Inspect `stdlib/` and `bootstrap/runtime/src/` to decide which
   stdlib files are WIP-self-hosted-compiler vs stable library.
3. `mkdir compiler/` and use `git mv` to move WIP files.
4. Update import paths everywhere they're referenced.
5. Add the `-- WIP: replaces ...` header to each moved file.
6. Write `compiler/README.md`.
7. Run `./test.sh`. Fix any breakage caused by import-path updates.
8. Commit. Push to `origin/agent-restructure-stdlib`.

## Forbidden

- Editing any file under `bootstrap/`.
- Adding new `.py` files.
- Adding new lines to existing `.py` files.
- Editing `kernel/`.
- Editing CLAUDE.md (the coordinator owns that file).

## Reporting back

When done, your final message must include:

- Branch name pushed (`agent-restructure-stdlib` or whatever).
- Output of `find compiler stdlib -name '*.ev' | sort`.
- Output of `bash scripts/check-deletable.sh` after your changes.
- `./test.sh` final line (e.g. "All phases passed. (3s)").

That's it. Don't paste full files. The coordinator can `git show`.
