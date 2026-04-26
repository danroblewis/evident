# Example 9: Conference Scheduling

A realistic constraint system composed from many sub-systems. The same primitive
(`all_different`) is reused parametrically for different resources. The top-level
claim is deliberately kept at "one line per requirement category."

```evident
type Speaker = {
    name  ∈ String
    track ∈ String
}

type Talk = {
    title            ∈ String
    speaker          ∈ Speaker
    duration         ∈ Nat       -- minutes
    expected_audience ∈ Nat
    requires_av      ∈ Bool
}

type Room = {
    name     ∈ String
    capacity ∈ Nat
    has_av   ∈ Bool
}

type Slot = {
    day   ∈ Nat
    start ∈ Nat    -- minutes from midnight
    end   ∈ Nat
}

type Assignment = {
    talk ∈ Talk
    room ∈ Room
    slot ∈ Slot
}


-- ── Sub-systems ───────────────────────────────────────────────────────────────

-- A single assignment is internally consistent.
claim assignment_valid
    a ∈ Assignment
    a.talk.duration ≤ a.slot.end - a.slot.start    -- talk fits in the slot
    a.room.capacity ≥ a.talk.expected_audience       -- room is large enough
    a.talk.requires_av ⇒ a.room.has_av              -- AV equipment present if needed

-- Every talk appears in the schedule exactly once.
claim all_talks_scheduled
    talks    ∈ Set Talk
    schedule ∈ Set Assignment
    ∀ talk ∈ talks : exactly 1 { a ∈ schedule | a.talk = talk }


-- No room is used by two talks in the same slot.
-- Reuses all_different from the primitives library.
claim rooms_conflict_free
    schedule ∈ Set Assignment
    ∀ slot ∈ { a.slot | a ∈ schedule } :
        all_different { a.room | a ∈ schedule, a.slot = slot }

-- No speaker gives two talks in the same slot.
-- Same pattern as rooms_conflict_free, different key.
claim speakers_conflict_free
    schedule ∈ Set Assignment
    ∀ slot ∈ { a.slot | a ∈ schedule } :
        all_different { a.talk.speaker | a ∈ schedule, a.slot = slot }

-- At most N talks run in parallel across any given slot.
claim parallel_load_within
    schedule     ∈ Set Assignment
    slots        ∈ Set Slot
    max_parallel ∈ Nat
    ∀ slot ∈ slots :
        at_most max_parallel { a ∈ schedule | a.slot = slot }

-- Talks from the same track are not all scheduled in the same slot
-- (spread them out so attendees can catch more than one).
claim track_spread
    schedule ∈ Set Assignment
    ∀ slot ∈ { a.slot | a ∈ schedule } :
        ∀ track ∈ { a.talk.speaker.track | a ∈ schedule } :
            at_most 1 { a ∈ schedule | a.slot = slot, a.talk.speaker.track = track }

-- The largest talks (by expected audience) are in the biggest rooms.
-- Every talk's room rank ≥ its audience rank among scheduled talks.
claim big_talks_in_big_rooms
    schedule ∈ Set Assignment
    ∀ a, b ∈ schedule :
        a.talk.expected_audience > b.talk.expected_audience ⇒
            a.room.capacity ≥ b.room.capacity


-- ── Top-level ─────────────────────────────────────────────────────────────────

claim valid_conference
    talks        ∈ Set Talk
    rooms        ∈ Set Room
    slots        ∈ Set Slot
    max_parallel ∈ Nat
    schedule     ∈ Set Assignment

    all_talks_scheduled           -- talks, schedule: names match outer scope
    ∀ a ∈ schedule : assignment_valid a
    rooms_conflict_free           -- schedule: names match
    speakers_conflict_free        -- schedule: names match
    parallel_load_within          -- schedule, slots, max_parallel: names match
    track_spread                  -- schedule: names match
    big_talks_in_big_rooms        -- schedule: names match


-- ── Data ─────────────────────────────────────────────────────────────────────

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

assert hall    = { name = "Main Hall",  capacity = 400, has_av = true }
assert room_b  = { name = "Room B",     capacity = 200, has_av = true }
assert room_c  = { name = "Room C",     capacity = 150, has_av = false }

assert slot_1  = { day = 1, start = 540,  end = 600  }   -- 9:00–10:00
assert slot_2  = { day = 1, start = 615,  end = 660  }   -- 10:15–11:00
assert slot_3  = { day = 1, start = 720,  end = 780  }   -- 12:00–13:00

assert conference_talks = { talk_a, talk_b, talk_c, talk_d, talk_e }
assert conference_rooms = { hall, room_b, room_c }
assert conference_slots = { slot_1, slot_2, slot_3 }

assert schedule ∈ Set Assignment   -- unbound: solver fills this in

valid_conference conference_talks conference_rooms conference_slots 3 schedule

-- solver produces (one valid assignment):
-- schedule = {
--     { talk = talk_b, room = hall,   slot = slot_1 }   -- biggest audience in biggest room
--     { talk = talk_a, room = room_b, slot = slot_1 }   -- systems + ml in parallel, ok
--     { talk = talk_d, room = room_b, slot = slot_2 }
--     { talk = talk_c, room = room_c, slot = slot_2 }   -- no AV needed, room_c fine
--     { talk = talk_e, room = room_c, slot = slot_3 }
-- }
```
