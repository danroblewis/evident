# How the Scheduling Example Works

A visual breakdown of `valid_schedule` — the claim the user had trouble reading.

---

## The data being scheduled

Three tasks, two workers. Concrete numbers throughout.

```mermaid
classDiagram
    class Task {
        id : Nat
        name : String
        duration : Nat
        deadline : Nat
    }
    class Worker {
        id : Nat
        name : String
        available_from : Nat
        available_until : Nat
    }
    class Assignment {
        task_id : Nat
        worker_id : Nat
        start : Nat
    }

    Assignment --> Task : task_id references
    Assignment --> Worker : worker_id references
```

In our example:

| Task id | name | duration | deadline |
|---------|------|----------|----------|
| 1 | deploy | 60 min | by min 480 |
| 2 | test | 90 min | by min 540 |
| 3 | review | 30 min | by min 300 |

| Worker id | name | available |
|-----------|------|-----------|
| 1 | alice | min 0 → 480 |
| 2 | bob | min 120 → 600 |

A `Schedule` is just a list of `Assignment` records — each one says which worker does which task starting when.

---

## What `valid_schedule` is

```evident
claim valid_schedule : List Task -> List Worker -> Schedule -> Prop
```

`valid_schedule` is a **3-place relation**. It is established (true) when a given schedule
is valid for a given set of tasks and workers. It doesn't produce a schedule — it checks
(or constrains) one.

```mermaid
graph LR
    T["List Task\n(3 tasks)"]
    W["List Worker\n(alice, bob)"]
    S["Schedule\n(list of assignments)"]
    VS["valid_schedule\n(Prop — established or not)"]

    T --> VS
    W --> VS
    S --> VS

    style VS fill:#dbeafe,stroke:#2563eb
```

When we query `? valid_schedule tasks workers ?schedule`, the solver must find a value
for `?schedule` that makes the claim true.

---

## The four constraints — built up one at a time

`valid_schedule` is eventually defined by four simultaneous constraints:

```mermaid
graph TD
    VS["valid_schedule\ntasks workers schedule"]

    C1["① all_tasks_assigned\ntasks schedule\n\nevery task appears in the schedule"]
    C2["② all_assignments_valid\nworkers tasks schedule\n\nevery assignment uses a real worker\nwho is available at that time"]
    C3["③ no_overlapping_assignments\nschedule\n\nno worker has two tasks at once"]
    C4["④ all_deadlines_met\ntasks schedule\n\nevery task finishes before its deadline"]

    C1 -->|AND| VS
    C2 -->|AND| VS
    C3 -->|AND| VS
    C4 -->|AND| VS

    style VS fill:#dbeafe,stroke:#2563eb
    style C1 fill:#fef3c7,stroke:#d97706
    style C2 fill:#fef3c7,stroke:#d97706
    style C3 fill:#fef3c7,stroke:#d97706
    style C4 fill:#fef3c7,stroke:#d97706
```

What the solver is allowed to produce grows more constrained at each step:

```mermaid
graph LR
    S0["Step 0\nAnything goes\n\nschedule = []\nor garbage"]
    S1["Step 1\nAll tasks present\n\nbut worker_id=99\nor start=-100 ok"]
    S2["Step 2\nReal workers only\navailable at that time\n\nbut two tasks at once ok"]
    S3["Step 3\nNo overlaps\n\nbut past deadline ok"]
    S4["Step 4\nDeadlines met ✓\n\nUnique valid schedule"]

    S0 -->|add constraint ①| S1
    S1 -->|add constraint ②| S2
    S2 -->|add constraint ③| S3
    S3 -->|add constraint ④| S4

    style S4 fill:#dcfce7,stroke:#16a34a
```

---

## Unpacking constraint ② — the hardest one to read

The body of `assignment_valid` was the specific code the user found confusing:

```evident
evident assignment_valid workers tasks a
    find_worker a.worker_id workers ?worker
    find_task   a.task_id   tasks   ?task
    a.start >= worker.available_from
    a.start + task.duration <= worker.available_until
```

There are **two kinds of lines** mixed together here. Let's label them:

```mermaid
graph TD
    HEAD["assignment_valid workers tasks a\n(the assignment 'a' must be valid\ngiven these workers and tasks)"]

    subgraph LOOKUPS["Lookups — bind new variables"]
        L1["find_worker a.worker_id workers ?worker\n→ looks up the Worker record for a.worker_id\n→ binds it to the name 'worker'"]
        L2["find_task a.task_id tasks ?task\n→ looks up the Task record for a.task_id\n→ binds it to the name 'task'"]
    end

    subgraph CHECKS["Arithmetic constraints — use the bound variables"]
        C1["a.start >= worker.available_from\n→ can't start before worker is available"]
        C2["a.start + task.duration <= worker.available_until\n→ must finish before worker leaves"]
    end

    LOOKUPS -->|"once worker and task are known"| CHECKS
    CHECKS -->|"all hold → "| HEAD

    style HEAD fill:#dbeafe,stroke:#2563eb
    style LOOKUPS fill:#fef3c7,stroke:#d97706
    style CHECKS fill:#dcfce7,stroke:#16a34a
```

Concretely, for assignment `{ task_id=1, worker_id=1, start=30 }`:

| Line | What happens |
|------|-------------|
| `find_worker 1 workers ?worker` | finds `{ id=1, name="alice", available_from=0, available_until=480 }` |
| `find_task 1 tasks ?task` | finds `{ id=1, name="deploy", duration=60, deadline=480 }` |
| `30 >= 0` | alice is available at minute 30 ✓ |
| `30 + 60 <= 480` | deploy finishes at 90, alice is there until 480 ✓ |

---

## Constraint ③ — what "no overlap" means

```evident
evident non_overlapping a b tasks
    find_task a.task_id tasks ?ta
    find_task b.task_id tasks ?tb
    a.start + ta.duration <= b.start
        | b.start + tb.duration <= a.start
```

The `|` is disjunction: one OR the other must hold.

```mermaid
graph TD
    NV["non_overlapping a b"]
    F1["find_task a.task_id → ta"]
    F2["find_task b.task_id → tb"]
    OR{"either..."}
    D1["a finishes before b starts\na.start + ta.duration ≤ b.start"]
    D2["b finishes before a starts\nb.start + tb.duration ≤ a.start"]

    F1 --> OR
    F2 --> OR
    OR --> D1
    OR --> D2
    D1 -->|or| NV
    D2 -->|or| NV

    style OR fill:#f3e8ff,stroke:#7c3aed
```

For alice doing deploy (start=30, duration=60) and alice doing review (start=0, duration=30):
- Does review finish before deploy starts? `0 + 30 = 30 <= 30` ✓ Yes — no overlap.

For alice doing deploy (start=0) and alice doing test (start=0):
- Does deploy finish before test? `0 + 60 = 60 <= 0`? No.
- Does test finish before deploy? `0 + 90 = 90 <= 0`? No.
- Neither holds → overlap → constraint fails.

---

## The full claim dependency tree

```mermaid
graph TD
    VS["valid_schedule\ntasks workers schedule"]

    AT["all_tasks_assigned\ntasks schedule"]
    AV["all_assignments_valid\nworkers tasks schedule"]
    NO["no_overlapping_assignments\nschedule"]
    DL["all_deadlines_met\ntasks schedule"]

    TIA["task_is_assigned\nid schedule"]
    ASVAL["assignment_valid\nworkers tasks a"]
    SW["same_worker\na b"]
    NV["non_overlapping\na b tasks"]
    FA["find_assignment\nid schedule"]

    FW["find_worker\nid workers"]
    FT["find_task\nid tasks"]
    MEM["member\na schedule"]

    VS --> AT
    VS --> AV
    VS --> NO
    VS --> DL

    AT --> TIA
    TIA --> MEM

    AV --> ASVAL
    ASVAL --> FW
    ASVAL --> FT

    NO --> SW
    NO --> NV
    NV --> FT

    DL --> FA
    FA --> MEM

    style VS fill:#dbeafe,stroke:#2563eb
```

Every box is a `claim`. Every arrow is "requires." The solver walks this tree, posting
constraints at each node, propagating values upward until `valid_schedule` is established.

---

## What the solver actually does — with real numbers

The solver's job is to find values of `?schedule` (specifically, the `start` time in each
assignment and which worker does which task) satisfying all constraints simultaneously.

```mermaid
sequenceDiagram
    participant Q  as Query
    participant S  as Solver
    participant DB as Evidence Base

    Q  ->> S: ? valid_schedule tasks workers ?schedule
    Note over S: schedule = list of 3 assignments (one per task)\nworker_id and start for each are unknown

    S  ->> S: constraint ①: all 3 tasks must appear in schedule
    Note over S: schedule has 3 slots, task_ids = {1, 2, 3}

    S  ->> S: constraint ②: each assignment must use a real worker
    S  ->> S: alice (id=1) available 0→480, bob (id=2) available 120→600
    Note over S: each slot: worker_id ∈ {1, 2}, start within their window

    S  ->> S: constraint ②: task durations must fit in window
    S  ->> S: task 3 (review 30min): if alice, start ∈ [0, 450]
    S  ->> S: task 1 (deploy 60min): if alice, start ∈ [0, 420]
    S  ->> S: task 2 (test 90min):   if alice, start ∈ [0, 390]
    S  ->> S:                        if bob,   start ∈ [120, 510]

    S  ->> S: constraint ③: no worker does two tasks at the same time
    Note over S: for any two assignments with same worker_id:\none must finish before the other starts

    S  ->> S: constraint ④: all deadlines met
    S  ->> S: task 3 (review):  start + 30 ≤ 300  →  start ≤ 270
    S  ->> S: task 1 (deploy):  start + 60 ≤ 480  →  start ≤ 420
    S  ->> S: task 2 (test):    start + 90 ≤ 540  →  start ≤ 450

    Note over S: Propagation: task 3 has earliest deadline (300)\nmust be done first if alice does it

    S  ->> S: try: alice does review (start=0), then deploy (start=30)
    S  ->> S: review: 0+30=30 ≤ 300 ✓  deploy: 30+60=90 ≤ 480 ✓
    S  ->> S: no overlap: review ends 30, deploy starts 30 ✓
    S  ->> S: bob does test: start=120, 120+90=210 ≤ 540 ✓

    S  ->> DB: establish valid_schedule with this assignment
    DB -->> Q: schedule = [review@0/alice, deploy@30/alice, test@120/bob] ✓
```

---

## The valid schedule as a timeline

```mermaid
gantt
    title Valid Schedule (time in minutes from 00:00)
    dateFormat  X
    axisFormat  %s min

    section Alice
    review  (task 3) :done, 0, 30
    deploy  (task 1) :done, 30, 90

    section Bob
    test    (task 2) :done, 120, 210
```

Deadlines (for reference): review by 300, deploy by 480, test by 540. All met.

---

## Why the body lines are hard to read — and what they actually say

The specific block the user found confusing:

```evident
evident valid_allocation jobs resources slots
    all_jobs_allocated jobs slots
    all_slots_valid resources jobs slots
    no_resource_overlap resources jobs slots
    all_jobs_on_time jobs slots
```

Each line is: **claim-name  arg1  arg2  ...**

The hidden structure:

```mermaid
graph LR
    subgraph L1["all_jobs_allocated  jobs  slots"]
        P1["claim: all_jobs_allocated"]
        A1a["arg 1: jobs"]
        A1b["arg 2: slots"]
    end

    subgraph L2["all_slots_valid  resources  jobs  slots"]
        P2["claim: all_slots_valid"]
        A2a["arg 1: resources"]
        A2b["arg 2: jobs"]
        A2c["arg 3: slots"]
    end

    subgraph L3["no_resource_overlap  resources  jobs  slots"]
        P3["claim: no_resource_overlap"]
        A3a["arg 1: resources"]
        A3b["arg 2: jobs"]
        A3c["arg 3: slots"]
    end
```

What's not visible in the syntax:
- Which token is the claim name vs. which are arguments
- What role each argument plays (is `resources` the thing being checked, or the context?)
- Why `resources` appears in lines 2 and 3 but not line 1

This is the readability gap the language design needs to close.
The diagrams above make the structure visible. The syntax currently does not.
