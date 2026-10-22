import os, sqlite3, sys, time
DB = sys.argv[1]
UB = chr(0x10FFFF)

def io():
    d = {}
    for l in open('/proc/self/io'):
        k, v = l.split(':'); d[k] = int(v)
    return d

def evict():
    fd = os.open(DB, os.O_RDONLY)
    os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
    os.close(fd)

def A(c, p):
    r = c.execute("SELECT * FROM lookup WHERE lookup_key LIKE ? LIMIT 1", (p + '%',)).fetchone()
    return (r[0] if r else None, 1 if r else 0)

def B(c, p):
    k = c.execute("SELECT lookup_key FROM lookup WHERE lookup_key >= ? AND lookup_key < ? ORDER BY length(lookup_key), lookup_key LIMIT 1", (p, p + UB)).fetchone()
    if not k: return (None, 0)
    r = c.execute("SELECT * FROM lookup WHERE lookup_key = ?", (k[0],)).fetchone()
    return (r[0], 1)

def C(c, p):
    r = c.execute("SELECT * FROM lookup WHERE lookup_key >= ? AND lookup_key < ? AND deconstructor != '' ORDER BY length(lookup_key), lookup_key LIMIT 1", (p, p + UB)).fetchone()
    return (r[0] if r else None, -1)

def D(c, p):
    keys = c.execute("SELECT lookup_key FROM lookup WHERE lookup_key >= ? AND lookup_key < ? ORDER BY length(lookup_key), lookup_key", (p, p + UB)).fetchall()
    n = 0
    for (k,) in keys:
        n += 1
        bd = c.execute("SELECT deconstructor FROM lookup WHERE lookup_key = ?", (k,)).fetchone()[0]
        if bd:
            return (k, n)
    return (None, n)

def run(fn, p, cold):
    if cold: evict()
    c = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    i0 = io(); t = time.perf_counter()
    res = fn(c, p)
    ms = (time.perf_counter() - t) * 1000; i1 = io()
    c.close()
    return ms, (i1['rchar'] - i0['rchar']) / 1e6, (i1['read_bytes'] - i0['read_bytes']) / 1e6, res

words = ["kilesapariruddho", "aggaphalasukhaṁ", "salalarukkho", "sañcuṇṇamathitā", "pupphasu"]
prefixes = []
for w in words:
    prefixes.append(("a2 " + w, w))
    prefixes.append(("a4 " + w, w[:-1]))
prefixes += [("worst3 sam", "sam"), ("prd saṁ", "saṁ")]

print("sqlite", sqlite3.sqlite_version)
print(f"{'case':28} {'ap':2} {'cold_ms':>8} {'warm_ms':>8} {'req_MB':>8} {'disk_MB':>8}  rows_probed  key")
for label, p in prefixes:
    for name, fn in [("A", A), ("B", B), ("C", C), ("D", D)]:
        cms, req, disk, res = run(fn, p, True)
        wms, _, _, _ = run(fn, p, False)
        print(f"{label:28} {name:2} {cms:8.1f} {wms:8.1f} {req:8.2f} {disk:8.2f}  {res[1]:>5}  {res[0]}")
