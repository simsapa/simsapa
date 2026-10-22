import csv, sys
rows = list(csv.DictReader(open(sys.argv[1]), delimiter='\t'))
def pick(r):
    for a in ('a2', 'a4'):
        k = r[f'{a}_range_bd']
        if k not in ('', '-'):
            return a, k, r[f'{a}_range_bd_breakdown']
    return None
def like(r):
    for a in ('a2', 'a4'):
        k = r[f'{a}_like']
        if k not in ('', '-'):
            return a, k, r[f'{a}_like_breakdown']
    return ('', '', '')
seen = set(); cands = []
for r in rows:
    p = pick(r)
    if p and r['query'] not in seen:
        seen.add(r['query']); cands.append((r, p))
yes = [c for c in cands if c[0]['other_results'] == 'yes']
no = [c for c in cands if c[0]['other_results'] == 'no']
n = min(len(no), 50 - len(yes)) if len(sys.argv) < 4 else len(no)
step = len(no) / n
chosen = yes + [no[int(i * step)] for i in range(n)]
w = csv.writer(open(sys.argv[2], 'w'), delimiter='\t', lineterminator='\n')
w.writerow(['query', 'other_results', 'attempt', 'guess_key', 'guess_breakdown', 'like_attempt', 'like_key', 'like_breakdown', 'context', 'sutta_uid', 'useful', 'like_useful'])
for r, (a, k, bd) in chosen:
    la, lk, lbd = like(r)
    w.writerow([r['query'], r['other_results'], a, k, bd, la, lk, lbd, r['context'], r['sutta_uid'], '', '=' if lk == k else ('-' if not lk else ('no-bd' if not lbd else ''))])
print(len(cands), 'candidates;', len(yes), 'with other results; wrote', len(chosen))
