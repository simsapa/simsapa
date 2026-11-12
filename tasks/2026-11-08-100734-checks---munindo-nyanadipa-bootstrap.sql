-- Success-metric checks (task 9.5) for the segmented Munindo / Nyanadipa
-- import, the Dhammapada verse groups and the page-title segments, run against
-- a freshly bootstrapped appdata.sqlite3. Read-only.
--
--   sqlite3 -readonly <dist>/simsapa/app-assets/appdata.sqlite3 \
--     < tasks/2026-11-08-100734-checks---munindo-nyanadipa-bootstrap.sql
--
-- Each check prints a label and either "ok" or the offending rows.
-- A verse-group heading is selected by the rule of dhp_verse_group_ranges():
-- a `dhpN:0` key, or the chapter's first-verse `dhpN:0.4` key (the only
-- `0.x` key with x >= 4 in the 26 Pāli templates). `dhp416:5.0` is a
-- verse-story heading that by design gets no range, and is not selected.

.headers off
.mode list

-- 9.5.1 -----------------------------------------------------------------
SELECT '9.5.1 Munindo / Nyanadipa records (expect 26 / 36, all segmented, no HTML)';
SELECT author, count(*) AS records,
       sum(content_json IS NOT NULL AND content_json != '') AS with_json,
       sum(content_html IS NULL) AS html_null,
       sum(content_json_tmpl IS NOT NULL AND content_json_tmpl != '') AS with_tmpl
FROM (SELECT CASE WHEN uid LIKE '%/en/munindo' THEN 'munindo' ELSE 'nyanadipa' END AS author,
             content_json, content_html, content_json_tmpl
      FROM suttas WHERE uid LIKE '%/en/munindo' OR uid LIKE '%/en/nyanadipa')
GROUP BY author;
SELECT 'snp5.19/en/nyanadipa present: ' || count(*) FROM suttas WHERE uid = 'snp5.19/en/nyanadipa';
SELECT 'old snp5.18/en/nyanadipa present (expect 0): ' || count(*) FROM suttas WHERE uid = 'snp5.18/en/nyanadipa';

-- 9.5.2 -----------------------------------------------------------------
DROP TABLE IF EXISTS temp.dhp_headings;
CREATE TEMP TABLE dhp_headings AS
SELECT s.uid,
       substr(s.uid, instr(s.uid, '/') + 1) AS edition,
       j.key AS segment_id,
       j.value AS text
FROM suttas s, json_each(s.content_json) j
WHERE s.uid GLOB 'dhp*-*'
  AND (s.uid LIKE '%/pli/ms' OR s.uid LIKE '%/en/sujato'
       OR s.uid LIKE '%/en/thanissaro' OR s.uid LIKE '%/en/munindo')
  AND (j.key GLOB 'dhp*:0' OR j.key GLOB 'dhp*:0.4');

SELECT '9.5.2a headings per edition (expect four rows with equal counts, 26 records each)';
SELECT edition, count(DISTINCT uid) AS records, count(*) AS headings
FROM dhp_headings GROUP BY edition ORDER BY edition;

-- The range is the text before the first ". " (Pāli) or the whole text
-- without its final "." (translations).
DROP TABLE IF EXISTS temp.dhp_ranges;
CREATE TEMP TABLE dhp_ranges AS
SELECT uid, edition, segment_id, text,
       CASE WHEN edition = 'pli/ms' THEN substr(text, 1, instr(text || '. ', '. ') - 1)
            ELSE rtrim(text, '.') END AS range
FROM dhp_headings;

SELECT '9.5.2b headings without a well-formed range (expect none)';
SELECT uid, segment_id, quote(text) FROM dhp_ranges
WHERE range = '' OR range GLOB '*[^0-9–]*' OR range LIKE '%-%'
   OR (edition != 'pli/ms' AND text != range || '.')
   OR (edition = 'pli/ms' AND length(text) <= length(range) + 2);

SELECT '9.5.2c the same range in all four editions for each heading (expect none)';
SELECT segment_id, group_concat(edition || '=' || range, ' ') FROM dhp_ranges
GROUP BY segment_id HAVING count(DISTINCT range) != 1 OR count(*) != 4;

SELECT '9.5.2d Pāli ranges cover 1–423 once each (expect 423 | 423 | 1 | 423)';
WITH RECURSIVE bounds AS (
  SELECT CAST(CASE WHEN instr(range, '–') > 0 THEN substr(range, 1, instr(range, '–') - 1) ELSE range END AS INTEGER) AS first,
         CAST(CASE WHEN instr(range, '–') > 0 THEN substr(range, instr(range, '–') + 1) ELSE range END AS INTEGER) AS last
  FROM dhp_ranges WHERE edition = 'pli/ms'
), verses(v, last) AS (
  SELECT first, last FROM bounds
  UNION ALL SELECT v + 1, last FROM verses WHERE v < last
)
SELECT count(*), count(DISTINCT v), min(v), max(v) FROM verses;

SELECT '9.5.2e dhp416:5.0 untouched (expect Jotikattheravatthu without a range)';
SELECT quote(json_extract(content_json, '$."dhp416:5.0"')) FROM suttas WHERE uid = 'dhp383-423/pli/ms';

-- 9.5.3 -----------------------------------------------------------------
SELECT '9.5.3 page-title segments (expect munindo 26 / thanissaro 26 / nyanadipa 36, empty 0)';
WITH titled AS (
  SELECT s.uid,
         CASE WHEN s.uid LIKE '%/en/munindo' THEN 'munindo'
              WHEN s.uid LIKE '%/en/nyanadipa' THEN 'nyanadipa'
              ELSE 'thanissaro' END AS author,
         t.key AS segment_id,
         json_extract(s.content_json, '$."' || t.key || '"') AS title
  FROM suttas s, json_each(s.content_json_tmpl) t
  WHERE (s.uid LIKE '%/en/munindo' OR s.uid LIKE '%/en/nyanadipa'
         OR (s.uid GLOB 'dhp*-*' AND s.uid LIKE '%/en/thanissaro'))
    -- the page title is the <h1>; the verse-group headings are <h2 class='sutta-title'>
    AND (t.value LIKE '%<h1 class=''range-title''>%' OR t.value LIKE '%<h1 class=''sutta-title''>%')
)
SELECT author, count(DISTINCT uid) AS records, sum(coalesce(trim(title), '') = '') AS empty
FROM titled GROUP BY author;
SELECT 'samples: ' || group_concat(uid || ' = ' || json_extract(content_json, '$."dhp179:0.3"'), ' | ')
FROM suttas WHERE uid IN ('dhp179-196/en/munindo', 'dhp179-196/en/thanissaro');
SELECT 'samples: ' || json_extract(content_json, '$."snp4.2:0.2"') FROM suttas WHERE uid = 'snp4.2/en/nyanadipa';

-- 9.5.4 -----------------------------------------------------------------
SELECT '9.5.4 navigation leaves of these editions that open no record (expect none)';
SELECT n.node_uid, n.sutta_uid FROM navigation_nodes n
LEFT JOIN suttas s ON s.uid = n.sutta_uid
WHERE (n.sutta_uid GLOB 'dhp*' OR n.sutta_uid GLOB 'snp*')
  AND s.uid IS NULL;

-- 9.5.5 -----------------------------------------------------------------
-- The rule of default_english_for_pali() (backend/src/app_data.rs): leaving
-- out the AI English (`/en/opentipitaka`), Sujato's segmented English; else,
-- in uid order, the first segmented English with source_info NULL, else the
-- first segmented, else the first.
SELECT '9.5.5a default English beside the Pāli Dhp / Snp texts, by author (expect sujato only)';
WITH english AS (
  SELECT p.uid AS pali_uid, e.uid AS en_uid, e.source_info,
         row_number() OVER (PARTITION BY p.uid ORDER BY
           (coalesce(e.content_json, '') != '' AND e.uid LIKE '%/en/sujato') DESC,
           (coalesce(e.content_json, '') != '' AND e.source_info IS NULL) DESC,
           (coalesce(e.content_json, '') != '') DESC,
           e.uid) AS rank
  FROM suttas p
  JOIN suttas e ON e.uid >= substr(p.uid, 1, instr(p.uid, '/') - 1) || '/en/'
               AND e.uid <  substr(p.uid, 1, instr(p.uid, '/') - 1) || '/en0'
  WHERE (p.uid GLOB 'dhp*/pli/ms' OR p.uid GLOB 'snp*/pli/ms')
    AND e.uid NOT LIKE '%/en/opentipitaka'
)
SELECT substr(en_uid, instr(en_uid, '/en/') + 4), count(*)
FROM english WHERE rank = 1 GROUP BY 1 ORDER BY 1;

-- Nyanadipa notes -------------------------------------------------------
SELECT 'notes: Nyanadipa suttas with notes, and notes in all (expect 29 notes)';
SELECT count(*), sum((length(content_json) - length(replace(content_json, 'data-n=', ''))) / length('data-n='))
FROM sutta_comments WHERE sutta_uid LIKE '%/en/nyanadipa';
