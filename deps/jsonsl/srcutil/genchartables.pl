#!/usr/bin/perl
# This script generates the character table for 'special' lookups
#
use strict;
use warnings;
use Getopt::Long;

################################################################################
################################################################################
### Character Table Definitions                                              ###
################################################################################
################################################################################
my @special_begin;
$special_begin[ord('-')] = 'JSONSL_SPECIALf_DASH';
$special_begin[ord('t')] = 'JSONSL_SPECIALf_TRUE';
$special_begin[ord('f')] = 'JSONSL_SPECIALf_FALSE';
$special_begin[ord('n')] = 'JSONSL_SPECIALf_NULL';
$special_begin[ord($_)]  = 'JSONSL_SPECIALf_UNSIGNED' for (0..9);
$special_begin[ord('0')] = 'JSONSL_SPECIALf_ZERO';

my @strdefs;
$strdefs[ord('\\')] = 1;
$strdefs[ord('"')] = 1;

#Tokens which terminate a 'special' sequence. Basically all JSON tokens
#themselves
my @special_end;
{
    my @toks = qw([ { } ] " : \\ );
    push @toks, ',';
    $special_end[ord($_)] = 1 for (@toks);
}

#RFC 4627 allowed whitespace
my @wstable;
foreach my $x (0x20, 0x09, 0xa, 0xd) {
    $wstable[$x] = 1;
    $special_end[$x] = 1;
}

my @special_body;
{
    foreach my $x (0..9) {
        $special_body[ord($x)] = 1;
    }
    foreach my $x ('E', 'e', 'a','l','s','u','-','+', '.') {
        $special_body[ord($x)] = 1;
    }
}

my @unescapes;
$unescapes[ord('t')] = 0x09;
$unescapes[ord('b')] = 0x08;
$unescapes[ord('n')] = 0x0a;
$unescapes[ord('f')] = 0x0c;
$unescapes[ord('r')] = 0x0d;

my @allowed_escapes;
{
    @allowed_escapes[ord($_)] = 1 foreach
        ('"', '\\', '/', 'b', 'f', 'n', 'r', 't', 'u');
}

my @string_passthrough;
$string_passthrough[ord($_)] = 1 for ('\\','"');
$string_passthrough[$_] = 1 for (0..19);

################################################################################
################################################################################
### CLI Options                                                              ###
################################################################################
################################################################################

my %HMap = (
    special => [ undef, \@special_begin ],
    strings => [ undef, \@strdefs ],
    special_end => [ undef, \@special_end ],
    special_body => [undef, \@special_body ],
    whitespace => [ undef, \@wstable ],
    unescapes => [undef, \@unescapes],
    allowed_escapes => [ undef, \@allowed_escapes],
    string_passthrough => [ undef, \@string_passthrough ]
);

my $Table;
my %opthash;
while (my ($optname,$optarry) = each %HMap) {
    $opthash{$optname} = \$optarry->[0];
}
GetOptions(%opthash, escape_newlines => \my $EscapeNewlines);

while (my ($k,$v) = each %HMap) {
    if ($v->[0]) {
        $Table = $v->[1];
        last;
    }
}

if (!$Table) {
    die("Please specify one of: " . join(",", keys %HMap));
}

################################################################################
################################################################################
### Logic                                                                    ###
################################################################################
################################################################################
my %PrettyMap = (
"\x00" => '<NUL>',
"\x01" => '<SOH>',
"\x02" => '<STX>',
"\x03" => '<ETX>',
"\x04" => '<EOT>',
"\x05" => '<ENQ>',
"\x06" => '<ACK>',
"\x07" => '<BEL>',
"\x08" => '<BS>',
"\x09" => '<HT>',
"\x0a" => '<LF>',
"\x0b" => '<VT>',
"\x0c" => '<FF>',
"\x0d" => '<CR>',
"\x0e" => '<SO>',
"\x0f" => '<SI>',
"\x10" => '<DLE>',
"\x11" => '<DC1>',
"\x12" => '<DC2>',
"\x13" => '<DC3>',
"\x14" => '<DC4>',
"\x15" => '<NAK>',
"\x16" => '<SYN>',
"\x17" => '<ETB>',
"\x18" => '<CAN>',
"\x19" => '<EM>',
"\x1a" => '<SUB>',
"\x1b" => '<ESC>',
"\x1c" => '<FS>',
"\x1d" => '<GS>',
"\x1e" => '<RS>',
"\x1f" => '<US>',
"\x20" => '<SP>',
"\x21" => '<!>',
"\x22" => '<">',
"\x23" => '<#>',
"\x24" => '<$>',
"\x25" => '<%>',
"\x26" => '<&>',
"\x27" => '<\'>',
"\x28" => '<(>',
"\x29" => '<)>',
"\x2a" => '<*>',
"\x2b" => '<+>',
"\x2c" => '<,>',
"\x2d" => '<->',
"\x2e" => '<.>',
"\x2f" => '</>',
"\x30" => '<0>',
"\x31" => '<1>',
"\x32" => '<2>',
"\x33" => '<3>',
"\x34" => '<4>',
"\x35" => '<5>',
"\x36" => '<6>',
"\x37" => '<7>',
"\x38" => '<8>',
"\x39" => '<9>',
"\x3a" => '<:>',
"\x3b" => '<;>',
"\x3c" => '<<>',
"\x3d" => '<=>',
"\x3e" => '<>>',
"\x3f" => '<?>',
"\x40" => '<@>',
"\x41" => '<A>',
"\x42" => '<B>',
"\x43" => '<C>',
"\x44" => '<D>',
"\x45" => '<E>',
"\x46" => '<F>',
"\x47" => '<G>',
"\x48" => '<H>',
"\x49" => '<I>',
"\x4a" => '<J>',
"\x4b" => '<K>',
"\x4c" => '<L>',
"\x4d" => '<M>',
"\x4e" => '<N>',
"\x4f" => '<O>',
"\x50" => '<P>',
"\x51" => '<Q>',
"\x52" => '<R>',
"\x53" => '<S>',
"\x54" => '<T>',
"\x55" => '<U>',
"\x56" => '<V>',
"\x57" => '<W>',
"\x58" => '<X>',
"\x59" => '<Y>',
"\x5a" => '<Z>',
"\x5b" => '<[>',
"\x5c" => '<\>',
"\x5d" => '<]>',
"\x5e" => '<^>',
"\x5f" => '<_>',
"\x60" => '<`>',
"\x61" => '<a>',
"\x62" => '<b>',
"\x63" => '<c>',
"\x64" => '<d>',
"\x65" => '<e>',
"\x66" => '<f>',
"\x67" => '<g>',
"\x68" => '<h>',
"\x69" => '<i>',
"\x6a" => '<j>',
"\x6b" => '<k>',
"\x6c" => '<l>',
"\x6d" => '<m>',
"\x6e" => '<n>',
"\x6f" => '<o>',
"\x70" => '<p>',
"\x71" => '<q>',
"\x72" => '<r>',
"\x73" => '<s>',
"\x74" => '<t>',
"\x75" => '<u>',
"\x76" => '<v>',
"\x77" => '<w>',
"\x78" => '<x>',
"\x79" => '<y>',
"\x7a" => '<z>',
"\x7b" => '<{>',
"\x7c" => '<|>',
"\x7d" => '<}>',
"\x7e" => '<~>',
"\x7f" => '<DEL>',
);

my @lines;
my $cur = { begin => 0, items => [], end => 0 };
push @lines, $cur;

my $i = 0;
my $cur_col = 0;

my $special_last = 0;

sub add_to_grid {
    my $v = shift;

    if ($special_last) {
        $cur = { begin => $i, end => $i, items => [ $v ]};
        push @lines, $cur;
        $special_last = 0;
        $cur_col = 1;
        return;
    } else {
        push @{$cur->{items}}, $v;
        $cur->{end} = $i;
        $cur_col++;
    }

    if ($cur_col >= 32) {
        $cur = {
            begin => $i+1, end => $i+1, items => [] };
        $cur_col = 0;
        push @lines, $cur;
    }
}

sub add_special {
    my $v = shift;
    push @lines, { items => [ $v ], begin => $i, end => $i };
    $special_last = 1;
}

$special_last = 0;
for (; $i < 255; $i++) {
    my $v = $Table->[$i];
    if (defined $v) {
        my $char_pretty = $PrettyMap{chr($i)};
        if (defined $char_pretty) {
            $v = sprintf("$v /* %s */", $char_pretty);
            add_special($v);
        } else {
            add_to_grid(1);
        }
    } else {
        add_to_grid(0);
    }
}

foreach my $line (@lines) {
    my $items = $line->{items};
    if (@$items) {
        printf("/* 0x%02x */ %s, /* 0x%02x */",
            $line->{begin}, join(",", @$items), $line->{end});
        if ($EscapeNewlines) {
            print "  \\";
        }
        print "\n";
    }
}
