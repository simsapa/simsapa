import csv, collections
rows = list(csv.DictReader(open('guess_sample_30k.tsv'), delimiter='\t'))
def first(r, form):
    for a in ('a2', 'a4'):
        k = r[f'{a}_{form}']
        if k not in ('', '-'):
            return a, k, r[f'{a}_{form}_breakdown']
    return None
print('rows', len(rows), 'distinct', len({r['query'] for r in rows}))
p8 = [r for r in rows if r['other_results']=='no' and int(r['p8_results'])>0]
print('no other results, Phase 8 (guesses off) gives results:', len(p8))
none_any = [r for r in rows if r['other_results']=='no' and int(r['p8_results'])==0]
print('no other results and Phase 8 empty:', len(none_any))
for form in ('like','range','range_bd'):
    g = [r for r in rows if (f:=first(r,form)) and f[2]]
    g_p8 = [r for r in g if int(r['p8_results'])>0 or r['other_results']=='yes']
    g_only = [r for r in g if r['other_results']=='no' and int(r['p8_results'])==0]
    print(f'{form:8} with bd={len(g)}  where something else exists={len(g_p8)}  only answer={len(g_only)}')
# like vs range_bd keys
same=diff=0; dlist=[]
for r in rows:
    a=first(r,'like'); b=first(r,'range_bd')
    if a and a[2] and b:
        if a[1]==b[1]: same+=1
        else: diff+=1; dlist.append((r['query'],a[1],b[1]))
print('like(with bd) vs range_bd: same key', same, 'different', diff)
# like hits with empty bd, where range_bd found a bd
lost=[r['query'] for r in rows if (a:=first(r,'like')) and not a[2] and first(r,'range_bd')]
print('like returned a row with no bd, range_bd found one:', len(lost), lost)
# attempt
att=collections.Counter(first(r,'range_bd')[0] for r in rows if first(r,'range_bd'))
print('range_bd hit attempt', att)
# structural: last component of every break-down is a clitic
CL={'iti','api','pi','ca','eva','va','ādi','iccādi','iva','hi'}
seen=set(); cl=0; tot=0; extra=[]
for r in rows:
    f=first(r,'range_bd')
    if not f or r['query'] in seen: continue
    seen.add(r['query']); tot+=1
    bds=[b.split(' + ') for b in f[2].split(' | ')]
    if all(b[-1] in CL for b in bds): cl+=1
    extra.append(len(f[1])-len(r['query']))
print('distinct range_bd guesses', tot, '; all break-downs end in iti/api/ca/eva/ādi/iccādi:', cl)
print('key length minus query length:', sorted(collections.Counter(extra).items()))
# query length of words reaching guesses
print('reaching: query length distribution', sorted(collections.Counter(min(len(r['query']),15) for r in rows).items()))
# fragments: start with a cluster no Pāli word starts with
frag=[r['query'] for r in rows if r['query'][:2] in ('ññ','nt','ṇṇ','ṅk','mp','tt','pp','kk','cc','ṭṭ','dd','bb','ss','ll','yy','nd','ṇḍ','mb','ṅg','jj','gg','ḍḍ')]
print('starts with a doubled/nasal cluster (text-splitting residue?):', len(frag), frag[:20])

ed=collections.Counter(('ms' if r['sutta_uid'].endswith('/ms') else 'cst') for r in rows)
print('reaching by edition', ed)
edg=collections.Counter(('ms' if r['sutta_uid'].endswith('/ms') else 'cst') for r in rows if first(r,'range_bd'))
print('range_bd guesses by edition', edg)
