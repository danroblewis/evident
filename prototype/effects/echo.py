"""echo — read a line from stdin and print it, via a UserPropagateBase.

Prettified constraint model:

    echoed = line

`line` is the value read from the world (an effect); `echoed` is what the program
prints. At the commit point (a complete model) the propagator reads a real line
from stdin and prints it. The backtracking can be weird and we don't care — it
reads and logs. Run:  echo "hello" | python3 -m effects.echo
"""
import sys
import z3
from effects.runtime import EffectProp

# ── the constraint model: echo what we read ──
line = z3.String("line")        # the line read from stdin (the effect's result)
echoed = z3.String("echoed")    # what the program prints
s = z3.Solver()
s.add(echoed == line)


class Echo(EffectProp):
    def __init__(self, s):
        super().__init__(s)
        self.done = False

    def on_final(self):                     # commit point: a complete model
        if self.done:
            return
        self.done = True
        data = sys.stdin.readline().rstrip("\n")    # the read effect
        print(data)                                 # the echo / log


if __name__ == "__main__":
    Echo(s)
    s.check()
