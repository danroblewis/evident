// Built-in example programs shown in the Examples dropdown.
// Each entry: { name: string, source: string }

const EXAMPLES = [

{
name: 'Scheduling',
source: `schema Task
    id       ∈ Nat
    duration ∈ Nat
    deadline ∈ Nat
    duration < deadline

schema ValidSchedule
    task   ∈ Task
    slot   ∈ Nat
    budget ∈ Nat
    slot > 0
    slot + task.duration ≤ budget
`
},

{
name: 'Date',
source: `type MonthName = Jan | Feb | Mar | Apr | May | Jun | Jul | Aug | Sep | Oct | Nov | Dec

claim month_has_name
    bronth    ∈ Nat
    monthName ∈ MonthName
    (bronth, monthName) ∈ {
        (1, Jan),  (2, Feb),  (3, Mar),
        (4, Apr),  (5, May),  (6, Jun),
        (7, Jul),  (8, Aug),  (9, Sep),
        (10, Oct), (11, Nov), (12, Dec)
    }

schema Date
    year  ∈ Nat
    month ∈ Nat
    day   ∈ Nat

    ..month_has_name (bronth ↦ month)

    0 < year
    1 ≤ month ≤ 12
    1 ≤ day   ≤ 31

    monthName = Feb                  ⇒ day ≤ 28
    monthName ∈ {Apr, Jun, Sep, Nov} ⇒ day ≤ 30
`
},

{
name: 'Enum colors',
source: `type Color  = Red | Green | Blue
type Size   = Small | Medium | Large
type Shape  = Circle | Square | Triangle

schema Widget
    color ∈ Color
    size  ∈ Size
    shape ∈ Shape

    -- Red widgets are never large
    color = Red ⇒ size ≠ Large

    -- Triangles must be small or medium
    shape = Triangle ⇒ size ∈ {Small, Medium}

    -- Blue squares are always large
    color = Blue ∧ shape = Square ⇒ size = Large
`
},

{
name: 'Correlated variables',
source: `schema ValidSchedule
    x ∈ Nat
    y ∈ Nat
    0 < x < 50
    y > x
    y < x * 2
`
},

{
name: 'Relational mapping',
source: `type Priority = Low | Medium | High | Critical

assert sla_hours = {
    (Low,      72),
    (Medium,   24),
    (High,      4),
    (Critical,  1)
}

schema Ticket
    priority    ∈ Priority
    hoursToFix  ∈ Nat
    assignees   ∈ Nat

    (priority, hoursToFix) ∈ sla_hours

    -- Critical tickets need at least 2 assignees
    priority = Critical ⇒ assignees ≥ 2
    assignees ≥ 1
`
},

{
name: 'Access control',
source: `type Role    = Admin | Editor | Viewer
type Action  = Read | Write | Delete | Publish

assert allowed = {
    (Admin,  Read),    (Admin,  Write),
    (Admin,  Delete),  (Admin,  Publish),
    (Editor, Read),    (Editor, Write),
    (Editor, Publish),
    (Viewer, Read)
}

schema Permission
    role   ∈ Role
    action ∈ Action
    (role, action) ∈ allowed
`
},

{
name: 'Set membership',
source: `schema PrimeSlot
    n ∈ Nat
    -- n must be a prime number (up to 20)
    n ∈ {2, 3, 5, 7, 11, 13, 17, 19}

schema CompositeSlot
    n ∈ Nat
    n ∈ {4, 6, 8, 9, 10, 12, 14, 15, 16, 18, 20}

schema AnySlot
    n ∈ Nat
    n ∈ {2, 3, 5, 7, 11, 13, 17, 19} ∪
        {4, 6, 8, 9, 10, 12, 14, 15, 16, 18, 20}
`
},

];
