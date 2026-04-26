# Example 11: Conference Scheduling — Syntactic Sugar

Two versions of the same constraint. The first is explicit and fully expanded.
The second uses shorthand for common patterns.

---

## Verbose — fully expanded

```evident
claim valid_conference
    schedule     ∈ Set Assignment
    talks        ∈ Set Talk
    rooms        ∈ Set Room
    slots        ∈ Set Slot
    max_parallel ∈ Nat

    ∀ slot ∈ { a.slot | a ∈ schedule } :
        all_different { a.room | a ∈ schedule, a.slot = slot }
        all_different { a.talk.speaker | a ∈ schedule, a.slot = slot }
        ∀ track ∈ { a.talk.speaker.track | a ∈ schedule } :
            at_most 1 { a ∈ schedule | a.slot = slot, a.talk.speaker.track = track }

    ∀ slot ∈ slots :
        at_most max_parallel { a ∈ schedule | a.slot = slot }

    ∀ a, b ∈ schedule :
        a.talk.expected_audience > b.talk.expected_audience ⇒
            a.room.capacity ≥ b.room.capacity

    ∀ a ∈ schedule : assignment_valid a
```

---

## Sugared — same constraints, shorter notation

| Sugar | Expands to |
|---|---|
| `S.field` | `{ a.field \| a ∈ S }` |
| `S grouped_by .field` | partition S into subsets sharing a field value |
| `∀ a ∈ S where .field = v` | `∀ a ∈ { x ∈ S \| x.field = v }` |
| `∀ a ≠ b ∈ S` | `∀ a, b ∈ S : a ≠ b ⇒ ...` |

```evident
claim valid_conference
    schedule     ∈ Set Assignment
    talks        ∈ Set Talk
    rooms        ∈ Set Room
    slots        ∈ Set Slot
    max_parallel ∈ Nat

    ∀ by_slot ∈ schedule grouped_by .slot :
        all_different by_slot.room
        all_different by_slot.talk.speaker
        ∀ by_track ∈ by_slot grouped_by .talk.speaker.track :
            at_most 1 by_track

    ∀ slot ∈ slots :
        at_most max_parallel schedule where .slot = slot

    ∀ { talk = t1, room = r1 } ≠ { talk = t2, room = r2 } ∈ schedule :
        t1.expected_audience > t2.expected_audience ⇒ r1.capacity ≥ r2.capacity

    ∀ a ∈ schedule : assignment_valid a
```
