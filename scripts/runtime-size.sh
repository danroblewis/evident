#!/usr/bin/env bash
# TODO: rewrite in Evident
#
# runtime-size.sh — measure the size of the runtime implementation.
#
# One combined report over two bodies of source:
#   - Rust under bootstrap/runtime/src — every `*.rs` with embedded
#     `#[cfg(test)]` blocks stripped out. The stripper is string-/comment-/
#     char-literal-aware so braces inside string literals or comments don't
#     fool it.
#   - Evident under stdlib/passes — every `*.ev` self-hosted pass. Evident
#     comments are `--` to end of line; the classifier strips those (and is
#     string-literal-aware) before counting code lines and tokens.
#
# A single summary table (Rust / Evident / Total columns), one combined
# file-length histogram, and one combined longest-files list — sized to fit
# on a screen. Comment lines, blank lines, and raw char counts are
# deliberately not reported.
#
# Usage:
#   scripts/runtime-size.sh                 # summary, fits on a page
#   scripts/runtime-size.sh --per-file      # also dump a per-file table
#
# Transition-only Bash: a faithful byte-for-byte port of the retired
# scripts/runtime-size.py. The per-char classifiers, #[cfg(test)] stripper,
# line/token counters, histogram, and tables are expressed in embedded Perl
# (always present on macOS/Linux); a pure-Bash port of the character state
# machines would be unusably slow.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

exec perl -e '
use POSIX qw(ceil);

my $repo_root = shift @ARGV;
my $per_file = 0;
for my $a (@ARGV) { $per_file = 1 if $a eq "--per-file"; }

my $RUST_ROOT = "$repo_root/bootstrap/runtime/src";
my $EVIDENT_ROOT = "$repo_root/stdlib/passes";

# ---------- classifiers ----------
# Each returns two array-refs of per-char flags: (in_code, is_comment).

sub ident_char { my $ch = shift; return ($ch =~ /[A-Za-z0-9_]/) ? 1 : 0; }

sub classify_rust {
    my ($t) = @_;
    my $n = length($t);
    my @in_code = (1) x $n;
    my @is_comment = (0) x $n;
    my $i = 0;
    while ($i < $n) {
        my $c = substr($t, $i, 1);

        # line comment
        if ($c eq "/" && $i + 1 < $n && substr($t, $i + 1, 1) eq "/") {
            while ($i < $n && substr($t, $i, 1) ne "\n") {
                $in_code[$i] = 0; $is_comment[$i] = 1; $i++;
            }
            next;
        }

        # block comment (nesting)
        if ($c eq "/" && $i + 1 < $n && substr($t, $i + 1, 1) eq "*") {
            my $depth = 1;
            $in_code[$i] = 0; $in_code[$i + 1] = 0;
            $is_comment[$i] = 1; $is_comment[$i + 1] = 1;
            $i += 2;
            while ($i < $n && $depth > 0) {
                if (substr($t, $i, 1) eq "/" && $i + 1 < $n && substr($t, $i + 1, 1) eq "*") {
                    $depth++;
                    $in_code[$i] = 0; $in_code[$i + 1] = 0;
                    $is_comment[$i] = 1; $is_comment[$i + 1] = 1;
                    $i += 2;
                } elsif (substr($t, $i, 1) eq "*" && $i + 1 < $n && substr($t, $i + 1, 1) eq "/") {
                    $depth--;
                    $in_code[$i] = 0; $in_code[$i + 1] = 0;
                    $is_comment[$i] = 1; $is_comment[$i + 1] = 1;
                    $i += 2;
                } else {
                    $in_code[$i] = 0; $is_comment[$i] = 1; $i++;
                }
            }
            next;
        }

        # raw string: (b?r) #* " ... " #*  — only at a token boundary
        if (($c eq "r" || $c eq "b") && ($i == 0 || !ident_char(substr($t, $i - 1, 1)))) {
            if (substr($t, $i) =~ /^(b?r(\#*)\")/) {
                my $hashes = $2;
                my $close = "\"" . $hashes;
                my $body_start = $i + length($1);
                my $end = index($t, $close, $body_start);
                $end = ($end == -1) ? $n : $end + length($close);
                my $lim = ($end < $n) ? $end : $n;
                for (my $k = $i; $k < $lim; $k++) { $in_code[$k] = 0; }
                $i = $end;
                next;
            }
        }

        # byte string b"..." or normal string "..."
        if ($c eq "\"" || ($c eq "b" && $i + 1 < $n && substr($t, $i + 1, 1) eq "\"")) {
            my $j = $i + ($c eq "b" ? 2 : 1);
            $in_code[$i] = 0;
            $in_code[$i + 1] = 0 if $c eq "b";
            while ($j < $n) {
                $in_code[$j] = 0;
                if (substr($t, $j, 1) eq "\\") { $j += 2; next; }
                if (substr($t, $j, 1) eq "\"") { $j++; last; }
                $j++;
            }
            $i = $j;
            next;
        }

        # char literal vs lifetime
        if ($c eq "'"'"'") {
            if (substr($t, $i) =~ /^('"'"'(?:\\u\{[0-9A-Fa-f_]+\}|\\.|[^'"'"'\\\n])'"'"')/) {
                my $len = length($1);
                for (my $k = $i; $k < $i + $len; $k++) { $in_code[$k] = 0; }
                $i += $len;
                next;
            }
            $i++;
            next;
        }

        $i++;
    }
    return (\@in_code, \@is_comment);
}

sub classify_evident {
    my ($t) = @_;
    my $n = length($t);
    my @in_code = (1) x $n;
    my @is_comment = (0) x $n;
    my $i = 0;
    while ($i < $n) {
        my $c = substr($t, $i, 1);

        # line comment: -- to end of line
        if ($c eq "-" && $i + 1 < $n && substr($t, $i + 1, 1) eq "-") {
            while ($i < $n && substr($t, $i, 1) ne "\n") {
                $in_code[$i] = 0; $is_comment[$i] = 1; $i++;
            }
            next;
        }

        # string literal "..."
        if ($c eq "\"") {
            $in_code[$i] = 0;
            my $j = $i + 1;
            while ($j < $n) {
                $in_code[$j] = 0;
                if (substr($t, $j, 1) eq "\\") {
                    $in_code[$j + 1] = 0 if $j + 1 < $n;
                    $j += 2; next;
                }
                if (substr($t, $j, 1) eq "\"") { $j++; last; }
                $j++;
            }
            $i = $j;
            next;
        }

        $i++;
    }
    return (\@in_code, \@is_comment);
}

# ---------- transforms / counters ----------

sub strip_comments_text {
    my ($t, $classify) = @_;
    my (undef, $is_comment) = $classify->($t);
    my $out = "";
    my $n = length($t);
    for (my $i = 0; $i < $n; $i++) {
        $out .= substr($t, $i, 1) unless $is_comment->[$i];
    }
    return $out;
}

my $CFG_TEST = qr/\#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/;

sub strip_cfg_test {
    my ($t) = @_;
    my ($in_code, undef) = classify_rust($t);
    my $n = length($t);
    my @spans;

    while ($t =~ /$CFG_TEST/g) {
        my $start = $-[0];
        my $mend = $+[0];
        next unless $in_code->[$start];
        my $j = $mend;
        my $brace_open;
        my $semi;
        while ($j < $n) {
            if ($in_code->[$j]) {
                if (substr($t, $j, 1) eq "{") { $brace_open = $j; last; }
                if (substr($t, $j, 1) eq ";") { $semi = $j; last; }
            }
            $j++;
        }
        if (defined $brace_open) {
            my $depth = 0;
            my $p = $brace_open;
            my $end = $n;
            while ($p < $n) {
                if ($in_code->[$p]) {
                    if (substr($t, $p, 1) eq "{") { $depth++; }
                    elsif (substr($t, $p, 1) eq "}") {
                        $depth--;
                        if ($depth == 0) { $end = $p + 1; last; }
                    }
                }
                $p++;
            }
            push @spans, [$start, $end];
        } elsif (defined $semi) {
            push @spans, [$start, $semi + 1];
        }
    }

    my $out = $t;
    for my $sp (sort { $b->[0] <=> $a->[0] } @spans) {
        my ($s, $e) = @$sp;
        $out = substr($out, 0, $s) . substr($out, $e);
    }
    return $out;
}

sub count_lines {
    my ($t, $classify) = @_;
    my ($in_code, undef) = $classify->($t);
    my @lines = split(/\n/, $t, -1);
    pop @lines if (@lines && $lines[-1] eq "");
    my $total = scalar(@lines);
    my $code = 0;
    my $offset = 0;
    for my $ln (@lines) {
        if ($ln =~ /\S/) {
            my $has_code = 0;
            my $len = length($ln);
            for (my $k = 0; $k < $len; $k++) {
                my $ch = substr($ln, $k, 1);
                if ($in_code->[$offset + $k] && $ch !~ /\s/) { $has_code = 1; last; }
            }
            $code++ if $has_code;
        }
        $offset += length($ln) + 1;
    }
    return ($total, $code);
}

sub count_tokens {
    my ($t) = @_;
    my $words = () = ($t =~ /\S+/g);
    my $lex = () = ($t =~ /[A-Za-z_][A-Za-z0-9_]*|[0-9][0-9_.]*|[^\sA-Za-z0-9_]/g);
    my $llm = ceil(length($t) / 4);
    return ($words, $lex, $llm);
}

# ---------- gather / collect ----------

sub gather {
    my ($root, $ext) = @_;
    my @files;
    return @files unless -d $root;
    my @stack = ($root);
    while (@stack) {
        my $dir = pop @stack;
        opendir(my $dh, $dir) or next;
        my @entries = sort readdir($dh);
        closedir($dh);
        for my $e (@entries) {
            next if $e eq "." || $e eq "..";
            my $p = "$dir/$e";
            if (-d $p) {
                next if $e eq "target";
                push @stack, $p;
            } elsif ($p =~ /\Q$ext\E\z/) {
                push @files, $p;
            }
        }
    }
    return sort @files;
}

sub relpath {
    my ($path, $base) = @_;
    my $b = $base;
    $b .= "/" unless $b =~ m{/$};
    if (index($path, $b) == 0) { return substr($path, length($b)); }
    return $path;
}

sub collect {
    my ($root, $ext, $classify, $strip_tests) = @_;
    my %agg = (files => 0, code => 0, words => 0, lex_tokens => 0, approx_llm => 0);
    my @rows;
    for my $path (gather($root, $ext)) {
        open(my $fh, "<:encoding(UTF-8)", $path) or next;
        local $/; my $text = <$fh>; close $fh;
        $text = "" unless defined $text;
        if ($strip_tests) { $text = strip_cfg_test($text); }
        my $code_only = strip_comments_text($text, $classify);
        my ($lt, $lc) = count_lines($text, $classify);
        my ($w, $lx, $llm) = count_tokens($code_only);
        $agg{files}++;
        $agg{code} += $lc;
        $agg{words} += $w;
        $agg{lex_tokens} += $lx;
        $agg{approx_llm} += $llm;
        push @rows, [relpath($path, $repo_root), $lc, $llm];
    }
    return (\%agg, \@rows);
}

# ---------- formatting ----------

sub commafy {
    my $n = shift;
    my $s = "$n";
    1 while $s =~ s/^(\d+)(\d{3})/$1,$2/;
    return $s;
}

sub print_summary {
    my ($rust, $ev) = @_;
    my %total;
    for my $k (keys %$rust) { $total{$k} = $rust->{$k} + $ev->{$k}; }
    my @cols = (["files","files"],["code","code"],["LLM tokens","approx_llm"],
                ["lexical","lex_tokens"],["words","words"]);
    my $hdr = "  " . sprintf("%-9s", "");
    for my $c (@cols) { $hdr .= sprintf("%11s", $c->[0]); }
    print "$hdr\n";
    for my $pair (["Rust",$rust],["Evident",$ev],["Total",\%total]) {
        my ($name, $agg) = @$pair;
        my $line = "  " . sprintf("%-9s", $name);
        for my $c (@cols) { $line .= sprintf("%11s", commafy($agg->{$c->[1]})); }
        print "$line\n";
    }
}

my @BUCKETS = (
    ["0\x{2013}49", 0, 50],
    ["50\x{2013}99", 50, 100],
    ["100\x{2013}199", 100, 200],
    ["200\x{2013}499", 200, 500],
    ["500\x{2013}999", 500, 1000],
    ["1000+", 1000, undef],
);

sub print_histogram {
    my ($totals) = @_;
    my @counts;
    for my $b (@BUCKETS) {
        my ($label, $lo, $hi) = @$b;
        my $c = 0;
        for my $v (@$totals) {
            if (defined $hi) { $c++ if ($lo <= $v && $v < $hi); }
            else { $c++ if ($lo <= $v); }
        }
        push @counts, $c;
    }
    my $peak = 0; for my $c (@counts) { $peak = $c if $c > $peak; }
    $peak = 1 unless $peak;
    my $width = 30;
    print "  File size distribution (code lines)\n";
    for (my $idx = 0; $idx < scalar(@BUCKETS); $idx++) {
        my $label = $BUCKETS[$idx][0];
        my $c = $counts[$idx];
        my $bars = int($c / $peak * $width + 0.5);
        my $bar = "\x{2588}" x $bars;
        printf "  %9s \x{2502} %s %d\n", $label, $bar, $c;
    }
}

sub print_longest {
    my ($rows, $k) = @_;
    $k = 6 unless defined $k;
    my @by_len = sort { $b->[1] <=> $a->[1] } @$rows;
    $k = scalar(@$rows) if $k > scalar(@$rows);
    print "  Longest $k files (code lines)\n";
    for (my $idx = 0; $idx < $k; $idx++) {
        my $r = $by_len[$idx];
        printf "    %6s  %s\n", commafy($r->[1]), $r->[0];
    }
}

binmode(STDOUT, ":utf8");

my ($rust_agg, $rust_rows) = collect($RUST_ROOT, ".rs", \&classify_rust, 1);
my ($ev_agg, $ev_rows) = collect($EVIDENT_ROOT, ".ev", \&classify_evident, 0);
my @rows = (@$rust_rows, @$ev_rows);

print "Runtime size \x{2014} Rust bootstrap/runtime/src + Evident stdlib/passes\n\n";
print_summary($rust_agg, $ev_agg);
print "\n";
my @totals = map { $_->[1] } @rows;
print_histogram(\@totals);
print "\n";
print_longest(\@rows, 6);

if ($per_file) {
    my @sorted = sort { $b->[1] <=> $a->[1] } @rows;
    printf "\n%-48s%8s%10s\n", "file", "code", "~tokens";
    print "-" x 66, "\n";
    for my $r (@sorted) {
        printf "%-48s%8s%10s\n", $r->[0], $r->[1], $r->[2];
    }
}
' "$REPO_ROOT" "$@"
