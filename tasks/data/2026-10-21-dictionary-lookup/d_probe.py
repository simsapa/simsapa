"""Form D headroom: for every 3- and 4-letter prefix of Pāli letters in the
lookup table, how many rows form D checks before it finds a key with a
break-down (keys in (length, key) order), or the whole range if none has one.

    python3 -I d_probe.py <path to dpd.sqlite3>
"""
import sqlite3, sys
from collections import defaultdict
PALI = set("aāiīuūeokgṅcjñṭḍṇtdnpbmyrlvshḷṁ")
c = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
rows = c.execute("SELECT lookup_key, deconstructor != '' FROM lookup").fetchall()
for n in (3, 4):
    groups = defaultdict(list)
    for k, bd in rows:
        if len(k) >= n: groups[k[:n]].append((len(k), k, bd))
    probes = []
    for p, g in groups.items():
        g.sort(); r = next((i + 1 for i, x in enumerate(g) if x[2]), None)
        probes.append((r if r else len(g), r is None, p, len(g)))
    probes = [x for x in probes if all(ch in PALI for ch in x[2])]
    probes.sort(reverse=True)
    over = lambda cap: sum(1 for x in probes if x[0] > cap)
    print(f"{n}-letter prefixes: {len(probes)}; probes >100: {over(100)}, >500: {over(500)}, >1000: {over(1000)}")
    print("  worst:", [(p, r, "no-bd" if nb else "", size) for r, nb, p, size in probes[:8]])
