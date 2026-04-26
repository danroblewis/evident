# Evident

**Evident** is a programming language concept built on a single premise: programs are collections of *claims about what is true*, not sequences of *commands about what to do*.

You start by naming something you want to be established — a high-level claim. The runtime asks: *can this be evidenced?* If yes, it is **evident**. If not, you decompose the claim into smaller sub-claims whose joint truth constitutes evidence for the whole. You keep decomposing until every leaf is directly checkable.

The order of evaluation is the system's problem, not yours.

---

## Key Ideas

| Idea | Where it comes from | What problem it solves |
|---|---|---|
| **Evidence over execution** | Proof assistants (Coq, Lean, Agda) | Programs that over-specify *how* when only *what* matters |
| **Implication as the core primitive** | Curry-Howard, logic programming | Dependencies are implicit in sequential code; here they are the structure |
| **Order independence** | Go maps, CRDTs, dataflow languages, ASP | Brittle sequential assumptions that hide bugs |
| **Top-down decomposition** | Tactic-based proof, build systems | The gap between a high-level specification and its implementation |

---

## Documents

- [**Vision**](vision.md) — What Evident is and why it should exist
- [**Prior Art**](prior-art.md) — Related languages, tools, and research
- [**Theoretical Foundations**](theoretical-foundations.md) — The formal ideas underneath
- [**Language Design**](language-design.md) — What programs look like, with examples
- [**Open Problems**](open-problems.md) — The hard questions not yet answered
