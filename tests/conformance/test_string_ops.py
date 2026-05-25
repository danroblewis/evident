"""
Conformance tests for string-manipulation builtins (session GAPC).

Evident lowers these to Z3's string (seq) theory:

  length        #text  /  str_len(text)        → str.len
  index-of      index_of(text, sub[, off])     → str.indexof
  substring     substr(text, off, len)         → str.substr
  replace       replace(text, src, dst)        → str.replace
  char-at       char_at(text, i)               → str.at
  contains      str_contains(t, sub) / sub ∈ t → str.contains
  prefix test   starts_with(t, pre)            → str.prefixof
  suffix test   ends_with(t, suf)              → str.suffixof

These are load/setup-time operations (Z3 string solving only runs when one
appears in a queried constraint), so per-tick runtime is unaffected. They
are the capability that lets generics' split_generic_head + substitute_idents
be expressed in Evident.
"""

from .conftest import query, assert_sat, assert_unsat, assert_binding


# ---------------------------------------------------------------------------
# length
# ---------------------------------------------------------------------------

def test_str_len_and_cardinality():
    src = """
claim S
    s ∈ String = "hello"
    n ∈ Int = #s
    m ∈ Int = str_len(s)
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'n', 5)
    assert_binding(b, 'm', 5)


# ---------------------------------------------------------------------------
# substring / slice
# ---------------------------------------------------------------------------

def test_substr_slice():
    src = """
claim S
    s ∈ String = "Edge<Rect>"
    head ∈ String = substr(s, 0, 4)
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'head', 'Edge')


def test_substr_is_exact():
    """A wrong expected value is UNSAT — proves substr really computes."""
    src = """
claim U
    s ∈ String = "Edge<Rect>"
    head ∈ String = substr(s, 0, 4)
    head = "Edgz"
"""
    assert_unsat(query(src, 'U'))


# ---------------------------------------------------------------------------
# replace
# ---------------------------------------------------------------------------

def test_replace_first_occurrence():
    src = """
claim S
    t ∈ String = "Seq(T)"
    out ∈ String = replace(t, "T", "Rect")
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'out', 'Seq(Rect)')


# ---------------------------------------------------------------------------
# index-of
# ---------------------------------------------------------------------------

def test_index_of_present_and_absent():
    src = """
claim S
    s ∈ String = "Edge<Rect>"
    lt ∈ Int = index_of(s, "<")
    gt ∈ Int = index_of(s, ">")
    miss ∈ Int = index_of(s, "@")
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'lt', 4)
    assert_binding(b, 'gt', 9)
    assert_binding(b, 'miss', -1)


def test_index_of_with_offset():
    src = """
claim S
    s ∈ String = "a.b.c"
    second ∈ Int = index_of(s, ".", 2)
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'second', 3)


# ---------------------------------------------------------------------------
# char-at
# ---------------------------------------------------------------------------

def test_char_at():
    src = """
claim S
    s ∈ String = "abc"
    c ∈ String = char_at(s, 1)
"""
    b = assert_sat(query(src, 'S'))
    assert_binding(b, 'c', 'b')


# ---------------------------------------------------------------------------
# contains / prefix / suffix (Bool)
# ---------------------------------------------------------------------------

def test_contains_call_sat():
    src = """
claim S
    s ∈ String = "world.pos"
    str_contains(s, "pos")
"""
    assert_sat(query(src, 'S'))


def test_contains_infix_sat():
    """`sub ∈ text` infix form — the shape #18 used to drop."""
    src = """
claim S
    s ∈ String = "world.pos"
    "world" ∈ s
"""
    assert_sat(query(src, 'S'))


def test_contains_unsat():
    src = """
claim U
    s ∈ String = "abc"
    "xyz" ∈ s
"""
    assert_unsat(query(src, 'U'))


def test_starts_with_sat_and_unsat():
    sat = """
claim S
    s ∈ String = "world.pos"
    starts_with(s, "world.")
"""
    assert_sat(query(sat, 'S'))
    unsat = """
claim U
    s ∈ String = "world.pos"
    starts_with(s, "local.")
"""
    assert_unsat(query(unsat, 'U'))


def test_ends_with_sat():
    src = """
claim S
    s ∈ String = "world.pos"
    ends_with(s, ".pos")
"""
    assert_sat(query(src, 'S'))


# ---------------------------------------------------------------------------
# generics unblock: split_generic_head + substitute_idents
# ---------------------------------------------------------------------------

def test_generics_split_and_substitute():
    """The exact string manipulation PORT-generics couldn't express:
    split "Edge<Rect>" on `<`/`>` into "Edge" + "Rect", then substitute
    "T" → "Rect" inside "Seq(T)" to get "Seq(Rect)"."""
    src = """
claim Generics
    g ∈ String = "Edge<Rect>"
    lt ∈ Int = index_of(g, "<")
    gt ∈ Int = index_of(g, ">")
    head ∈ String = substr(g, 0, lt)
    arg ∈ String = substr(g, lt + 1, gt - lt - 1)
    tmpl ∈ String = "Seq(T)"
    mono ∈ String = replace(tmpl, "T", arg)
"""
    b = assert_sat(query(src, 'Generics'))
    assert_binding(b, 'head', 'Edge')
    assert_binding(b, 'arg', 'Rect')
    assert_binding(b, 'mono', 'Seq(Rect)')
