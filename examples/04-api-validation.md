# Example 4: API Validation — Hierarchical Evidence with Types

This is the "HTTP validation" example from before, but redesigned to show:
- Rich type-driven validation (not just boolean checks)
- The refactoring flow as we discover missing constraints
- Evidence terms as audit trails
- Forward implication for derived facts

---

## Types

```evident
type HttpMethod = GET | POST | PUT | DELETE | PATCH

type AuthScheme = Bearer | Basic | ApiKey

type Token = {
    raw       : String
    scheme    : AuthScheme
    subject   : String
    issued_at : Nat           -- unix timestamp
    expires_at : Nat
    scopes    : List String
}

type Request = {
    method  : HttpMethod
    path    : String
    headers : { authorization : String, content_type : String }
    body    : Maybe String
    time    : Nat             -- when the request arrived
}
```

---

## Step 0: One big undifferentiated claim

```evident
claim valid_request : Request -> Prop
```

```evident
assert req {
    method  = POST
    path    = "/api/orders"
    headers = { authorization = "Bearer abc123", content_type = "text/plain" }
    body    = Some "{ \"item\": \"widget\" }"
    time    = 1714000000
}

? valid_request req
```

```
-- Solver: valid_request is not evident (no rules).
-- Or: if we add 'evident valid_request _', then everything is valid.
-- Either way, useless.
```

---

## Step 1: Break into named sub-claims

```evident
evident valid_request req
    valid_method req
    valid_auth req
    valid_content req
```

Now we have three named sub-claims. Each can be checked independently,
each can have its own error evidence if it fails.

```evident
claim valid_method  : Request -> semidet
claim valid_auth    : Request -> semidet
claim valid_content : Request -> semidet

evident valid_method req when req.method in [GET, POST, PUT, DELETE]
```

```evident
? valid_method req   -- Yes ✓ (POST is in the list)
? valid_auth req     -- Not yet defined (not evident)
? valid_request req  -- Not evident (valid_auth not established)
```

---

## Step 2: Auth validation — decomposed with types

```evident
evident valid_auth req
    parse_token req.headers.authorization ?token
    token_not_expired token req.time
    token_signature_valid token

claim parse_token : String -> Token -> semidet

evident parse_token raw_header token
    split_bearer raw_header ?raw_jwt
    decode_jwt raw_jwt ?claims
    token = {
        raw       = raw_header
        scheme    = Bearer
        subject   = claims.sub
        issued_at = claims.iat
        expires_at = claims.exp
        scopes    = claims.scopes
    }

claim token_not_expired : Token -> Nat -> semidet

evident token_not_expired token current_time
    token.expires_at > current_time

claim token_signature_valid : Token -> semidet
-- (dispatched to cryptographic verifier — self-evident leaf)
```

```evident
? valid_auth req
```

```
-- Assuming "Bearer abc123" is a valid JWT that hasn't expired:
-- valid_auth req is evident.
-- Evidence term:
ParsedToken {
    token = { scheme = Bearer, subject = "user:456", ... }
    not_expired = ExpiryCheck { expires = 1714003600, current = 1714000000 }
    signature   = SignatureVerification { algorithm = RS256, ... }
}
```

---

## Step 3: Content validation — and we discover a problem

```evident
evident valid_content req when req.method in [POST, PUT, PATCH]
    req.headers.content_type = "application/json"
    req.body is_some
    valid_json (unwrap req.body)

evident valid_content req when req.method in [GET, DELETE]
    -- GET and DELETE have no body; content type is irrelevant
```

Now we query:

```evident
? valid_request req
```

```
-- Not evident!
-- valid_content requires content_type = "application/json"
-- but req has content_type = "text/plain"
-- The constraint fails.
```

We discovered a bug in our test request. The solver told us exactly why.

Fix the request:

```evident
assert req {
    method  = POST
    path    = "/api/orders"
    headers = { authorization = "Bearer abc123", content_type = "application/json" }
    body    = Some "{ \"item\": \"widget\" }"
    time    = 1714000000
}

? valid_request req
```

```
-- valid_request req is evident ✓
-- Evidence tree:
ValidRequest {
    method  = MethodOk  { method = POST }
    auth    = ParsedToken { ... }
    content = JsonBody {
        content_type = "application/json"
        body_present = Some "{ \"item\": \"widget\" }"
        valid_json   = JsonParsed { ... }
    }
}
```

---

## Step 4: Authorization — what the user can do

So far we've validated the request's form. Now: is this user *allowed* to do this?

```evident
claim authorized_for : Token -> Request -> semidet

evident authorized_for token req
    required_scope req ?scope
    scope in token.scopes

claim required_scope : Request -> String -> det

evident required_scope req "orders:read"  when req.method = GET,    req.path starts_with "/api/orders"
evident required_scope req "orders:write" when req.method = POST,   req.path starts_with "/api/orders"
evident required_scope req "orders:write" when req.method = PUT,    req.path starts_with "/api/orders"
evident required_scope req "orders:write" when req.method = DELETE, req.path starts_with "/api/orders"
```

Add to the top-level claim:

```evident
evident valid_request req
    valid_method req
    valid_auth req              -- establishes the token as a side-effect of evidence
    valid_content req
    authorized_for ?token req   -- uses the token from valid_auth's evidence

-- Wait: how does authorized_for get the token that valid_auth established?
```

This surfaces an interesting design question: `valid_auth` produces a `Token` as part of its
evidence, and `authorized_for` needs that token. We need the evidence from one sub-claim
to flow into another.

One approach — name the intermediate evidence explicitly:

```evident
evident valid_request req
    valid_method req
    valid_auth req as auth_ev           -- bind the evidence to a name
    valid_content req
    authorized_for auth_ev.token req    -- extract token from the evidence
```

The evidence term from `valid_auth` contains a `token` field (because `parse_token` established it
and `valid_auth`'s evidence includes that). We project the field and pass it to `authorized_for`.

---

## Step 5: Forward implication — derived capabilities

Once a request is valid, we can derive what the handler is allowed to do:

```evident
-- A valid POST to /api/orders means we can create an order
valid_request req, req.method = POST, req.path starts_with "/api/orders"
    => can_create_order req

-- A valid GET means we can read
valid_request req, req.method = GET, req.path starts_with "/api/orders"
    => can_read_orders req
```

These are forward implications: once `valid_request` and the method are established,
the capability claim fires automatically. The handler never validates — it just checks
whether `can_create_order req` is evident.

```evident
-- Handler:
? can_create_order req
    then create_order req.body
    else reject 403 "Insufficient permissions"
```

---

## Composability: reusing validation sub-claims

Every sub-claim is independently testable and reusable:

```evident
-- Unit-test the token parsing in isolation
assert test_header "Bearer eyJhbGci..."
? parse_token test_header ?t
-- t = { scheme = Bearer, subject = "user:456", ... }

-- Check if a specific token has a specific scope
assert my_token { ..., scopes = ["orders:read", "profile"] }
? "orders:write" in my_token.scopes   -- Not evident
? "orders:read"  in my_token.scopes   -- Evident ✓

-- Check if a request would be authorized if we changed its method
? authorized_for my_token { req with method = GET }   -- Yes
? authorized_for my_token { req with method = POST }  -- No (missing orders:write)
```

---

## The evidence tree as an audit log

The full evidence tree for a valid request is a complete audit log:

```
ValidRequest {
    method = MethodOk { method = POST, allowed = [GET, POST, PUT, DELETE] }

    auth = TokenValidation {
        parsed = ParsedToken {
            header       = "Bearer eyJhbGci..."
            jwt_verified = SignatureOk { algorithm = RS256 }
            claims       = { sub = "user:456", exp = 1714003600, ... }
        }
        not_expired = ExpiryOk {
            expires_at   = 1714003600
            current_time = 1714000000
            margin       = 3600
        }
    }

    content = JsonContent {
        content_type = "application/json"
        body         = "{ \"item\": \"widget\" }"
        parsed       = JsonOk { root = Object [...] }
    }

    authorization = ScopeOk {
        required = "orders:write"
        granted  = ["orders:read", "orders:write", "profile"]
    }
}
```

This tree is not a log message we wrote. It is the evidence that the claim is true —
the solver produced it as a certificate. If you need to audit why a request was accepted
or rejected, you inspect this tree. Every field is a proved sub-claim.
