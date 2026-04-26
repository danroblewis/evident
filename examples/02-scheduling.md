# Example 2: Scheduling — Composable Types and Constraint Accumulation

Scheduling is a naturally relational problem. There is no "scheduling algorithm" in this program —
just a description of what a valid schedule looks like, built from composable pieces.

---

## Types first

```evident
type Task = {
    id       : Nat
    name     : String
    duration : Nat      -- minutes
    deadline : Nat      -- minutes from start of day
}

type Worker = {
    id           : Nat
    name         : String
    available_from : Nat   -- minutes from start of day
    available_until : Nat
}

type Assignment = {
    task_id   : Nat
    worker_id : Nat
    start     : Nat    -- when the task begins
}

type Schedule = List Assignment
```

---

## Step 0: A claim with no constraints

```evident
claim valid_schedule : List Task -> List Worker -> Schedule -> Prop
```

```evident
assert tasks [
    { id = 1, name = "deploy",  duration = 60,  deadline = 480 }
    { id = 2, name = "test",    duration = 90,  deadline = 540 }
    { id = 3, name = "review",  duration = 30,  deadline = 300 }
]

assert workers [
    { id = 1, name = "alice", available_from = 0,   available_until = 480 }
    { id = 2, name = "bob",   available_from = 120, available_until = 600 }
]

? valid_schedule tasks workers ?schedule
```

```
-- Solver may return:
schedule = []                           -- valid (it's a List Assignment)
schedule = [{ task_id=1, worker_id=99, start=9999 }]  -- valid (no constraints)

-- Useless. We haven't said anything about what a valid schedule is.
```

---

## Step 1: Every task must be assigned

```evident
evident valid_schedule tasks workers schedule
    all_tasks_assigned tasks schedule
```

```evident
claim all_tasks_assigned : List Task -> Schedule -> Prop

evident all_tasks_assigned [] _schedule
evident all_tasks_assigned [task | rest] schedule
    task_is_assigned task.id schedule
    all_tasks_assigned rest schedule

claim task_is_assigned : Nat -> Schedule -> semidet

evident task_is_assigned id schedule
    member a schedule
    a.task_id = id
```

```evident
? valid_schedule tasks workers ?schedule
```

```
-- Solver may return:
schedule = [
    { task_id=1, worker_id=99, start=9999 }   -- wrong worker, impossible start
    { task_id=2, worker_id=0,  start=0    }   -- no such worker
    { task_id=3, worker_id=1,  start=-100 }   -- negative time
]
-- All tasks assigned, but by nonexistent workers at impossible times.
```

---

## Step 2: Workers must exist and be available

```evident
evident valid_schedule tasks workers schedule
    all_tasks_assigned tasks schedule
    all_assignments_valid workers tasks schedule
```

```evident
claim all_assignments_valid : List Worker -> List Task -> Schedule -> Prop

evident all_assignments_valid workers tasks []
evident all_assignments_valid workers tasks [a | rest]
    assignment_valid workers tasks a
    all_assignments_valid workers tasks rest

claim assignment_valid : List Worker -> List Task -> Assignment -> Prop

evident assignment_valid workers tasks a
    find_worker a.worker_id workers ?worker
    find_task   a.task_id   tasks   ?task
    a.start >= worker.available_from
    a.start + task.duration <= worker.available_until
```

Supporting claims:

```evident
claim find_worker : Nat -> List Worker -> Worker -> semidet
claim find_task   : Nat -> List Task   -> Task   -> semidet

evident find_worker id [w | _] w when w.id = id
evident find_worker id [_ | rest] result
    find_worker id rest result

-- find_task is symmetric
```

```evident
? valid_schedule tasks workers ?schedule
```

```
-- Solver may return:
schedule = [
    { task_id=1, worker_id=1, start=0   }   -- alice does deploy (ok)
    { task_id=2, worker_id=1, start=0   }   -- alice does test simultaneously! ← wrong
    { task_id=3, worker_id=2, start=120 }   -- bob does review (ok)
]
-- Workers are real and available. But tasks overlap!
```

---

## Step 3: No worker does two tasks simultaneously

```evident
evident valid_schedule tasks workers schedule
    all_tasks_assigned tasks schedule
    all_assignments_valid workers tasks schedule
    no_overlapping_assignments schedule
```

```evident
claim no_overlapping_assignments : Schedule -> Prop

evident no_overlapping_assignments schedule
    all pairs a b in schedule where a != b:
        same_worker a b => non_overlapping a b tasks

claim same_worker : Assignment -> Assignment -> semidet

evident same_worker a b when a.worker_id = b.worker_id

claim non_overlapping : Assignment -> Assignment -> List Task -> Prop

evident non_overlapping a b tasks
    find_task a.task_id tasks ?ta
    find_task b.task_id tasks ?tb
    a.start + ta.duration <= b.start
        | b.start + tb.duration <= a.start
```

Note the `|` for disjunction: either a finishes before b starts, or b finishes before a starts.

```evident
? valid_schedule tasks workers ?schedule
```

```
-- Solver may return:
schedule = [
    { task_id=1, worker_id=1, start=0   }   -- alice: deploy 0-60
    { task_id=2, worker_id=2, start=120 }   -- bob:   test 120-210
    { task_id=3, worker_id=1, start=60  }   -- alice: review 60-90
]
-- No overlaps! But task 2 (test) has deadline 540, that's fine.
-- task 3 (review) has deadline 300, 60+30=90 < 300, fine.
-- But: did we check deadlines?
```

---

## Step 4: All tasks must finish before their deadline

```evident
evident valid_schedule tasks workers schedule
    all_tasks_assigned tasks schedule
    all_assignments_valid workers tasks schedule
    no_overlapping_assignments schedule
    all_deadlines_met tasks schedule
```

```evident
claim all_deadlines_met : List Task -> Schedule -> Prop

evident all_deadlines_met [] _
evident all_deadlines_met [task | rest] schedule
    find_assignment task.id schedule ?a
    a.start + task.duration <= task.deadline
    all_deadlines_met rest schedule

claim find_assignment : Nat -> Schedule -> Assignment -> semidet

evident find_assignment id [a | _] a when a.task_id = id
evident find_assignment id [_ | rest] result
    find_assignment id rest result
```

```evident
? valid_schedule tasks workers ?schedule
```

```
-- Solver returns a valid schedule:
schedule = [
    { task_id=3, worker_id=1, start=0   }   -- alice: review 0-30  (deadline 300 ✓)
    { task_id=1, worker_id=1, start=30  }   -- alice: deploy 30-90 (deadline 480 ✓)
    { task_id=2, worker_id=2, start=120 }   -- bob:   test 120-210 (deadline 540 ✓)
]
-- ✓ All tasks assigned
-- ✓ All workers real and available
-- ✓ No overlaps
-- ✓ All deadlines met
```

---

## Composability: reusing sub-claims

The sub-claims we wrote are fully reusable:

```evident
-- Check if a proposed assignment is valid in isolation
? assignment_valid workers tasks { task_id=1, worker_id=1, start=0 }   -- Yes
? assignment_valid workers tasks { task_id=1, worker_id=2, start=0 }   -- No (bob not available at 0)

-- Check if a partial schedule has no overlaps
? no_overlapping_assignments [
    { task_id=1, worker_id=1, start=0 }
    { task_id=3, worker_id=1, start=30 }
]   -- Yes ✓

? no_overlapping_assignments [
    { task_id=1, worker_id=1, start=0 }
    { task_id=3, worker_id=1, start=0 }
]   -- No (alice can't do both at time 0)
```

---

## Making it parametric: generic resource scheduling

The scheduling model above is specific to workers. We can generalize:

```evident
-- A Resource has an id and an available time window
type Resource = {
    id    : Nat
    from  : Nat
    until : Nat
}

-- A Job has a duration and deadline
type Job = {
    id       : Nat
    duration : Nat
    deadline : Nat
}

-- A generic slot: job J assigned to resource R starting at time T
type Slot = { job_id : Nat, resource_id : Nat, start : Nat }

claim valid_allocation : List Job -> List Resource -> List Slot -> Prop

evident valid_allocation jobs resources slots
    all_jobs_allocated jobs slots
    all_slots_valid resources jobs slots
    no_resource_overlap resources jobs slots
    all_jobs_on_time jobs slots

-- Workers and Tasks are just specializations:
-- Worker = Resource, Task = Job, Assignment = Slot
```

Any domain fitting the Job/Resource/Slot pattern gets the full scheduler for free.

---

## Optimization: find the schedule that minimizes makespan

Once the model is fully constrained, we can add an objective:

```evident
? valid_schedule tasks workers ?schedule
    minimizing makespan schedule tasks
```

```evident
claim makespan : Schedule -> List Task -> Nat -> det

evident makespan schedule tasks m
    all finish_times of assignments in schedule
    m = max of those finish_times
```

The solver now searches for the valid schedule with the smallest makespan — a fully
constrained optimization problem, not just a feasibility query.
