# Constraints as Builders: A More Powerful Alternative to the Builder Pattern

## 1. The Traditional Builder Pattern in OOP

The Builder pattern exists to solve a specific, concrete problem: constructing complex objects with many optional fields, enforcing construction invariants, and avoiding the explosion of constructor overloads.

### The Telescoping Constructor Problem

Without a builder, adding optional fields to a class forces you to write a proliferating family of constructors:

```java
// Java: telescoping constructor anti-pattern
new Person("Alice")
new Person("Alice", 30)
new Person("Alice", 30, "admin")
new Person("Alice", 30, "admin", 80000)
new Person("Alice", 30, "admin", 80000, "engineering")
```

With each optional field, the constructor count doubles. For ten optional fields you need up to 1024 constructors, most of which are never used. The programmer cannot remember which overload does what, and two fields of the same type are silently swapped by position.

### The Builder Solution

The Builder pattern replaces positional constructors with a fluent method-chain idiom:

```java
// Java: fluent builder
Person person = Person.builder()
    .name("Alice")
    .age(30)
    .role("admin")
    .salary(80000)
    .department("engineering")
    .build();
```

The `.method().method().method().build()` chain is the canonical form. Each method sets one field and returns `this`, enabling chaining. The terminal `.build()` performs validation and constructs the final object.

### Why Builders Exist: Three Motivations

**1. Named, optional fields.** The StringBuilder, SQL query builder, URL builder, and HTTP request builder all share this trait: many fields with sensible defaults, and the caller only sets what they care about.

```java
// HTTP request builder (OkHttp-style)
Request request = new Request.Builder()
    .url("https://api.example.com/orders")
    .method("POST", body)
    .addHeader("Authorization", "Bearer " + token)
    .addHeader("Content-Type", "application/json")
    .build();

// URL builder
URI uri = new URIBuilder()
    .setScheme("https")
    .setHost("api.example.com")
    .setPath("/search")
    .addParameter("q", "constraint programming")
    .addParameter("limit", "10")
    .build();

// SQL query builder (jOOQ-style)
Result result = dsl
    .select(PERSON.NAME, PERSON.SALARY)
    .from(PERSON)
    .where(PERSON.ROLE.eq("admin"))
    .and(PERSON.SALARY.greaterThan(60000))
    .orderBy(PERSON.NAME)
    .fetch();
```

**2. Immutability.** The builder is mutable; the built object is immutable. The builder accumulates state; `.build()` crystallizes it into a final, thread-safe value.

**3. Validation at build time.** Rather than letting invalid objects escape construction, the builder can check invariants at `.build()`:

```java
Person build() {
    if (name == null) throw new IllegalStateException("name is required");
    if (age < 0 || age > 150) throw new IllegalStateException("invalid age");
    if (salary < 0) throw new IllegalStateException("salary must be non-negative");
    return new Person(name, age, role, salary, department);
}
```

---

## 2. Limitations of the Traditional Builder

The builder pattern is powerful for its intended use case, but it has deep structural limitations.

### Sequential and Ordered

Builder methods are called in sequence. The chain `a.name("Alice").age(30).role("admin")` sets name, then age, then role. The ordering is irrelevant for simple setters, but the *sequential model* constrains what builders can express.

Consider a configuration where the valid set of options depends on earlier choices. A builder for this must make choices in the right order, and downstream options must account for what has been set upstream. There is no mechanism to say "validate all fields together, holistically."

### Fields Are Independent

Each setter method touches exactly one field. The builder has no way to express that setting field A constrains the valid values for field B. The connection between fields must be deferred to `.build()`, where it is expressed as an imperative conditional:

```java
Person build() {
    if ("admin".equals(role) && salary < 100000) {
        throw new IllegalStateException("admins must earn at least 100k");
    }
    // ... more checks
}
```

This is an afterthought. The relationship between `role` and `salary` is invisible at call sites — there is no help for the caller at the moment they set the salary.

### No Cross-Field Propagation

If you set the role to "senior_engineer", nothing automatically propagates. The builder does not derive that `requiresCodeReview = true`, or that the valid salary range is 90000–150000, or that a mentor must be assigned. These derived facts must be manually set by the caller, who has to know the implications.

### Invalid Objects Can Escape

If the caller forgets to call `.build()`, or if `.build()` validation is incomplete, an invalid object can escape. Some builders throw at build time; others silently build objects with null fields. There is no structural guarantee that a "built" object satisfies all invariants.

### Builders Don't Express Relationships

The fundamental limitation: **a builder is a setter-accumulator, not a constraint system**. It cannot express:
- "salary must be at least 80000 given this role"
- "if location is 'remote', equipment_allowance must be set"
- "these three fields together define a valid payment method"

These relationships exist in the domain but must be encoded as imperative checks scattered through `.build()` or lost entirely.

---

## 3. How Constraint Programming Already IS a Builder

In a constraint programming system, you don't set fields sequentially — you *assert facts* about an object incrementally, and the solver maintains consistency across all of them. This is exactly what a builder is trying to do, but with a more powerful underlying model.

### The Correspondence

```
-- OOP builder:
Person.new
    .name("Alice")
    .age(30)
    .role("admin")
    .salary(80000)
    .build()

-- Constraint builder (Evident):
person ∈ Person
person.name = "Alice"
person.age = 30
person.role = "admin"
-- solver derives: person.salary based on role+seniority constraints
```

Each assertion in the constraint version corresponds to one method call in the builder version. But the constraint version has no `.build()` call — the object *is* whatever satisfies all the asserted constraints. The solver finds a concrete value.

### The Solver IS the Builder

In a constraint system, you describe what you want; the solver constructs a witness. The "build" step is not a method call — it is the solver finding an element of the intersection of all the constraint sets you have defined.

In Evident's model:

```evident
-- Declare that person is a variable ranging over Person
person ∈ Person

-- Assert facts
person.name = "Alice"
person.age = 30
person.role = "admin"

-- Query: find a person satisfying all constraints
? person
```

The solver finds a `Person` value where name is "Alice", age is 30, role is "admin", and all other constraints on `Person` values hold. If there are rules connecting `role` to `salary`, those are automatically applied. The "builder" is the constraint accumulation; the "build" is the query.

---

## 4. The Power of Constraint-Based Construction

Constraint builders have capabilities that OOP builders fundamentally cannot achieve.

### Interdependent Fields

In an OOP builder, setting `role = "admin"` does not affect what values are valid for `salary`. In a constraint builder, role constraints can propagate to salary:

```evident
claim salary_appropriate : Person → Prop

evident salary_appropriate p when p.role = "admin"
    p.salary ≥ 100000

evident salary_appropriate p when p.role = "engineer"
    p.salary ≥ 70000
    p.salary ≤ 200000

evident salary_appropriate p when p.role = "intern"
    p.salary ≥ 0
    p.salary ≤ 40000
```

When you assert `person.role = "admin"`, the solver immediately knows that `person.salary ≥ 100000` must hold. If you then assert `person.salary = 50000`, the solver reports an inconsistency before you ever "build" anything. There is no deferred `.build()` check — the constraint is live from the moment it is posted.

### Partial Specification

An OOP builder must have all required fields set before `.build()` succeeds. A constraint builder can leave fields unspecified and let the solver fill them in:

```evident
person ∈ Person
person.name = "Alice"
person.role = "admin"
-- salary not specified; solver picks any value ≥ 100000

? person
-- returns: Person { name = "Alice", role = "admin", salary = 100000 }
--          (or any other salary ≥ 100000)
```

If you want a specific salary, you specify it. If you don't care, the solver picks a valid one. This partial specification is impossible in an OOP builder — a field is either set or it causes a build-time error.

### Guaranteed Validity

If the solver succeeds (finds a witness), the object is guaranteed to satisfy every constraint. There is no gap between "the builder produced an object" and "the object is valid" — they are the same thing. An object returned by the solver is valid by construction.

If the constraints are inconsistent (you asserted both `person.salary = 50000` and `person.role = "admin"` with the rule `admin => salary ≥ 100000`), the solver returns `unsatisfiable`. There is no "build" that produces an invalid object.

### Bidirectionality

OOP builders are directional: you provide field values and get an object. Constraint builders support bidirectional queries. You can ask:

```evident
-- Forward: what salary range is valid for this role?
? salary_appropriate { role = "admin", salary = ?s }
-- s ≥ 100000

-- Backward: what roles are valid for this salary?
person ∈ Person
person.salary = 85000
? person.role
-- role ∈ { "engineer", "senior_engineer", ... }
-- (any role with a salary range covering 85000)

-- Find: is there a valid person with these constraints?
? ∃ p ∈ Person : p.salary = 85000 ∧ p.role = "admin"
-- Not evident (admin requires salary ≥ 100000)
```

An OOP builder cannot answer "what roles are valid for this salary?" without the caller explicitly inverting the logic. Constraint systems support this naturally.

---

## 5. Forward Implication as a Cascade Builder

OOP builders require the caller to manually set every derived field. If setting one field should logically trigger others, the caller must know this and set them manually.

Forward implication (`⇒`) in Evident provides automatic propagation: when one fact is established, derived facts are immediately established as well. This is a cascade builder where asserting one property triggers all its implications.

### Role-Based Cascade

```evident
-- Asserting role = "senior_engineer" cascades to derived facts
person.role = "senior_engineer"
    ⇒ requires_code_review person
    ⇒ requires_documentation person
    ⇒ person.salary ≥ 100000
    ⇒ mentor_assigned person

-- Asserting role = "intern" cascades differently
person.role = "intern"
    ⇒ person.salary ≤ 40000
    ⇒ requires_supervision person
    ⇒ ¬can_deploy_to_production person
```

Once you assert `person.role = "senior_engineer"`, all of these facts become established automatically. The caller never has to know that senior engineers require code review, or that they need documentation, or that a mentor should be assigned. The cascade happens by implication propagation.

This is like a builder where setting `.role("senior_engineer")` internally calls `.requiresCodeReview(true)`, `.requiresDocumentation(true)`, `.minSalary(100000)`, and `.assignMentor(true)` — but you don't write that code in the caller, and you don't write it in the builder's `role()` method either. You write the implication rules once, in one place, and they apply everywhere.

### Existing Systems That Express Cascading Constraints

Several systems have explored this kind of cascading:

**Datalog and Deductive Databases.** Rules in Datalog fire whenever their premises are satisfied. `inferred_admin(X) :- user(X), group_member(X, "admins").` fires for every user in the admins group. As new users are added, the rule fires and adds them to `inferred_admin`. The cascade is live.

**CHR (Constraint Handling Rules).** CHR propagation rules (`==>`) fire when their head pattern matches the constraint store and add new constraints without removing old ones. This is the formalism closest to Evident's forward implication.

**Business rules engines (Drools, RETE).** Rules in RETE-based systems fire when their condition patterns match the working memory. Setting one fact can trigger a cascade of rules that fire in sequence. Used extensively for insurance underwriting rules, tax calculation, access control.

**Spreadsheet formulas.** Changing one cell updates all dependent cells. This is forward propagation over a dependency graph — the simplest constraint builder most people have used.

---

## 6. The `∃` Binding as "Construct an Instance"

The traditional builder idiom is trying to express something very specific: "construct an instance of type T satisfying these conditions." The existential quantifier says exactly this.

In classical logic: `∃ x ∈ T : P(x)` — "there exists an x of type T satisfying P." In constructive logic (and constraint programming), establishing this claim produces a *witness* — a concrete x that satisfies P.

### The Existential Builder

```evident
-- OOP builder idiom:
Config config = Config.builder()
    .host("api.example.com")
    .port(443)
    .useTLS(true)
    .build();

-- Evident existential builder:
? ∃ config ∈ ValidConfig :
    config.host = "api.example.com"
    config.port = 443
    config.use_tls = true
```

The query asks: does there exist a `ValidConfig` with these properties? The solver finds one if it can. The result is a concrete `ValidConfig` instance — the witness.

The difference is deep. The OOP builder constructs whatever you tell it to, then validates. The existential builder asks the solver to *find* a value in the intersection of all the constraints, including all the invariants baked into `ValidConfig`. If you specified an inconsistent combination, there is no witness — the query has no answer.

### Partial Specification as Natural Idiom

```evident
-- "Build me a valid network config for this host; fill in the rest"
? ∃ config ∈ ValidNetworkConfig :
    config.host = "192.168.1.100"
    config.protocol = TCP

-- Solver fills in:
-- config.port = (some valid TCP port)
-- config.timeout = (some valid timeout)
-- config.retry_limit = (some valid retry count)
-- ... all satisfying ValidNetworkConfig's constraints
```

This is closer to what builders are *actually trying to do* than the sequential method-chain. The caller specifies what they know and care about; the system fills in the rest while guaranteeing validity.

---

## 7. Existing Constraint-Based Object Construction Systems

Several existing systems already embody (aspects of) the constraint builder model.

### Alloy's Instance Finding

Alloy is a relational modeling language where you specify a signature (a type's structural invariants) and then ask the Alloy Analyzer to find an *instance* — a concrete model satisfying the specification.

```alloy
sig Person {
    name: one String,
    role: one Role,
    salary: one Int,
    reports_to: lone Person
}

fact SalaryByRole {
    all p: Person | p.role = Admin implies p.salary >= 100
    all p: Person | p.role = Engineer implies p.salary >= 70
}

run {} for 3 Person
```

The `run` command asks Alloy to find any instance with up to 3 Persons satisfying the fact constraints. The Alloy Analyzer uses a SAT solver (Kodkod) under the hood. This is exactly constraint-based object construction.

### Z3 for Value Synthesis

Z3 is Microsoft's SMT solver, designed for verification, but also usable for synthesis. Given a constraint over variables of given sorts, Z3 finds values satisfying the constraints:

```python
from z3 import *

salary = Int('salary')
role = String('role')

s = Solver()
s.add(Implies(role == "admin", salary >= 100000))
s.add(role == "admin")

s.check()  # sat
m = s.model()  # { salary: 100000, role: "admin" }
```

Z3 is finding a model — a concrete assignment of values to variables that satisfies all constraints. This is constraint-based construction for primitive values.

### Rosette's Angelic Nondeterminism

Rosette (a solver-aided programming language hosted in Racket) provides `solve` and `synthesize` operations that let the solver fill in "holes" in programs:

```racket
(define (make-config host port)
  (solve (assert (valid-config? (config host port ??tls ??timeout)))))
```

The `??` holes are filled by the solver to make the assertion hold. Rosette calls this *angelic nondeterminism* — the solver "divines" values that make execution succeed. The programmer specifies the validity condition; the solver constructs the witness.

This is the exact builder-by-constraint model: you write what a valid config looks like, and the solver finds one.

### MiniZinc's Solution Finding

MiniZinc is a high-level constraint modeling language designed specifically for this workflow:

```minizinc
include "alldifferent.mzn";

var 0..150000: salary;
var {"admin", "engineer", "intern"}: role;

constraint role = "admin" -> salary >= 100000;
constraint role = "engineer" -> salary >= 70000;
constraint role = "intern" -> salary <= 40000;

solve satisfy;
```

MiniZinc separates the model (what must be true) from the solver (how to find it). The `solve satisfy` directive asks for any solution. Add `solve minimize salary` for the minimum-salary solution. The programmer writes constraints; the solver constructs the object.

### Prolog's Query Answer Construction

Prolog's query mechanism finds bindings for uninstantiated variables:

```prolog
:- use_module(library(clpfd)).

valid_person(Name, Role, Salary) :-
    member(Role, [admin, engineer, intern]),
    (Role = admin    -> Salary #>= 100000 ; true),
    (Role = engineer -> Salary #>= 70000  ; true),
    (Role = intern   -> Salary #=< 40000  ; true).

?- valid_person("Alice", admin, Salary).
%  Salary in 100000..sup
```

The query `valid_person("Alice", admin, Salary)` asks: for what values of `Salary` is `valid_person("Alice", admin, Salary)` provable? Prolog finds the answer by constraint propagation through CLP(FD). This is constraint-based construction for the fields of an implicit "person" record.

---

## 8. What a Constraint Builder Syntax Might Look Like in Evident

Evident's constraint model suggests several syntactic idioms for "building" complex objects.

### Progressive Constraint Narrowing

Each assertion narrows the set of valid objects. The object is whatever survives all the narrowing:

```evident
-- Progressive construction: each line adds a constraint
person ∈ Person
person.name = "Alice"          -- narrow to Persons named Alice
person.age = 30                -- narrow to those aged 30
person.role = "senior_engineer"  -- narrow further; triggers cascade:
                               --   person.salary >= 100000 (by implication)
                               --   requires_code_review person (by implication)

? person   -- find a concrete person satisfying all constraints
```

No `.build()`. The query `? person` is the "build" — it asks the solver to find a concrete element of the intersection of all the asserted constraint sets.

### Named Builders as Claims

A "named builder" is just a claim that bundles common construction constraints. Instead of building a fluent API class, you define a claim:

```evident
-- "Builder" for a valid admin user
claim admin_user : Person → Prop

evident admin_user p
    p.role = "admin"
    p.salary ≥ 100000
    p.permissions ⊇ { read, write, delete, admin }
    p.access_level = "full"

-- Use it
person ∈ Person
person.name = "Alice"
admin_user person          -- applies all "admin_user" constraints at once

? person
-- person.salary ≥ 100000, person.permissions ⊇ {...}, etc.
```

The `admin_user` claim IS the builder. Asserting it applies all its conditions to `person`. You never have to remember which individual constraints admin users need — you just apply the claim.

### Implication Chains as Automatic Cascades

Forward implications let one assertion trigger a cascade of derived constraints:

```evident
-- One assertion cascades to many
person.role = "senior_engineer"
    ⇒ person.salary ≥ 100000
    ⇒ requires_code_review person
    ⇒ requires_documentation person
    ⇒ person.on_call_rotation = true

person.location = "remote"
    ⇒ person.equipment_stipend ≥ 1500
    ⇒ person.home_office_allowance ≥ 500
```

### The Final Build is Implicit

There is no `.build()`. The object IS whatever satisfies all constraints. The query `? person` finds a witness:

```evident
-- The whole "construction" in one place
person ∈ Person
person.name = "Alice"
person.role = "senior_engineer"   -- cascade fires
person.location = "remote"        -- cascade fires

? person
-- Returns one concrete Person satisfying all constraints:
-- { name = "Alice", role = "senior_engineer", salary = 100000,
--   location = "remote", equipment_stipend = 1500,
--   home_office_allowance = 500, on_call_rotation = true, ... }
```

---

## 9. Use Cases Where Constraint Builders Beat OOP Builders

### Configuration Objects with Complex Interdependencies

Network configurations, database connection pools, and service meshes have configurations where options are deeply interdependent. A builder for a TLS configuration has dozens of options that must be consistent: the cipher suites must match the TLS version, the certificate must match the hostname, the timeout must exceed the expected round-trip time.

An OOP builder defers all consistency checking to `.build()`, which must contain a combinatorial explosion of cross-field checks. A constraint builder expresses these as named constraints that fire at assertion time and make inconsistency immediately visible.

### Scheduling Objects (Start Time Depends on Dependencies)

A task's start time depends on its dependencies' end times, which depend on resource availability, which depends on other tasks' assignments. No OOP builder can express this — you must set all fields explicitly, and you must do the scheduling calculation yourself before calling the setter.

A constraint builder lets you assert the dependencies and the resource constraints, and the solver finds valid start times. You never write a scheduling algorithm. The constraints define what a valid schedule is; the solver finds one.

### Network Configuration

Firewall rules, routing tables, and load balancer configurations have complex mutual constraints: a rule allowing traffic on port 443 must correspond to a service listening on 443, the service's health check interval must be less than the rule's timeout, etc.

A constraint builder for network configuration would express these relationships directly. Asserting a service configuration would automatically constrain the firewall rules to be consistent with it.

### Valid Game States

A chess game state has many invariants: pieces cannot overlap, the board has fixed dimensions, piece counts are bounded, certain combinations of pieces imply certain things about what moves are legal. Constructing a valid arbitrary game state for testing with an OOP builder requires calculating all these invariants manually. A constraint builder generates valid states automatically.

---

## 10. Worked Examples in Evident Syntax

### Example A: Employee Record Construction

The classic builder example, as a constraint model:

```evident
type Department = Engineering | Product | Design | Operations | Legal

type Role = Intern | Junior | MidLevel | Senior | Principal | Director | VP

type Employee = {
    name        ∈ String
    role        ∈ Role
    department  ∈ Department
    salary      ∈ Nat
    equity_pct  ∈ Real
    reports_to  ∈ Maybe String
    manages     ∈ Set String
    remote      ∈ Bool
    stipend     ∈ Nat
}

-- Role-to-compensation constraints (the "cascade builder" rules)
claim compensation_valid : Employee → Prop

evident compensation_valid e when e.role = Intern
    e.salary ≥ 20000
    e.salary ≤ 45000
    e.equity_pct = 0.0

evident compensation_valid e when e.role = Junior
    e.salary ≥ 70000
    e.salary ≤ 120000
    e.equity_pct ≥ 0.01

evident compensation_valid e when e.role ∈ [Senior, Principal]
    e.salary ≥ 130000
    e.equity_pct ≥ 0.1

evident compensation_valid e when e.role ∈ [Director, VP]
    e.salary ≥ 200000
    e.equity_pct ≥ 0.5
    e.manages ≠ {}      -- must manage someone

-- Remote work stipend constraint
e.remote = true ⇒ e.stipend ≥ 1500
e.remote = false ⇒ e.stipend = 0

-- Non-intern engineers must have a reports_to
e.role ≠ Intern, e.department = Engineering ⇒ e.reports_to is_some

-- Building an employee by constraint
emp ∈ Employee
emp.name = "Alice"
emp.role = Senior
emp.department = Engineering
emp.remote = true
compensation_valid emp     -- applies role-based salary/equity constraints

? emp
-- Returns: Employee {
--   name = "Alice", role = Senior, department = Engineering,
--   salary = 130000,   (≥ 130000, solver picks minimum valid)
--   equity_pct = 0.1,  (≥ 0.1)
--   remote = true,
--   stipend = 1500,    (derived by implication)
--   reports_to = ...,  (must be set, solver will leave as unbound or error)
--   manages = {}       (not required for Senior)
-- }
```

Notice that `stipend = 1500` is derived automatically — the caller never sets it. The rule `e.remote = true ⇒ e.stipend ≥ 1500` fires as soon as `emp.remote = true` is asserted. An OOP builder could compute this in the `remote()` method, but that would silently override whatever the caller set for `stipend`, and there would be no way to query "what stipend is valid for a remote employee?" The constraint is live and bidirectional.

---

### Example B: HTTP Request Builder

```evident
type AuthScheme = Bearer | ApiKey | None

type HttpConfig = {
    url          ∈ String
    method       ∈ HttpMethod
    auth_scheme  ∈ AuthScheme
    auth_token   ∈ Maybe String
    content_type ∈ Maybe String
    body         ∈ Maybe String
    timeout_ms   ∈ Nat
    retry_limit  ∈ Nat
}

-- Auth consistency constraints
claim auth_consistent : HttpConfig → Prop

evident auth_consistent cfg when cfg.auth_scheme = Bearer
    cfg.auth_token is_some
    cfg.auth_token starts_with "eyJ"   -- JWT prefix

evident auth_consistent cfg when cfg.auth_scheme = ApiKey
    cfg.auth_token is_some
    cfg.auth_token.length ≥ 32

evident auth_consistent cfg when cfg.auth_scheme = None
    -- No token needed

-- Body-method consistency constraints
cfg.method ∈ [POST, PUT, PATCH] ⇒ cfg.body is_some
cfg.method ∈ [POST, PUT, PATCH] ⇒ cfg.content_type is_some
cfg.method ∈ [GET, DELETE]      ⇒ cfg.body = None

-- Timeout/retry sensible defaults via implication
cfg.timeout_ms = 0  ⇒ cfg.timeout_ms = 30000   -- 0 is not valid; enforce default
cfg.retry_limit > 10 ⇒ False                    -- disallow pathological retry counts

-- Build a POST request
req ∈ HttpConfig
req.url = "https://api.example.com/orders"
req.method = POST
req.auth_scheme = Bearer
req.auth_token = Some "eyJhbGciOiJSUzI1NiJ9..."
req.body = Some "{ \"item\": \"widget\" }"
req.timeout_ms = 5000
auth_consistent req

? req
-- Returns: HttpConfig {
--   url = "https://api.example.com/orders"
--   method = POST
--   auth_scheme = Bearer
--   auth_token = Some "eyJhbGci..."
--   content_type = ?  ← solver requires this for POST; must be specified or constrained
--   body = Some "{ \"item\": \"widget\" }"
--   timeout_ms = 5000
--   retry_limit = ?   ← unspecified; solver picks any valid value (0..10)
-- }
```

Contrast with an OOP builder: if you call `.method("POST")` and forget to set `.body()`, the builder won't tell you until `.build()`. In the constraint version, `cfg.method = POST` immediately asserts `cfg.body is_some` and `cfg.content_type is_some` as live constraints. If you then query `? req` without having set those fields, the solver either fills them in (if it can pick any valid value) or reports them as unconstrained — a signal to specify them, not a deferred crash.

---

### Example C: Server Infrastructure Configuration

This example shows the constraint builder's advantage for deeply interdependent configuration:

```evident
type LoadBalancer = {
    algorithm       ∈ Algorithm
    health_check_ms ∈ Nat
    timeout_ms      ∈ Nat
    max_connections ∈ Nat
}

type ServiceConfig = {
    name              ∈ String
    port              ∈ Nat
    replicas          ∈ Nat
    cpu_millicores    ∈ Nat
    memory_mb         ∈ Nat
    load_balancer     ∈ LoadBalancer
    min_ready_seconds ∈ Nat
    max_surge_pct     ∈ Real
}

-- A valid service configuration has these cross-field constraints:
claim valid_service : ServiceConfig → Prop

evident valid_service svc
    svc.port ≥ 1024
    svc.port ≤ 65535
    svc.replicas ≥ 1
    svc.cpu_millicores ≥ 100
    svc.memory_mb ≥ 64
    -- LB health check must complete within the service's timeout budget
    svc.load_balancer.health_check_ms < svc.load_balancer.timeout_ms
    -- With multiple replicas, surge must stay reasonable
    svc.replicas > 1 ⇒ svc.max_surge_pct ≤ 50.0
    -- High-traffic services need larger connection pools
    svc.replicas ≥ 10 ⇒ svc.load_balancer.max_connections ≥ 1000

-- Forward implications for standard tier configurations
svc.replicas ≤ 3
    ⇒ svc.load_balancer.max_connections ≥ 100
svc.memory_mb ≥ 4096
    ⇒ svc.cpu_millicores ≥ 1000   -- memory-heavy services need proportional CPU

-- Build a production API service
api ∈ ServiceConfig
api.name = "order-service"
api.port = 8080
api.replicas = 5
api.cpu_millicores = 500
api.memory_mb = 1024
api.load_balancer.algorithm = RoundRobin
api.load_balancer.health_check_ms = 200
api.load_balancer.timeout_ms = 5000
valid_service api

? api
-- Solver resolves:
-- api.load_balancer.max_connections ≥ 100 (from replicas ≤ 3? No, replicas = 5)
-- max_surge_pct ≤ 50.0 (from replicas > 1)
-- remaining fields filled to any valid value

-- If we add another constraint:
? ∃ api ∈ ServiceConfig :
    api.name = "order-service"
    api.replicas = 5
    api.memory_mb = 8192    -- 8GB RAM
    valid_service api
-- Solver adds: api.cpu_millicores ≥ 1000 (memory ≥ 4096 implies proportional CPU)
-- and finds a consistent configuration
```

In an OOP builder, setting `memory_mb = 8192` does nothing to `cpu_millicores`. The developer must know (and remember) that high-memory services need proportional CPU, and must set it manually. The constraint captures this domain knowledge once, and it applies everywhere — in all contexts, for all services, regardless of who writes the code.

---

## Summary

The Builder pattern is engineering's answer to a fundamental problem: constructing complex objects with many interdependent fields, ensuring validity, and providing a legible call-site syntax. It works adequately for simple cases but fails at its core task — it cannot express the *relationships* between fields, and validation is deferred and afterthought-shaped.

Constraint programming is the correct substrate for what builders are trying to be. A constraint system expresses field interdependencies as first-class constraints, validates continuously (not just at build time), supports partial specification where unspecified fields are filled in by the solver, and is bidirectional — you can ask "what values are valid for this field given everything else?" rather than just "is this value valid for this field?"

In Evident:
- The `∃` binding replaces `.build()` — "find me a witness satisfying all constraints"
- Forward implication (`⇒`) replaces manual cascade logic in builder methods
- Named claims replace builder classes — a claim IS the bundled construction pattern
- Every constructed object is a witness to a proof, carrying its derivation as a first-class value

The OOP builder pattern exists because programming languages don't have constraint solvers. Evident does.
