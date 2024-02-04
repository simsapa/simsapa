#!/usr/bin/env nu

let url = 'http://localhost:4848/sutta_study'

let query_params = [
  '{ "sutta_panels": [ {"sutta_uid": "mn4/pli/ms", "find_text": "bhagava"} ], "lookup_panel": {"query_text": "bhagava"} }'
  '{ "sutta_panels": [ {"sutta_uid": "mn2/pli/ms", "find_text": "āsava"} ], "lookup_panel": {"query_text": "āsava"} }'
]

for x in [1 2 3 4 5 6 7 8 9 10] {
  let m = $x mod ($query_params | length)
  let params = $query_params | get $m
  print -n $"Query ($x) ... "
  curl -X POST --json $params $url
  print ""
  sleep 3sec
}
