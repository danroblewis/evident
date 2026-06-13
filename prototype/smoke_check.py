import z3
from benchsuite import pretty, tactics
from benchsuite.tasks import TASKS
bad=0
for name,tk in TASKS.items():
    for enc in tk.encodings:
        for seq in ['', 'simplify[blast_select_store=True]', 'solve-eqs>simplify']:
            try:
                g=enc.build(tk.scales[0])
                if seq: g,_,_=tactics.apply(g, tactics.parse(seq))
                assert isinstance(pretty.goal(g),str)
            except Exception as ex:
                bad+=1; print('CRASH',name,enc.name,repr(seq),type(ex).__name__,ex)
print('crashes:', bad)
Int=z3.IntSort(); S=z3.Const('S',z3.SetSort(Int)); T=z3.Const('T',z3.SetSort(Int)); x=z3.Int('x')
print('forall:', pretty.expr(z3.ForAll([x], z3.Implies(z3.IsMember(x,S), x>0))))
print('subset:', pretty.expr(z3.IsSubset(S,T)))
print('union :', pretty.expr(z3.IsMember(z3.IntVal(5), z3.SetUnion(S,T))))
