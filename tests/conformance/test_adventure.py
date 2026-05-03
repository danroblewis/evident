"""
Execution tests for the text adventure game.

Tests the adventure game as a streaming program via `evident execute`,
feeding it sequences of commands and checking the output.
"""

import pytest
from pathlib import Path
from .conftest import _evident

ADVENTURE = str(Path(__file__).parent.parent.parent / 'programs' / 'adventure' / 'adventure.ev')


def run(commands: list[str]) -> list[str]:
    """Run the adventure with the given commands, return non-empty output lines."""
    stdin = '\n'.join(commands) + '\n'
    r = _evident('execute', ADVENTURE, stdin=stdin)
    assert r.returncode == 0, f"evident execute failed:\n{r.stderr}"
    return [line for line in r.stdout.splitlines() if line.strip()]


# ── Starting room ──────────────────────────────────────────────────────────────

def test_look_at_start_shows_entrance():
    lines = run(['look'])
    assert any('entrance' in l.lower() for l in lines), \
        f"Expected 'entrance' in output, got: {lines}"

def test_look_at_start_mentions_north_and_east():
    lines = run(['look'])
    text = ' '.join(lines).lower()
    assert 'north' in text or 'east' in text, \
        f"Expected directions in entrance description, got: {lines}"


# ── Movement ───────────────────────────────────────────────────────────────────

def test_go_north_reaches_forest():
    lines = run(['go north'])
    assert any('forest' in l.lower() for l in lines), \
        f"Expected 'forest' after going north, got: {lines}"

def test_go_east_reaches_cave():
    lines = run(['go east'])
    assert any('cave' in l.lower() for l in lines), \
        f"Expected 'cave' after going east, got: {lines}"

def test_go_north_then_south_returns_to_entrance():
    lines = run(['go north', 'go south'])
    last = lines[-1].lower() if lines else ''
    assert 'entrance' in last, \
        f"Expected to return to entrance, last line: {last!r}"

def test_abbreviation_n_moves_north():
    full  = run(['go north'])
    abbr  = run(['n'])
    assert full == abbr, \
        f"'n' should produce same output as 'go north'. got: full={full}, abbr={abbr}"

def test_abbreviation_e_moves_east():
    full = run(['go east'])
    abbr = run(['e'])
    assert full == abbr

def test_go_south_from_entrance_blocked():
    lines = run(['go south'])
    assert any("can't" in l.lower() or "cannot" in l.lower() for l in lines), \
        f"Expected blocked message going south from entrance, got: {lines}"

def test_go_west_from_entrance_blocked():
    lines = run(['go west'])
    assert any("can't" in l.lower() or "cannot" in l.lower() for l in lines), \
        f"Expected blocked message, got: {lines}"


# ── Multi-step navigation ──────────────────────────────────────────────────────

def test_reach_dungeon_via_cave():
    lines = run(['go east', 'go down'])
    assert any('dungeon' in l.lower() for l in lines), \
        f"Expected to reach dungeon via cave, got: {lines}"

def test_reach_tower_via_forest():
    lines = run(['go north', 'go east'])
    assert any('tower' in l.lower() for l in lines), \
        f"Expected to reach tower via forest, got: {lines}"

def test_full_loop_back_to_entrance():
    # entrance → forest → tower → forest → entrance
    lines = run(['go north', 'go east', 'go west', 'go south', 'look'])
    assert any('entrance' in l.lower() for l in lines), \
        f"Expected to return to entrance after loop, got: {lines}"


# ── Commands ───────────────────────────────────────────────────────────────────

def test_help_lists_commands():
    lines = run(['help'])
    text = ' '.join(lines).lower()
    assert 'go' in text, f"Expected 'go' in help output, got: {lines}"

def test_question_mark_is_help():
    help_lines  = run(['help'])
    qmark_lines = run(['?'])
    assert help_lines == qmark_lines

def test_quit_says_goodbye():
    lines = run(['quit'])
    assert any('goodbye' in l.lower() or 'bye' in l.lower() for l in lines), \
        f"Expected goodbye message, got: {lines}"

def test_q_is_quit():
    quit_lines = run(['quit'])
    q_lines    = run(['q'])
    assert quit_lines == q_lines

def test_unknown_command_gives_error():
    lines = run(['xyzzy'])
    assert any("don't understand" in l.lower() or 'unknown' in l.lower() or "invalid" in l.lower()
                for l in lines), \
        f"Expected error for unknown command, got: {lines}"

def test_empty_line_produces_no_output():
    # Send a look first to initialize location, then an empty line.
    # The empty line should not change the room or produce a description.
    before = run(['look'])
    after  = run(['look', ''])
    # The look outputs should match (same room, no spurious description appended)
    assert before == after, \
        f"Empty line after look should produce no extra output.\nbefore={before}\nafter={after}"


# ── Look command ───────────────────────────────────────────────────────────────

def test_look_and_l_are_equivalent():
    look_lines = run(['look'])
    l_lines    = run(['l'])
    assert look_lines == l_lines

def test_examine_is_equivalent_to_look():
    look_lines    = run(['look'])
    examine_lines = run(['examine'])
    assert look_lines == examine_lines

def test_look_after_move_shows_new_room():
    entrance_desc = run(['look'])
    forest_desc   = run(['go north', 'look'])
    assert entrance_desc != forest_desc, \
        "Description after moving should differ from entrance"
    assert any('forest' in l.lower() for l in forest_desc)
