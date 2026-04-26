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
| `S.field` | `{ a.field \| a ∈ S }` — project a field across a set |
| `S[condition]` | `{ a ∈ S \| condition }` — filter, `.` refers to current element |
| `S[.field = v]` | `{ a ∈ S \| a.field = v }` — filter by field value |
| `S.field[condition]` | project then filter — chains freely |
| `S grouped_by .field` | partition S into subsets sharing a field value |
| `∀ a ≠ b ∈ S` | `∀ a, b ∈ S : a ≠ b ⇒ ...` — distinct pairs |
| `∀ S : claim` | `∀ a ∈ S : claim a` — apply a one-argument claim to every element |
| `∀ S[condition] : claim` | filter then apply |

```evident
claim valid_conference
    schedule     ∈ Set Assignment
    talks        ∈ Set Talk
    rooms        ∈ Set Room
    slots        ∈ Set Slot
    max_parallel ∈ Nat

    ∀ slot_assignments ∈ schedule grouped_by .slot :
        all_different slot_assignments.room
        all_different slot_assignments.talk.speaker
        ∀ track_assignments ∈ slot_assignments grouped_by .talk.speaker.track :
            at_most 1 track_assignments

    ∀ slot ∈ slots :
        at_most max_parallel schedule[.slot = slot]

    ∀ { talk = t1, room = r1 } ≠ { talk = t2, room = r2 } ∈ schedule :
        t1.expected_audience > t2.expected_audience ⇒ r1.capacity ≥ r2.capacity

    ∀ schedule : assignment_valid
```

---

## Named subsets — binding a set to a name for multiple assertions

When you need to make several assertions about the same derived set,
name it with an inline binding or a sub-claim. The naming problem:
`rooms_for_slot` sounds like a procedure, not a constraint. Options shown below —
pick whichever reads best.

```evident
-- Option A: inline binding with _name
-- No sub-claim needed. _rooms is a body-internal variable.
claim valid_conference
    schedule     ∈ Set Assignment
    talks        ∈ Set Talk
    rooms        ∈ Set Room
    slots        ∈ Set Slot
    max_parallel ∈ Nat

    ∀ slot ∈ slots :
        _rooms   = schedule[.slot = slot].room
        _speakers = schedule[.slot = slot].talk.speaker
        _tracks  = schedule[.slot = slot].talk.speaker.track
        all_different _rooms
        all_different _speakers
        ∀ track ∈ _tracks :
            at_most 1 schedule[.slot = slot, .talk.speaker.track = track]
        at_most max_parallel schedule[.slot = slot]

    ∀ { talk = t1, room = r1 } ≠ { talk = t2, room = r2 } ∈ schedule :
        t1.expected_audience > t2.expected_audience ⇒ r1.capacity ≥ r2.capacity

    ∀ schedule : assignment_valid


-- Option B: sub-claim with a relational name
-- The concept "slot assignments" is named; three assertions apply to it.
claim slot_assignments_of
    schedule         ∈ Set Assignment
    slot             ∈ Slot
    slot_assignments ∈ Set Assignment
    slot_assignments = schedule[.slot = slot]

claim valid_conference
    schedule     ∈ Set Assignment
    talks        ∈ Set Talk
    rooms        ∈ Set Room
    slots        ∈ Set Slot
    max_parallel ∈ Nat

    ∀ slot ∈ slots :
        slot_assignments_of slot _assignments
        all_different _assignments.room
        all_different _assignments.talk.speaker
        at_most max_parallel _assignments
        ∀ track ∈ _assignments.talk.speaker.track :
            at_most 1 _assignments[.talk.speaker.track = track]

    ∀ { talk = t1, room = r1 } ≠ { talk = t2, room = r2 } ∈ schedule :
        t1.expected_audience > t2.expected_audience ⇒ r1.capacity ≥ r2.capacity

    ∀ schedule : assignment_valid
```
