"""model_const — constants shared by the Model core and its analysis/query layers.

Pulled into their own module so `model_analysis` / `model_query` can import them
WITHOUT importing `evident_viz` (which imports the mixin classes from those modules
at class-definition time — a back-dependency on evident_viz would be circular).
"""

# Per-solve wall-clock cap (ms). Every z3 Solver/Optimize the dynamics layer builds gets this, so a
# single intractable check — e.g. an NRA reachable-step on a nonlinear-Real sample (predator-prey's
# _prey·_pred) — returns `unknown` instead of hanging the whole server unboundedly (Ana #300). A timed
# check that returns unknown is treated exactly like unsat (no successor), so sampling stops cleanly.
SOLVE_TIMEOUT_MS = 4000


# Visual-channel effectiveness by variable class (Cleveland & McGill 1984 /
# Mackinlay 1986): POSITION decodes best for everything; SIZE is good for
# quantitative but poor for categorical; COLOR (hue) and FACET are excellent for
# categorical but weak for quantitative. importance(var) x this table decides which
# variable lands on which channel. Color/size/facet are SECONDARY — a good plot
# reads from its axes alone.
CHANNEL_FITNESS = {
    "x":       {"quant": 1.00, "cat": 0.90},
    "y":       {"quant": 1.00, "cat": 0.90},
    "size":    {"quant": 0.70, "cat": 0.25},
    "opacity": {"quant": 0.60, "cat": 0.25},
    "color":   {"quant": 0.40, "cat": 0.85},
    "facet":   {"quant": 0.20, "cat": 0.80},
    "shape":   {"quant": 0.10, "cat": 0.60},
}
