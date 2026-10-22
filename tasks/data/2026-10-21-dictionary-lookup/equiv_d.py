"""Form D (cap 500) against form C over every attempt-2 / attempt-4 prefix of
guess_sample_30k.tsv: same key, rows checked, prefixes that reach the cap.
Also prints the plan of the key query for `sam` with literal values.

    python3 -I equiv_d.py <path to dpd.sqlite3> guess_sample_30k.tsv
"""
import csv, sqlite3, sys
c = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
UB = chr(0x10FFFF); CAP = 500
def C(p):
    r = c.execute("SELECT lookup_key FROM lookup INDEXED BY sqlite_autoindex_lookup_1 WHERE lookup_key >= ? AND lookup_key < ? AND deconstructor != '' ORDER BY length(lookup_key), lookup_key LIMIT 1", (p, p + UB)).fetchone()
    return r[0] if r else None
def D(p):
    keys = c.execute("SELECT lookup_key FROM lookup WHERE lookup_key >= ? AND lookup_key < ? ORDER BY length(lookup_key), lookup_key LIMIT ?", (p, p + UB, CAP)).fetchall()
    for n, (k,) in enumerate(keys, 1):
        if c.execute("SELECT deconstructor FROM lookup WHERE lookup_key = ?", (k,)).fetchone()[0]:
            return k, n
    return None, len(keys)
plan = c.execute("EXPLAIN QUERY PLAN SELECT lookup_key FROM lookup WHERE lookup_key >= 'sam' AND lookup_key < 'sam" + UB + "' ORDER BY length(lookup_key), lookup_key LIMIT 500").fetchall()
print("D key query plan (sam, literal):", [r[3] for r in plan])
rows = list(csv.DictReader(open(sys.argv[2]), delimiter="\t"))
n = same = 0; maxp = (0, ""); capped = 0
for r in rows:
    q = r["query"]
    if len(q) < 4: continue
    for p in (q, q[:-1]):
        k, probes = D(p); n += 1; same += k == C(p)
        if probes > maxp[0]: maxp = (probes, p)
        capped += probes == CAP
print(f"prefixes={n} D==C {same}; max probes {maxp}; capped {capped}")
