import z3
TOTAL = 300        # 3 accounts, 100 each

def transition(s, s2):
    """one unit transferred between any two of the 3 accounts (guarded by source>0),
    or idle. s, s2 are 3-tuples of account balances."""
    moves = [z3.And(*[s2[k] == s[k] for k in range(3)])]          # idle
    for i in range(3):
        for j in range(3):
            if i != j:
                conds = [s[i] > 0]                                # source has funds
                for k in range(3):
                    conds.append(s2[k] == s[k] + (1 if k == j else -1 if k == i else 0))
                moves.append(z3.And(*conds))
    return z3.Or(*moves)

# ── Spacer: extract the reachability fixed point of this transition ──
fp = z3.Fixedpoint(); fp.set(engine="spacer")
Inv = z3.Function("Inv", z3.IntSort(), z3.IntSort(), z3.IntSort(), z3.BoolSort())
fp.register_relation(Inv)
a, b, c, a2, b2, c2 = z3.Ints("a b c a2 b2 c2")
for v in (a, b, c, a2, b2, c2): fp.declare_var(v)
fp.rule(Inv(z3.IntVal(100), z3.IntVal(100), z3.IntVal(100)))     # init: 100/100/100
fp.rule(Inv(a2, b2, c2), [Inv(a, b, c), transition((a, b, c), (a2, b2, c2))])
# property: no account ever goes negative OR exceeds the total
bad = z3.Or(a < 0, b < 0, c < 0, a > TOTAL, b > TOTAL, c > TOTAL)
res = fp.query(z3.And(Inv(a, b, c), bad))
print("bad reachable?", res, " (unsat = proven safe forever)")
if res == z3.unsat:
    print("\nSPACER'S FIXED-POINT MODEL (the inductive invariant it synthesized):")
    print(fp.get_answer())
