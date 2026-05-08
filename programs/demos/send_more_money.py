from itertools import permutations
for s, e, n, d, m, o, r, y in permutations(range(10), 8):
    if s and m and 1000*s+100*e+10*n+d + 1000*m+100*o+10*r+e == 10000*m+1000*o+100*n+10*e+y:
        print(f"S={s} E={e} N={n} D={d} M={m} O={o} R={r} Y={y}"); break
