# Syntax: Prolog-Adjacent

This syntax borrows Prolog's foundational notation — `:-` for rule bodies, uppercase variables, lowercase atoms, `.` terminators, and `%` comments — but reframes the semantics around Evident's core concept: a claim is *evident* when a derivation exists for it. The keyword `evident` replaces predicate definitions, making the epistemic intent explicit. Unlike Prolog, clause ordering carries no operational meaning; rules for the same claim are unordered alternatives, and the runtime may explore them in any sequence. Evidence terms are first-class values, accessible via `@evidence` within rule bodies, allowing programs to inspect and propagate their own derivation trees.

---

## 1. Factorial

```evident
% factorial(N, F): F is the factorial of N
% clauses are unordered

% Base case: 0! = 1
evident factorial(0, 1).

% Recursive case: N! = N * (N-1)!
evident factorial(N, F) :-
    N > 0,
    M is N - 1,
    factorial(M, K),
    F is N * K.
```

The base case reads naturally as a self-evident ground fact. The recursive rule mirrors Prolog almost exactly, which is both its strength and its limitation: the arithmetic predicates (`is`, `>`) are inherited conventions that feel borrowed rather than native to Evident's claim-based worldview. The `evident` keyword before every clause reinforces that each rule is an independent derivation path, not a procedural step.

---

## 2. Sorted List

```evident
% sorted(List): List is in non-decreasing order
% clauses are unordered

% An empty list is trivially sorted.
evident sorted([]).

% A single-element list is trivially sorted.
evident sorted([_]).

% A list [A, B | Rest] is sorted when A =< B and [B | Rest] is sorted.
evident sorted([A, B | Rest]) :-
    A =< B,
    sorted([B | Rest]).
```

The pattern-matching decomposition on list structure is clean and expressive here — arguably cleaner than an imperative loop. The three clauses cover all structural cases exhaustively, and the unordered-clause model means the runtime can dispatch on list shape without any ordering commitment. The one awkward point: `=<` for less-than-or-equal is a Prolog convention that looks typographically odd; a native Evident syntax might prefer `<=` or `leq`.

---

## 3. HTTP Request Validation

```evident
% valid_api_request(Req): Req passes all validation checks
% clauses are unordered

% A request is valid when its method, auth token, and content type are all valid.
evident valid_api_request(Req) :-
    valid_method(Req),
    valid_auth(Req),
    valid_content_type(Req).

% --- Method validation ---
% clauses are unordered

evident valid_method(Req) :- Req.method = get.
evident valid_method(Req) :- Req.method = post.
evident valid_method(Req) :- Req.method = put.
evident valid_method(Req) :- Req.method = delete.

% --- Auth token validation ---
% clauses are unordered

% An auth token is valid when it uses bearer scheme, is not expired,
% and has a valid signature. The evidence term is retained for audit.
evident valid_auth(Req) :-
    bearer_scheme(Req.auth, Token),
    not_expired(Token),
    signature_valid(Token),
    @evidence = auth_evidence(Req.auth, Token).

% bearer_scheme: extracts the token from a "Bearer <token>" header.
evident bearer_scheme(Header, Token) :-
    Header = bearer(Token).

% not_expired: a token is not expired if its expiry is after now.
evident not_expired(Token) :-
    Token.expiry > now.

% signature_valid: token signature matches the shared secret.
evident signature_valid(Token) :-
    hmac(Token.payload, secret_key, Token.signature).

% --- Content type validation ---
% clauses are unordered

evident valid_content_type(Req) :- Req.content_type = application_json.
evident valid_content_type(Req) :- Req.content_type = application_form_urlencoded.

% --- What the evidence tree looks like on failure ---
%
% If valid_auth(Req) fails because the token is expired, the failed
% derivation tree would be inspectable as:
%
%   failure {
%     claim: valid_api_request(req_42),
%     because: valid_auth(req_42),
%     because: not_expired(token_xyz),
%     reason: token_xyz.expiry (1700000000) <= now (1745539200)
%   }
%
% The @evidence binding in valid_auth captures auth_evidence(...) on
% success, so callers can log or forward the derivation without
% re-deriving it.
```

The multi-clause method check is elegant: each HTTP verb is its own self-evident alternative, and the runtime picks whichever matches. The `@evidence` binding for audit trails is a natural fit for security-sensitive code — being able to pass the derivation proof downstream is a genuine advantage over Prolog. The dot-access syntax (`Req.method`, `Token.expiry`) for structured data is assumed here but would need to be defined by the language; Prolog's functor/arity approach would be more verbose.

---

## 4. Graph Reachability

```evident
% reachable(A, B): node B is reachable from node A via directed edges
% clauses are unordered

% Asserted edges (ground facts):
evident edge(a, b).
evident edge(b, c).
evident edge(c, d).
evident edge(b, d).
evident edge(e, a).

% Direct reachability: if there is an edge from A to B, B is reachable from A.
evident reachable(A, B) :-
    edge(A, B).

% Transitive reachability: B is reachable from A if some intermediate
% node M is reachable from A and B is reachable from M.
evident reachable(A, B) :-
    reachable(A, M),
    reachable(M, B).

% Example: reachable(e, d) would be derived as:
%   reachable(e, d)
%     because reachable(e, a) [via edge(e,a)]
%         and reachable(a, d)
%               because reachable(a, b) [via edge(a,b)]
%                   and reachable(b, d) [via edge(b,d)]

% Implications: asserting a new edge makes downstream reachability evident.
% edge(d, e) => reachable(a, e).   % adding this edge closes a cycle
```

The fixpoint semantics of Evident shine here: the runtime naturally computes the transitive closure without the programmer needing to worry about evaluation order or tabling. The `=>` implication syntax for dynamic assertions is used in the comment to show how adding `edge(d, e)` would propagate, though integrating it cleanly with the clause-body syntax is the main design tension. The risk of non-termination with the naive transitive rule is the same as in Prolog; Evident's fixpoint model requires cycle detection or tabling to handle it safely.

---

## 5. FizzBuzz

```evident
% fizzbuzz(N, Result): Result is the FizzBuzz label for integer N
% clauses are unordered

% Divisible by both 3 and 5: "FizzBuzz"
evident fizzbuzz(N, "FizzBuzz") :-
    0 is N mod 3,
    0 is N mod 5.

% Divisible by 3 only: "Fizz"
evident fizzbuzz(N, "Fizz") :-
    0 is N mod 3,
    not(0 is N mod 5).

% Divisible by 5 only: "Buzz"
evident fizzbuzz(N, "Buzz") :-
    0 is N mod 5,
    not(0 is N mod 3).

% Not divisible by 3 or 5: the number itself
evident fizzbuzz(N, N) :-
    not(0 is N mod 3),
    not(0 is N mod 5).
```

Because clauses are unordered, the FizzBuzz-first rule cannot rely on coming before Fizz and Buzz — each clause must be made mutually exclusive through explicit `not` guards. This is more verbose than a Prolog cut-based solution but more honest: the conditions are self-documenting and the correctness of the partition is locally verifiable. The use of `not` here is classical negation-as-failure; Evident would need to decide whether this is NAF or true negation, which has deep implications for the evidence model.

---

## Overall Assessment

The Prolog-adjacent syntax has a high ceiling for experienced logic programmers: the notation is dense, precise, and maps directly onto the underlying derivation model. The `evident` keyword adds semantic clarity without disrupting the familiar structure, and self-evident facts ending in `.` read naturally as axioms. The unordered-clause discipline is easy to communicate through comments and aligns well with Evident's fixpoint semantics.

The main friction points are inherited from Prolog's age: arithmetic via `is` and `mod` feels like an escape hatch rather than a first-class feature, list syntax `[H|T]` is powerful but cryptic to newcomers, and `=<` for less-than-or-equal is a longstanding typographic quirk. The `not` predicate's ambiguity between NAF and true negation is a genuine semantic landmine that Evident would need to resolve explicitly. For an audience already fluent in logic programming, this syntax is immediately productive; for a broader audience, the learning curve is steep and the error messages would need to be far better than Prolog's to compensate.
