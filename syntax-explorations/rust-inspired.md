# Syntax: Rust-Inspired

This syntax frames Evident as a system of functions that produce typed evidence values. A programmer coming from Rust or TypeScript reads `prove fn factorial(n, f)` the same way they read any function: it takes inputs and either returns a value or fails. Evidence types are declared as structs and enums, making derivation trees concrete, inspectable data. The `require!` macro makes sub-claim dependencies feel like error propagation — familiar from Rust's `?` operator. Claim families are declared with `claim`, analogous to a trait, and proof strategies are provided in `impl Evident for` blocks. Because clause ordering is irrelevant, multiple `prove fn` blocks for the same claim read as independent match arms that the runtime may explore in any order.

---

## 1. Factorial

```evident
// Declare the claim family: Factorial(n, f) means "f is the factorial of n"
claim Factorial(n: u64, f: u64);

// Evidence type: a derivation is either the base case or one recursive step
enum FactorialEvidence {
    Base,                              // 0! = 1
    Step {
        n: u64,
        sub: Box<Evidence<Factorial>>, // the derivation of (n-1)!
    },
}

// Base case: 0! = 1 (self-evident, no body needed)
prove fn factorial_base() -> Evidence<Factorial> {
    assert!(Factorial(0, 1));
    FactorialEvidence::Base
}

// Recursive case: n! = n * (n-1)!
prove fn factorial_step(n: u64) -> Result<Evidence<Factorial>, NotEvident> {
    if n == 0 { return Err(NotEvident); } // defer to base case

    let m = n - 1;
    let sub = require!(Factorial(m, ?k))?; // runtime finds k such that Factorial(m, k)
    let f = n * k;

    Ok(Evidence::new(
        Factorial(n, f),
        FactorialEvidence::Step { n, sub: Box::new(sub) },
    ))
}

// Caller side: ask the runtime to find evidence
fn main() {
    let ev = infer!(Factorial(5, ?f)).expect("must be evident");
    println!("5! = {}", ev.claim().f);   // 120

    // Pattern-match the derivation tree
    match ev.data() {
        FactorialEvidence::Base => println!("base case"),
        FactorialEvidence::Step { n, sub } => {
            println!("step at n={}, sub-derivation depth={}", n, sub.depth());
        }
    }
}
```

The `Result<Evidence<T>, NotEvident>` return type makes partial failure explicit and feels natural to any Rust programmer — a proof attempt either succeeds or it doesn't. The `require!(Factorial(m, ?k))?` line, which binds an output variable while propagating failure, is expressive but introduces two new syntactic ideas (`?` output binding and `!`-propagation) simultaneously, which may confuse readers on first contact. Boxing the sub-derivation is the right move semantically but adds noise that a higher-level ergonomic sugar could hide.

---

## 2. Sorted List

```evident
// Declare the claim family
claim Sorted(list: Vec<u64>);

// Evidence is an enum reflecting the three structural cases
enum SortedEvidence {
    Empty,                              // [] is sorted trivially
    Singleton(u64),                     // [x] is sorted trivially
    Cons {                              // [a, b, ..rest] is sorted because:
        head: u64,
        next: u64,
        ordering: u64,                  // witness: head <= next (stored as head)
        tail_proof: Box<Evidence<Sorted>>, // and [b, ..rest] is sorted
    },
}

// Case 1: empty list
prove fn sorted_empty() -> Evidence<Sorted> {
    assert!(Sorted(vec![]));
    Evidence::new(Sorted(vec![]), SortedEvidence::Empty)
}

// Case 2: singleton list
prove fn sorted_singleton(x: u64) -> Evidence<Sorted> {
    assert!(Sorted(vec![x]));
    Evidence::new(Sorted(vec![x]), SortedEvidence::Singleton(x))
}

// Case 3: list with at least two elements
prove fn sorted_cons(list: Vec<u64>) -> Result<Evidence<Sorted>, NotEvident> {
    // Destructure: need at least [a, b, ..rest]
    let [a, b, ref rest @ ..] = list.as_slice() else {
        return Err(NotEvident); // defer to empty/singleton cases
    };

    if a > b {
        return Err(NotEvident); // not sorted at this position
    }

    // Recursively require that the tail is sorted
    let tail: Vec<u64> = std::iter::once(*b).chain(rest.iter().copied()).collect();
    let tail_proof = require!(Sorted(tail))?;

    Ok(Evidence::new(
        Sorted(list.clone()),
        SortedEvidence::Cons {
            head: *a,
            next: *b,
            ordering: *a,
            tail_proof: Box::new(tail_proof),
        },
    ))
}

// Caller side
fn check_sorted(list: Vec<u64>) {
    match infer!(Sorted(list.clone())) {
        Ok(ev) => {
            println!("{:?} is sorted", list);
            // Walk the evidence tree to find the proof depth
            let mut depth = 0;
            let mut cur = &ev;
            while let SortedEvidence::Cons { tail_proof, .. } = cur.data() {
                depth += 1;
                cur = tail_proof;
            }
            println!("proof depth: {}", depth);
        }
        Err(NotEvident) => println!("{:?} is NOT sorted", list),
    }
}
```

The enum cases map cleanly onto the three structural possibilities, and Rust's slice pattern `[a, b, ref rest @ ..]` slots in naturally as a destructuring mechanism. The explicit construction of the `tail` vector inside the proof function is the main awkward point: the programmer is doing work that a native list-structured language would handle with pattern syntax, and it mixes Rust-flavored computation into what is conceptually a pure proof step. A dedicated list-pattern syntax in the `prove fn` signature would be cleaner.

---

## 3. HTTP Request Validation

```evident
// --- Domain types ---
struct Request {
    method: Method,
    auth: AuthHeader,
    content_type: ContentType,
}

enum Method { Get, Post, Put, Delete }
enum ContentType { ApplicationJson, ApplicationFormUrlencoded }

// --- Claim families ---
claim ValidRequest(req: Request);
claim ValidMethod(method: Method);
claim ValidAuth(auth: AuthHeader);
claim ValidContentType(ct: ContentType);

// --- Evidence types ---
struct ValidRequestEvidence {
    method_proof:  Evidence<ValidMethod>,
    auth_proof:    Evidence<ValidAuth>,
    content_proof: Evidence<ValidContentType>,
}

// ValidMethod is self-evident for the four allowed verbs
enum ValidMethodEvidence { Get, Post, Put, Delete }

struct ValidAuthEvidence {
    scheme:    String,          // "Bearer"
    token_id:  String,          // opaque token identifier for audit
    issued_at: u64,
    expires_at: u64,
}

enum ValidContentTypeEvidence { Json, Form }

// --- Proof strategies ---

impl Evident for ValidRequest {
    prove fn valid_request(req: Request) -> Result<Evidence<ValidRequest>, NotEvident> {
        // All three sub-claims must be evident; collect their evidence
        let method_proof  = require!(ValidMethod(req.method))?;
        let auth_proof    = require!(ValidAuth(req.auth))?;
        let content_proof = require!(ValidContentType(req.content_type))?;

        Ok(Evidence::new(
            ValidRequest(req),
            ValidRequestEvidence { method_proof, auth_proof, content_proof },
        ))
    }
}

impl Evident for ValidMethod {
    prove fn valid_method_get()    -> Evidence<ValidMethod> { assert!(ValidMethod(Method::Get));    Evidence::new(ValidMethod(Method::Get),    ValidMethodEvidence::Get)    }
    prove fn valid_method_post()   -> Evidence<ValidMethod> { assert!(ValidMethod(Method::Post));   Evidence::new(ValidMethod(Method::Post),   ValidMethodEvidence::Post)   }
    prove fn valid_method_put()    -> Evidence<ValidMethod> { assert!(ValidMethod(Method::Put));    Evidence::new(ValidMethod(Method::Put),    ValidMethodEvidence::Put)    }
    prove fn valid_method_delete() -> Evidence<ValidMethod> { assert!(ValidMethod(Method::Delete)); Evidence::new(ValidMethod(Method::Delete), ValidMethodEvidence::Delete) }
}

impl Evident for ValidAuth {
    prove fn valid_auth(auth: AuthHeader) -> Result<Evidence<ValidAuth>, NotEvident> {
        // Decompose the auth header: must be Bearer scheme
        let token = match auth.scheme.as_str() {
            "Bearer" => &auth.token,
            _        => return Err(NotEvident),
        };

        // Sub-claims: token must be unexpired and have a valid signature
        let _not_expired   = require!(TokenNotExpired(token))?;
        let _sig_valid     = require!(SignatureValid(token))?;

        Ok(Evidence::new(
            ValidAuth(auth.clone()),
            ValidAuthEvidence {
                scheme:    "Bearer".into(),
                token_id:  token.jti.clone(),
                issued_at: token.iat,
                expires_at: token.exp,
            },
        ))
    }
}

impl Evident for ValidContentType {
    prove fn valid_ct_json() -> Evidence<ValidContentType> {
        assert!(ValidContentType(ContentType::ApplicationJson));
        Evidence::new(ContentType::ApplicationJson, ValidContentTypeEvidence::Json)
    }
    prove fn valid_ct_form() -> Evidence<ValidContentType> {
        assert!(ValidContentType(ContentType::ApplicationFormUrlencoded));
        Evidence::new(ContentType::ApplicationFormUrlencoded, ValidContentTypeEvidence::Form)
    }
}

// --- Caller side: access and audit the evidence tree ---
fn handle(req: Request) {
    match infer!(ValidRequest(req)) {
        Ok(ev) => {
            let data: &ValidRequestEvidence = ev.data();

            // Audit log: pull the auth evidence out of the tree
            let auth_data: &ValidAuthEvidence = data.auth_proof.data();
            println!(
                "request authorized: token={} expires={}",
                auth_data.token_id, auth_data.expires_at
            );

            // The method evidence is also directly accessible
            let method_data = data.method_proof.data();
            println!("method evidence: {:?}", method_data);

            process(req);
        }
        Err(NotEvident) => {
            eprintln!("request rejected: could not establish ValidRequest");
            // In practice, the runtime can surface which sub-claim failed
        }
    }
}
```

The `impl Evident for ClaimType` grouping keeps proof strategies co-located with their claim, and reading `valid_request` reads almost like a typed validation pipeline — require each sub-claim, collect the evidence, wrap it up. The evidence tree on the caller side is genuinely useful: `data.auth_proof.data().token_id` traverses the derivation tree as ordinary field access. The most awkward part is the four one-liner methods for `ValidMethod` — the ceremony of `assert! + Evidence::new` for self-evident ground facts is verbose; an `axiom!` shorthand would help significantly.

---

## 4. Graph Reachability

```evident
// --- Domain types ---
type Node = &'static str;

// --- Claim families ---
claim Edge(from: Node, to: Node);
claim Reachable(from: Node, to: Node);

// --- Evidence types ---
struct EdgeEvidence;   // edges are ground facts; no sub-structure needed

enum ReachableEvidence {
    Direct {
        edge: Evidence<Edge>,           // a single edge from -> to
    },
    Transitive {
        first_leg:  Box<Evidence<Reachable>>,   // from -> mid
        second_leg: Box<Evidence<Reachable>>,   // mid -> to
        mid: Node,
    },
}

// --- Ground facts: assert the graph edges ---
fn load_graph() {
    assert!(Edge("a", "b"));
    assert!(Edge("b", "c"));
    assert!(Edge("c", "d"));
    assert!(Edge("b", "d"));
    assert!(Edge("e", "a"));
}

// Rule 1: direct reachability via a single edge
prove fn reachable_direct(from: Node, to: Node) -> Result<Evidence<Reachable>, NotEvident> {
    let edge_ev = require!(Edge(from, to))?;
    Ok(Evidence::new(
        Reachable(from, to),
        ReachableEvidence::Direct { edge: edge_ev },
    ))
}

// Rule 2: transitive reachability via an intermediate node
// The runtime instantiates `mid` by searching over known nodes
prove fn reachable_transitive(from: Node, to: Node, mid: Node) -> Result<Evidence<Reachable>, NotEvident> {
    let first_leg  = require!(Reachable(from, mid))?;
    let second_leg = require!(Reachable(mid, to))?;
    Ok(Evidence::new(
        Reachable(from, to),
        ReachableEvidence::Transitive {
            first_leg:  Box::new(first_leg),
            second_leg: Box::new(second_leg),
            mid,
        },
    ))
}

// Caller side: derive and inspect a reachability proof
fn main() {
    load_graph();

    match infer!(Reachable("e", "d")) {
        Ok(ev) => {
            println!("e can reach d");
            print_proof(&ev, 0);
        }
        Err(NotEvident) => println!("e cannot reach d"),
    }
}

fn print_proof(ev: &Evidence<Reachable>, indent: usize) {
    let pad = " ".repeat(indent * 2);
    match ev.data() {
        ReachableEvidence::Direct { edge } => {
            let e = edge.claim();
            println!("{}direct: {} -> {}", pad, e.from, e.to);
        }
        ReachableEvidence::Transitive { first_leg, second_leg, mid } => {
            println!("{}via {}:", pad, mid);
            print_proof(first_leg, indent + 1);
            print_proof(second_leg, indent + 1);
        }
    }
}
```

The two `prove fn` rules for `Reachable` translate almost directly from the logical reading — "direct" and "via an intermediate" — and the runtime's job of instantiating `mid` by searching over known nodes is the right place to hide the combinatorial work. The `print_proof` function demonstrates how evidence-as-data pays off: the derivation tree is structurally recursive and can be walked, printed, serialized, or diffed without any special runtime support. The main design question left open is how the runtime bounds the search for `mid` — some declaration of the domain (e.g., `search over: Node`) would make this explicit rather than implicit.

---

## 5. FizzBuzz

```evident
// Declare the claim family
claim FizzBuzz(n: u32, result: String);

// Evidence is an enum with one variant per case
enum FizzBuzzEvidence {
    FizzBuzz,              // divisible by both 3 and 5
    Fizz,                  // divisible by 3 only
    Buzz,                  // divisible by 5 only
    Number(u32),           // neither; result is n itself
}

// Case 1: FizzBuzz (divisible by both)
prove fn fizzbuzz_both(n: u32) -> Result<Evidence<FizzBuzz>, NotEvident> {
    if n % 3 != 0 || n % 5 != 0 { return Err(NotEvident); }
    Ok(Evidence::new(
        FizzBuzz(n, "FizzBuzz".into()),
        FizzBuzzEvidence::FizzBuzz,
    ))
}

// Case 2: Fizz (divisible by 3, not 5)
prove fn fizzbuzz_fizz(n: u32) -> Result<Evidence<FizzBuzz>, NotEvident> {
    if n % 3 != 0 || n % 5 == 0 { return Err(NotEvident); }
    Ok(Evidence::new(
        FizzBuzz(n, "Fizz".into()),
        FizzBuzzEvidence::Fizz,
    ))
}

// Case 3: Buzz (divisible by 5, not 3)
prove fn fizzbuzz_buzz(n: u32) -> Result<Evidence<FizzBuzz>, NotEvident> {
    if n % 5 != 0 || n % 3 == 0 { return Err(NotEvident); }
    Ok(Evidence::new(
        FizzBuzz(n, "Buzz".into()),
        FizzBuzzEvidence::Buzz,
    ))
}

// Case 4: plain number (neither 3 nor 5)
prove fn fizzbuzz_number(n: u32) -> Result<Evidence<FizzBuzz>, NotEvident> {
    if n % 3 == 0 || n % 5 == 0 { return Err(NotEvident); }
    Ok(Evidence::new(
        FizzBuzz(n, n.to_string()),
        FizzBuzzEvidence::Number(n),
    ))
}

// Caller side: run FizzBuzz for 1..=20 and inspect evidence
fn main() {
    for n in 1u32..=20 {
        let ev = infer!(FizzBuzz(n, ?result)).expect("FizzBuzz is total");
        let label = match ev.data() {
            FizzBuzzEvidence::FizzBuzz  => "fizzbuzz case",
            FizzBuzzEvidence::Fizz      => "fizz case",
            FizzBuzzEvidence::Buzz      => "buzz case",
            FizzBuzzEvidence::Number(x) => "plain number",
        };
        println!("{}: {} ({})", n, ev.claim().result, label);
    }
}
```

The four `prove fn` blocks make the case partition explicit and locally verifiable — each guard condition is self-contained, and the mutual exclusivity of the four cases can be checked by inspection. This mirrors exactly how a careful Rust programmer would write a total match: cover every case, make overlaps impossible by construction. The verbosity is real, though: each case requires an `if`-guard, an `Evidence::new` call, and an enum variant, whereas a dedicated `match`-style syntax inside `prove fn` bodies (similar to how Haskell or Agda handle case analysis) would be more concise. The `infer!(FizzBuzz(n, ?result))` pattern — binding an output variable — shows how the Evident runtime acts as a search oracle even for a deterministic claim.

---

## Overall Assessment

The Rust-inspired syntax succeeds at making evidence first-class in a way that feels idiomatic rather than bolted on. A programmer who has used `Result<T, E>` for error propagation immediately understands `Result<Evidence<T>, NotEvident>` — the mental model transfers directly. Struct and enum evidence types give derivation trees a concrete schema, which means they can be serialized, logged, diffed, and pattern-matched without any special runtime machinery. The `impl Evident for ClaimType` grouping organizes proof strategies in a way that scales to large programs.

The friction points are mostly syntactic ceremony: the `assert! + Evidence::new` pair required for every ground fact is repetitive, and the `require!(Claim(?var))?` output-binding syntax conflates two ideas (search and propagation) in a single expression. The deeper tension is that Rust's type system is built around uniqueness and ownership, while Evident's derivation model is inherently non-deterministic and potentially involves sharing — so some Rust idioms (like `Box` for recursive types) fit well, while others (like borrowing semantics) would need to be either relaxed or rethought. Overall, this syntax is the most immediately legible of any logic-programming surface syntax for developers coming from mainstream typed languages, at the cost of some verbosity and a few conceptual seams where the two paradigms meet.
