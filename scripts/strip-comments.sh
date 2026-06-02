#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# strip-comments.sh — strip comments from Rust (and similar) source.
#
# Reads source from stdin or a file path. Writes stripped output to stdout.
#
# Handles:
#   - `//` line comments
#   - `/* ... */` block comments (multi-line, non-nested)
#   - Skips `//`-like sequences inside string literals (`"..."`),
#     raw strings (`r"..."`, `r#"..."#`, etc.), and char literals (`'x'`)
#   - Distinguishes char literals from lifetimes by checking for a
#     closing `'` before a newline.
#
# Also collapses runs of blank lines down to one and trims trailing
# whitespace from each line.
#
# Usage:
#   strip-comments.sh path/to/file.rs  > stripped.rs
#   cat file.rs | strip-comments.sh    > stripped.rs
#
# The output is NOT guaranteed to be a verbatim semantic-preserving
# strip (it's a best-effort heuristic suitable for dumping into an LLM
# context, not for compilation).
#
# Transition-only Bash: a faithful byte-for-byte port of the retired
# scripts/strip-comments.py. The character state machine is expressed in
# embedded Perl (always present on macOS/Linux) because a per-char loop in
# pure Bash would be unusably slow. Operates on bytes so non-ASCII source
# passes through unchanged.

set -euo pipefail

exec perl -e '
my $arg = shift @ARGV;
my $t;
if (defined $arg && $arg ne "-" && $arg ne "--stdin") {
    open(my $fh, "<", $arg) or die "strip-comments: cannot open $arg: $!\n";
    local $/; $t = <$fh>; close $fh;
} else {
    local $/; $t = <STDIN>;
}
$t = "" unless defined $t;

# --- strip_rust_comments ---
my $out = "";
my $i = 0;
my $n = length($t);
while ($i < $n) {
    my $c = substr($t, $i, 1);

    # Double-quoted string.
    if ($c eq "\"") {
        $out .= $c; $i++;
        while ($i < $n) {
            my $ch = substr($t, $i, 1);
            if ($ch eq "\\" && $i + 1 < $n) { $out .= substr($t, $i, 2); $i += 2; next; }
            if ($ch eq "\"") { $out .= "\""; $i++; last; }
            $out .= $ch; $i++;
        }
        next;
    }

    # Raw string: r"..." or r#"..."# (any number of #).
    if ($c eq "r" && $i + 1 < $n && (substr($t, $i + 1, 1) eq "\"" || substr($t, $i + 1, 1) eq "#")) {
        my $j = $i + 1; my $hashes = 0;
        while ($j < $n && substr($t, $j, 1) eq "#") { $hashes++; $j++; }
        if ($j < $n && substr($t, $j, 1) eq "\"") {
            my $close = "\"" . ("#" x $hashes);
            my $end = index($t, $close, $j + 1);
            if ($end < 0) { $out .= substr($t, $i); $i = $n; }
            else { $out .= substr($t, $i, $end + length($close) - $i); $i = $end + length($close); }
            next;
        }
    }

    # Char literal vs lifetime.
    if ($c eq "'"'"'") {
        my $j = $i + 1;
        if ($j < $n && substr($t, $j, 1) eq "\\" && $j + 1 < $n) {
            my $k = $j + 2;
            while ($k < $n && substr($t, $k, 1) ne "'"'"'" && substr($t, $k, 1) ne "\n") { $k++; }
            if ($k < $n && substr($t, $k, 1) eq "'"'"'") {
                $out .= substr($t, $i, $k + 1 - $i); $i = $k + 1; next;
            }
        } elsif ($j < $n && substr($t, $j, 1) ne "'"'"'" && $j + 1 < $n && substr($t, $j + 1, 1) eq "'"'"'") {
            $out .= substr($t, $i, $j + 2 - $i); $i = $j + 2; next;
        }
        $out .= $c; $i++; next;
    }

    # Comments.
    if ($c eq "/" && $i + 1 < $n) {
        my $nxt = substr($t, $i + 1, 1);
        if ($nxt eq "/") {
            my $end = index($t, "\n", $i);
            if ($end < 0) { $i = $n; } else { $i = $end; }
            next;
        }
        if ($nxt eq "*") {
            my $end = index($t, "*/", $i + 2);
            if ($end < 0) { $i = $n; } else { $i = $end + 2; }
            next;
        }
    }

    $out .= $c; $i++;
}

# --- collapse_blanks ---
my $ends_nl = ($out =~ /\n\z/) ? 1 : 0;
# Emulate Python str.splitlines() for \n: a single trailing newline does
# not yield a final empty element.
my @parts = split(/\n/, $out, -1);
pop @parts if (@parts && $parts[-1] eq "" && $ends_nl);
my @res; my $blank = 0;
for my $ln (@parts) {
    $ln =~ s/\s+\z//;            # rstrip
    if ($ln eq "") {
        $blank++;
        push @res, "" if $blank <= 1;
    } else {
        $blank = 0;
        push @res, $ln;
    }
}
my $result = join("\n", @res);
$result .= "\n" if $ends_nl;
print $result;
' "$@"
