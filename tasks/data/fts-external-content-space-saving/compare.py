#!/usr/bin/env python3
"""Compare two fts_bench TSV files with the FR-4 gate.

A case passes when median_b <= max(1.10 * median_a, median_a + 20 ms), and the
result counts are equal. When both files have the uids hash column (7th), the
hashes of the result page's uids must be equal too; '-' or a missing column is
not compared. Usage: compare.py a.tsv b.tsv [--both-ways]
--both-ways also checks a against b (for a noise check of two runs of one copy).
"""
import sys
from collections import defaultdict


def load(path):
    d = {}
    for line in open(path, encoding="utf-8"):
        parts = line.rstrip("\n").split("\t")
        if len(parts) == 6:
            parts.append("-")
        if len(parts) != 7:
            continue
        _, case, kind, med, worst, count, uids = parts
        d[(case, kind)] = (float(med), float(worst), int(count), uids)
    return d


def limit(a):
    return max(1.10 * a, a + 20.0)


def main():
    a_path, b_path = sys.argv[1], sys.argv[2]
    both = "--both-ways" in sys.argv
    a, b = load(a_path), load(b_path)
    fails = []
    count_diff = []
    hash_diff = []
    per_table = defaultdict(lambda: [0, 0])
    for key in sorted(set(a) & set(b)):
        case, kind = key
        ma, _, ca, ha = a[key]
        mb, _, cb, hb = b[key]
        table = case.split("|")[0]
        per_table[table][0] += 1
        if ca != cb:
            count_diff.append((case, kind, ca, cb))
        if ha != "-" and hb != "-" and ha != hb:
            hash_diff.append((case, kind, ha, hb))
        bad = mb > limit(ma) or (both and ma > limit(mb))
        if bad:
            per_table[table][1] += 1
            fails.append((case, kind, ma, mb, (mb / ma - 1) * 100 if ma else 0.0))
    missing = sorted(set(a) ^ set(b))
    print(f"compared {len(set(a) & set(b))} lines; outside the gate: {len(fails)}; count differences: {len(count_diff)}; uid differences: {len(hash_diff)}; missing: {len(missing)}")
    for t, (n, f) in sorted(per_table.items()):
        print(f"  {t}: {f} of {n} outside")
    for case, kind, ma, mb, pct in fails:
        print(f"  OUT {kind}\t{ma:.1f} -> {mb:.1f} ms ({pct:+.0f}%)\t{case}")
    for case, kind, ca, cb in count_diff:
        print(f"  COUNT {kind}\t{ca} != {cb}\t{case}")
    for case, kind, ha, hb in hash_diff:
        print(f"  UIDS {kind}\t{ha} != {hb}\t{case}")
    for m in missing[:20]:
        print(f"  MISSING {m}")


if __name__ == "__main__":
    main()
