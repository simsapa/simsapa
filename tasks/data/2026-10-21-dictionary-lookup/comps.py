import csv, sqlite3, json, sys
db = sqlite3.connect('file:/home/gambhiro/prods/apps/simsapa-ng-project/bootstrap-assets-resources/dist/simsapa/app-assets/dpd.sqlite3?mode=ro', uri=True)
def show(comp):
    r = db.execute("select headwords from lookup where lookup_key=?", (comp,)).fetchone()
    if not r or not r[0]: return f"{comp}=?"
    ids = json.loads(r[0])[:4]
    out = []
    for i in ids:
        h = db.execute("select lemma_1, meaning_1, meaning_2 from dpd_headwords where id=?", (i,)).fetchone()
        if h: out.append(f"{h[0]}: {(h[1] or h[2] or '')[:40]}")
    return f"{comp}=[" + "; ".join(out) + "]"
for i, x in enumerate(csv.DictReader(open('guess_review.tsv'), delimiter='\t')):
    comps = []
    for bd in x['guess_breakdown'].split(' | ')[:2]:
        for c in bd.split(' + '):
            if c not in comps: comps.append(c)
    print(f"{i:2} {x['query']}: " + " ".join(show(c) for c in comps))
    if x['like_useful'] == '':
        lc = []
        for bd in x['like_breakdown'].split(' | ')[:2]:
            for c in bd.split(' + '):
                if c not in lc: lc.append(c)
        print("     LIKE: " + " ".join(show(c) for c in lc))
