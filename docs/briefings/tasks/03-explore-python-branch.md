# Task: Explore the Python-runtime branch for FTI + minimal-runtime techniques

## Background (context the coordinator gave you)

Before the current Rust runtime existed, Evident had a Python
runtime on a different git branch. That branch developed important
techniques for solving problems on a minimal kernel — most notably
the **Foreign Type Interface (FTI)**, which avoids the
Z3-model-explosion problem when an FSM needs to accumulate values
across ticks.

The FTI mechanism (as the coordinator currently understands it):

- An accumulator naively implemented with Z3 sequences/strings —
  carrying the growing value across ticks via state — causes the
  Z3 model to grow without bound and solving becomes intractable.
- FTI fixes this: the Z3-side value is just a `Seq` containing
  only the **tail / new portion**. To "push 42 onto a queue,"
  Evident writes `seq_var = _seq_var ++ ⟨42⟩` (where `_seq_var`
  is the carry from last tick). The Z3 model only sees `⟨42⟩`
  worth of new stuff. **The kernel runtime detects this is an FTI
  queue/stack and applies the operation to a real backing data
  structure it maintains externally** (not in Z3).
- This gives the Evident program the illusion of unbounded
  accumulators while keeping Z3's job tiny.

The coordinator's mental model on this is shallow and possibly
wrong. The exploration session needs to either confirm or correct
it from the actual Python source.

## Your task

This is **read-only research**. Do NOT modify any code or commit
to any branch in the main repo. Produce a single concise document.

1. **The branch is `tiny-runtime`** (the coordinator confirmed it
   from the user). Check it out into a temporary worktree:
   `git worktree add /tmp/evident-tiny-runtime origin/tiny-runtime`.
   Record its tip commit and a one-line description of how old it
   is (`git log -1 --format='%h %ad %s' --date=short origin/tiny-runtime`).

2. **Read the FTI implementation.** In the tiny-runtime worktree, read:
   - Any file named `fti*`, `foreign_type*`, `queue*`, `stack*`
   - The main runtime/kernel source
   - Effect dispatch code
   - Any tests that exercise FTI patterns
   - Any docs/notes about FTI or accumulators

3. **Document each FTI primitive.** For at least the queue and
   stack patterns, describe in your own words:
   - The Evident-side encoding (what does the FSM body look like?)
   - The kernel-side recognition (how does the runtime detect
     "this Seq value is the tail of an FTI queue"?)
   - The kernel-side storage (where does the real backing
     structure live?)
   - How values are read back into Evident on subsequent ticks
     (does `_<name>` see the full queue or just the head, etc.)
   - The single-writer rule's interaction with FTI

4. **Identify other minimal-runtime techniques.** Beyond FTI,
   what other patterns did the Python codebase develop for
   problems that don't fit in a single tick? Examples:
   recursion via work-stack, deferred I/O, multi-mode FSM,
   anything you find that's notably useful. Don't list
   everything — focus on patterns that would be load-bearing
   for transcribing `bootstrap/runtime/src/` to Evident on
   the current kernel.

5. **Recommend.** At the end of the document, write one or two
   paragraphs answering: "Should we bring the Python branch into
   this repo as reference material under (e.g.) `legacy-python/`,
   or are the learnings sufficient to capture in this document
   without copying the source?" Consider:
   - How big is the Python codebase? (`find . -name '*.py' | xargs wc -l`)
   - Are the techniques fully captured in your writeup, or would
     someone need to read code to use them?
   - Does the Python source contain test fixtures that demonstrate
     FTI usage we'd want to preserve?

## Output

Write your report to `docs/notes/python-branch-techniques.md` in
the main repo (which means committing it on a new branch off
`main`, not the Python branch). Do not write any other files. The
report should be 1–3 pages, terse, with code excerpts where
they're the clearest explanation.

Also produce, at the end of your final message, an inline summary:

- The branch name you explored (and its base commit, for posterity)
- The number of `.py` files in that branch
- The list of FTI primitives you found (just names)
- Your recommendation (bring in / capture only) in one sentence

## Forbidden

- Modifying any file under `bootstrap/`, `kernel/`, `compiler/`,
  or `stdlib/` on `main`.
- Editing any `.py` file on the Python branch (or anywhere).
- Cherry-picking or merging the Python branch's code into `main`.
- Anything destructive to the Python branch (no `git push --force`).

## Reporting back

Final message: branch name + 4-line summary as above + path to the
written report. Do not paste the report inline; the coordinator
will read it from the file.
