#!/usr/bin/env bash
# Overnight refactor completion measures (2026-06-10). Each is a
# grep-level ground-truth predicate: 0 (or the stated target) = DONE.
cd "$(dirname "$0")/.."
m() { printf '%-52s %s\n' "$1" "$2"; }
echo "== overnight completion measures =="
m "A: V18 BLOCKER families (target 0)" \
  "$(grep -hoE 'enum_h_field[0-5]|vf_t[0-4]|recdecl_(ty|sort)[0-5]|enum_fieldsym[0-5]' compiler2/*.ev | wc -l)"
m "B: fold families type_pin_g*/dec_tok*/callable concat (0)" \
  "$(grep -hoE 'type_pin_g[1-5]|dec_tok[0-7]' compiler2/*.ev | wc -l)"
m "W1: bind peel bind_n*/h*/tail* (target 0)" \
  "$(grep -hoE 'bind_(n|h)[0-5]|bind_tail[0-4]' compiler2/*.ev | wc -l)"
m "W1: type Bind exists (target 1)" \
  "$(grep -c 'type Bind(' compiler2/*.ev | awk -F: '{s+=$2} END{print s+0}')"
m "Deprefix: ..-lifted components in driver.ev (target ~0)" \
  "$(grep -cE '^\s*\.\.Driver' compiler2/driver.ev)"
m "Deprefix: fsm-prefixed body vars (target ~0)" \
  "$(awk '/^fsm Driver/{p=tolower(substr($2,7)); sub(/\(.*/,"",p); next} /^[a-z]/{next} /^    [a-z_]+ ∈/{v=$1; if (p!="" && index(v, p"_")==1) c++} END{print c+0}' compiler2/driver_*.ev)"
m "Numbered families overall (xN suffix decls, info)" \
  "$(grep -hoE '^    [a-z_]+[0-9] ∈' compiler2/*.ev | wc -l)"
echo "(critic-class counts: see latest docs/critic-reports/*baseline*.md)"
