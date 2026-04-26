# Example 11: Conference Scheduling — Expressive Syntax

Same problem as examples 09–10, but exploring syntax that makes common
constraint patterns more concise. Three new ideas:

- `S grouped_by .field` — partition a set into groups sharing a field value
- `S.field` on a set — project one field across all members: `{ a.field | a ∈ S }`
- Destructuring in `∀` — bind record fields directly in the quantifier

```evident
type Speaker    = { name ∈ String, track ∈ String }
type Talk       = { title ∈ String, speaker ∈ Speaker, duration ∈ Nat, expected_audience ∈ Nat, requires_av ∈ Bool }
type Room       = { name ∈ String, capacity ∈ Nat, has_av ∈ Bool }
type Slot       = { day ∈ Nat, start ∈ Nat, end ∈ Nat }
type Assignment = { talk ∈ Talk, room ∈ Room, slot ∈ Slot }


claim assignment_valid
    a ∈ Assignment
    a.talk.duration ≤ a.slot.end - a.slot.start
    a.room.capacity ≥ a.talk.expected_audience
    a.talk.requires_av ⇒ a.room.has_av


claim valid_conference
    schedule     ∈ Set Assignment
    talks        ∈ Set Talk
    slots        ∈ Set Slot
    max_parallel ∈ Nat

    -- every talk scheduled exactly once
    ∀ talk ∈ talks : exactly 1 { a ∈ schedule | a.talk = talk }

    -- every assignment is individually valid
    ∀ a ∈ schedule : assignment_valid a

    -- within each slot: load, room conflicts, speaker conflicts, track spread
    ∀ by_slot ∈ schedule grouped_by .slot :
        at_most max_parallel by_slot
        all_different by_slot.room
        all_different by_slot.talk.speaker
        ∀ by_track ∈ by_slot grouped_by .talk.speaker.track :
            at_most 1 by_track

    -- bigger expected audiences go in bigger rooms
    ∀ { talk = t1, room = r1 } ∈ schedule,
      { talk = t2, room = r2 } ∈ schedule :
        t1.expected_audience > t2.expected_audience ⇒ r1.capacity ≥ r2.capacity


assert alice   = { name = "Alice",   track = "systems" }
assert bob     = { name = "Bob",     track = "ml" }
assert carol   = { name = "Carol",   track = "systems" }
assert dan     = { name = "Dan",     track = "ml" }
assert eve     = { name = "Eve",     track = "theory" }

assert talk_a  = { title = "Distributed consensus", speaker = alice, duration = 45, expected_audience = 200, requires_av = true }
assert talk_b  = { title = "Transformers at scale",  speaker = bob,   duration = 30, expected_audience = 300, requires_av = true }
assert talk_c  = { title = "Memory allocators",      speaker = carol, duration = 45, expected_audience = 150, requires_av = false }
assert talk_d  = { title = "Reward shaping",         speaker = dan,   duration = 30, expected_audience = 180, requires_av = true }
assert talk_e  = { title = "Linear types",           speaker = eve,   duration = 45, expected_audience = 100, requires_av = false }

assert hall   = { name = "Main Hall", capacity = 400, has_av = true }
assert room_b = { name = "Room B",    capacity = 200, has_av = true }
assert room_c = { name = "Room C",    capacity = 150, has_av = false }

assert slot_1 = { day = 1, start = 540, end = 600 }
assert slot_2 = { day = 1, start = 615, end = 660 }
assert slot_3 = { day = 1, start = 720, end = 780 }

assert talks        = { talk_a, talk_b, talk_c, talk_d, talk_e }
assert slots        = { slot_1, slot_2, slot_3 }
assert max_parallel = 3

assert schedule ∈ Set Assignment

valid_conference

-- solver produces:
-- schedule = {
--     { talk = talk_b, room = hall,   slot = slot_1 }
--     { talk = talk_a, room = room_b, slot = slot_1 }
--     { talk = talk_d, room = room_b, slot = slot_2 }
--     { talk = talk_c, room = room_c, slot = slot_2 }
--     { talk = talk_e, room = room_c, slot = slot_3 }
-- }
```
