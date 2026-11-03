#!/bin/zsh
cd /home/gambhiro/prods/apps/simsapa-ng-project
B=simsapa/backend/target/release/examples/fts_bench
R=fts-bench-scratch/results
for i in 1 2; do
  $B --dir fts-bench-scratch/before --label before --runs 5 >| $R/8.2-b$i.tsv 2>| $R/8.2-b$i.err
  $B --dir fts-bench-scratch/after  --label after  --runs 5 >| $R/8.2-a$i.tsv 2>| $R/8.2-a$i.err
done
echo done >| $R/8.2-done
