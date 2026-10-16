#!/usr/bin/perl
# normalize.pl OUT FILE...: rewrites the golden snapshot's logs in place, the output folder's name
# (with either slash) as <out> and timings as <t>, so the logs of two runs compare. Used by
# tools/golden.sh.
use strict;
use warnings;
my $out = shift;
(my $back = $out) =~ s{/}{\\}g;
$^I = '';
while (<>) {
    s/\Q$out\E/<out>/g;
    s/\Q$back\E/<out>/g;
    s/[0-9.]+ ?(ms|s)\b/<t>/g;
    print;
}
