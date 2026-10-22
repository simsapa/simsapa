"""Cost per break-down guess form: cold / warm ms, bytes requested (rchar) and
bytes read from disk (read_bytes).

    python3 -I guess_cost.py <db> [forms]   forms: comma-separated, default "A,B,C,D"

<db> must be on a disk-backed filesystem (not tmpfs), or "cold" means nothing.
Point it at the dist dpd.sqlite3 (read-only). Form CI needs a scratch copy of
the lookup table with idx_lookup_key_with_deconstructor, which the dist DB lacks.

Forms:
  A   today's LIKE 'p%' LIMIT 1, table order
  B   index range, shortest key, then the row by key
  C   range + deconstructor != '', shortest key: the planner's choice of index
  CI  C forced onto the partial index (INDEXED BY idx_lookup_key_with_deconstructor),
      then the row by key (considered and rejected in task 7.1)
  D   keys from the key index by (length, key), rows one by one until one has a
      break-down: the form the app uses (it also caps the keys at 500)
"""
import os, sqlite3, sys, time
DB = sys.argv[1]
FORMS = (sys.argv[2] if len(sys.argv) > 2 else "A,B,C,D").split(",")
UB = chr(0x10FFFF)

C_SQL = ("SELECT lookup_key FROM lookup{ix} WHERE lookup_key >= ? AND lookup_key < ? "
         "AND deconstructor != '' ORDER BY length(lookup_key), lookup_key LIMIT 1")

def io():
    d = {}
    for l in open('/proc/self/io'):
        k, v = l.split(':'); d[k] = int(v)
    return d

def evict():
    fd = os.open(DB, os.O_RDONLY)
    os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
    os.close(fd)

def plan(c, sql, p):
    # Literal values: with STAT4 the plan depends on them, as it does for a bound statement.
    lit = sql.replace("?", "'{}'", 2).format(p.replace("'", "''"), (p + UB).replace("'", "''"))
    return c.execute("EXPLAIN QUERY PLAN " + lit).fetchone()[3].split(" (")[0].replace("SEARCH lookup USING ", "")

def A(c, p):
    r = c.execute("SELECT * FROM lookup WHERE lookup_key LIKE ? LIMIT 1", (p + '%',)).fetchone()
    return (r[0] if r else None, 1 if r else 0)

def B(c, p):
    k = c.execute("SELECT lookup_key FROM lookup WHERE lookup_key >= ? AND lookup_key < ? ORDER BY length(lookup_key), lookup_key LIMIT 1", (p, p + UB)).fetchone()
    if not k: return (None, 0)
    r = c.execute("SELECT * FROM lookup WHERE lookup_key = ?", (k[0],)).fetchone()
    return (r[0], 1)

def c_form(ix):
    def f(c, p):
        sql = C_SQL.format(ix=ix)
        k = c.execute(sql, (p, p + UB)).fetchone()
        if not k: return (None, plan(c, sql, p))
        r = c.execute("SELECT * FROM lookup WHERE lookup_key = ?", (k[0],)).fetchone()
        return (r[0], plan(c, sql, p))
    return f

def D(c, p):
    keys = c.execute("SELECT lookup_key FROM lookup WHERE lookup_key >= ? AND lookup_key < ? ORDER BY length(lookup_key), lookup_key", (p, p + UB)).fetchall()
    n = 0
    for (k,) in keys:
        n += 1
        bd = c.execute("SELECT deconstructor FROM lookup WHERE lookup_key = ?", (k,)).fetchone()[0]
        if bd:
            return (k, n)
    return (None, n)

ALL = {"A": A, "B": B, "C": c_form(""), "CI": c_form(" INDEXED BY idx_lookup_key_with_deconstructor"), "D": D}

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
prefixes += [("worst3 sam", "sam"), ("prd saṁ", "saṁ"), ("par", "par"), ("paṭ", "paṭ"),
             ("anu", "anu"), ("upa", "upa"), ("dham", "dham")]

print("sqlite", sqlite3.sqlite_version)
print(f"{'case':28} {'ap':2} {'cold_ms':>8} {'warm_ms':>8} {'req_MB':>8} {'disk_MB':>8}  probed/plan  key")
for label, p in prefixes:
    for name in FORMS:
        cms, req, disk, res = run(ALL[name], p, True)
        wms, _, _, _ = run(ALL[name], p, False)
        print(f"{label:28} {name:2} {cms:8.1f} {wms:8.1f} {req:8.2f} {disk:8.2f}  {res[1]!s:>5}  {res[0]}")
