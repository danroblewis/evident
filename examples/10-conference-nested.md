# Example 10: Conference Scheduling — Nested Claim Block

The same conference scheduling system from example 09, reorganised using a
nested claim block. All the sub-claims that share `schedule` are grouped
inside `Conference`, which declares the shared variables once.

```evident
type Speaker = {
    name  ∈ String
    track ∈ String
}

type Talk = {
    title             ∈ String
    speaker           ∈ Speaker
    duration          ∈ Nat
    expected_audience ∈ Nat
    requires_av       ∈ Bool
}

type Room = {
    name     ∈ String
    capacity ∈ Nat
    has_av   ∈ Bool
}

type Slot = {
    day   ∈ Nat
    start ∈ Nat
    end   ∈ Nat
}

type Assignment = {
    talk ∈ Talk
    room ∈ Room
    slot ∈ Slot
}


-- Helper claims (no shared context — used inside and outside Conference)

claim assignment_valid
    a ∈ Assignment
    a.talk.duration ≤ a.slot.end - a.slot.start
    a.room.capacity ≥ a.talk.expected_audience
    a.talk.requires_av ⇒ a.room.has_av

claim all_talks_scheduled
    talks    ∈ Set Talk
    schedule ∈ Set Assignment
    ∀ talk ∈ talks : exactly 1 { a ∈ schedule | a.talk = talk }


-- Nested claim block: all sub-claims share schedule, talks, rooms, slots, max_parallel

claim Conference
    schedule     ∈ Set Assignment
    talks        ∈ Set Talk
    rooms        ∈ Set Room
    slots        ∈ Set Slot
    max_parallel ∈ Nat

    claim rooms_conflict_free
        ∀ slot ∈ { a.slot | a ∈ schedule } :
            all_different { a.room | a ∈ schedule, a.slot = slot }

    claim speakers_conflict_free
        ∀ slot ∈ { a.slot | a ∈ schedule } :
            all_different { a.talk.speaker | a ∈ schedule, a.slot = slot }

    claim parallel_load_within
        ∀ slot ∈ slots :
            at_most max_parallel { a ∈ schedule | a.slot = slot }

    claim track_spread
        ∀ slot ∈ { a.slot | a ∈ schedule } :
            ∀ track ∈ { a.talk.speaker.track | a ∈ schedule } :
                at_most 1 { a ∈ schedule | a.slot = slot, a.talk.speaker.track = track }

    claim big_talks_in_big_rooms
        ∀ a, b ∈ schedule :
            a.talk.expected_audience > b.talk.expected_audience ⇒
                a.room.capacity ≥ b.room.capacity

    claim valid
        all_talks_scheduled
        ∀ a ∈ schedule : assignment_valid a
        rooms_conflict_free
        speakers_conflict_free
        parallel_load_within
        track_spread
        big_talks_in_big_rooms


-- Data

assert alice   = { name = "Alice",   track = "systems" }
assert bob     = { name = "Bob",     track = "ml" }
assert carol   = { name = "Carol",   track = "systems" }
assert dan     = { name = "Dan",     track = "ml" }
assert eve     = { name = "Eve",     track = "theory" }

assert talk_a  = { title = "Distributed consensus",  speaker = alice, duration = 45, expected_audience = 200, requires_av = true }
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
assert rooms        = { hall, room_b, room_c }
assert slots        = { slot_1, slot_2, slot_3 }
assert max_parallel = 3

assert schedule ∈ Set Assignment   -- unbound: solver fills this in


-- Assert that the conference is valid.
-- All variable names match the Conference block, so they flow automatically.
-- The solver populates 'schedule' to satisfy Conference.valid.

Conference.valid

-- solver produces:
-- schedule = {
--     { talk = talk_b, room = hall,   slot = slot_1 }
--     { talk = talk_a, room = room_b, slot = slot_1 }
--     { talk = talk_d, room = room_b, slot = slot_2 }
--     { talk = talk_c, room = room_c, slot = slot_2 }
--     { talk = talk_e, room = room_c, slot = slot_3 }
-- }
```
