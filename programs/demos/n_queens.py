def solve(n, q=()):
    if len(q) == n: return q
    for c in range(n):
        if all(c != x and abs(c - x) != len(q) - i for i, x in enumerate(q)):
            r = solve(n, q + (c,))
            if r: return r

print(solve(8))
