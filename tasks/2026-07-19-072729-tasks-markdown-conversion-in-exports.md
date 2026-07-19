# Tasks: Markdown Conversion in Exports (Org-Mode and DOCX)

PRD: [2026-07-19-072729-prd---markdown-conversion-in-exports.md](./2026-07-19-072729-prd---markdown-conversion-in-exports.md)

## Relevant Files

- `backend/src/markdown_convert.rs` - New module: shared mdast parse, styled-run flattening, Org-Mode emitter, DOCX (OOXML) emitter, plain-text fallback. Unit tests inline.
- `backend/src/prompt_utils.rs` - Factor the code-fence-around-tables pre-processing out of `markdown_to_html` into a shared helper (`unwrap_fenced_tables`).
- `backend/src/text_export.rs` - Org-Mode branches (`gloss_paragraph_orgmode`, `chat_message_orgmode`) call the Org emitter; retire `markdown_bullets_for_org`; update tests.
- `backend/src/docx_export.rs` - `format_paragraph` and the chat message loop call the DOCX emitter; extend `run()`/paragraph helpers (monospace, indent); generalize the table builder; new tests.
- `backend/src/lib.rs` - Register the new `markdown_convert` module.
- `PROJECT_MAP.md` - Note the new module.

### Notes

- Backend-only Rust work; run tests with `cd backend && cargo test` (only after all sub-tasks of a top-level task are done, per user preference).
- Build check: `make build -B`.
- No QML, bridge, or `export_types.rs` changes — only how `response` strings are rendered.
- The UI render path uses `to_html_with_options(text, &Options::gfm())` (`prompt_utils.rs:119`) with an `Err(_) => raw text` fallback; the export parse must mirror this: `to_mdast(text, &ParseOptions::gfm())`, `Err` → emit the raw response text.
- `markdown` crate is v1.0.0; mdast nodes live under `markdown::mdast::Node`. Verified against the crate source: variant is `Node::Blockquote` (no capital Q); `List { ordered: bool, start: Option<u32>, spread: bool }`; `ListItem { spread, checked }`; `Code { value, lang: Option<String> }`; `Heading { depth: u8 }`; `Link { url, children }`; `Table { align }`.
- `to_mdast` **practically cannot fail on plain markdown** — it only errors on MDX constructs, which `ParseOptions::gfm()` doesn't enable. The raw-text fallback is defensive; test it by calling the emitter-level fallback function directly rather than hunting for input that makes `to_mdast` fail.
- GFM parses `~~strike~~` as `Delete` and bare URLs as autolink `Link` nodes — both outside the PRD's supported set; `Delete` flattens to plain text via the fallback, and for autolinks (link text == url) emit the bare url (org) / plain url text (docx) instead of the redundant `[[url][url]]` / `url (url)`.

## Instructions for Completing Tasks

**IMPORTANT:** As you complete each task, you must check it off in this markdown file by changing `- [ ]` to `- [x]`. This helps track progress and ensures you don't skip any steps.

Update the file after completing each sub-task, not just after completing an entire parent task.

## Tasks

### 1.0 Shared Markdown conversion core

**Specs / dependencies:** No dependencies — this task only creates the module and refactors `prompt_utils.rs` without behavior change. API surface to end up with:

- `pub fn unwrap_fenced_tables(text: &str) -> String` (in `prompt_utils.rs`, or moved into `markdown_convert.rs` and re-used by `markdown_to_html`) — the existing two-regex fence-stripping.
- `pub fn parse_response(text: &str) -> Option<markdown::mdast::Node>` in `markdown_convert.rs`: applies `unwrap_fenced_tables`, trims, parses with `to_mdast(…, &ParseOptions::gfm())`; `None` on parse error (callers then emit the raw text).
- `struct InlineRun { text: String, bold: bool, italic: bool, code: bool, link_url: Option<String> }` (or equivalent) + `fn inline_runs(children: &[Node]) -> Vec<InlineRun>` — flattens `Text`/`Strong`/`Emphasis`/`InlineCode`/`Link`/`Break` (Break → `\n` in text); nested `Strong(Emphasis(..))` combines flags; unknown inline nodes flatten to their plain text.
- `fn node_plain_text(node: &Node) -> String` — recursive text flattening, the universal fallback for unsupported block nodes.

- [x] 1.0 Shared Markdown conversion core
  - [x] 1.1 In `prompt_utils.rs`, extract the two-regex fence-around-tables pre-processing (lines 100–117) into a `pub fn unwrap_fenced_tables(text: &str) -> String`; call it from `markdown_to_html`. No behavior change.
  - [x] 1.2 Create `backend/src/markdown_convert.rs` and register it in `backend/src/lib.rs`. Add `parse_response()` per the spec above (pre-process → trim → `to_mdast` with `ParseOptions::gfm()` → `Option<Node>`).
  - [x] 1.3 Implement `InlineRun` and `inline_runs()`: handle `Text`, `Strong`, `Emphasis`, `InlineCode`, `Link` (capture URL, flatten link children to text), `Break`/soft newline; combine nesting flags; flatten unknown inline nodes to text.
  - [x] 1.4 Implement `node_plain_text()` as the recursive plain-text fallback for any unsupported node.
  - [x] 1.5 Unit tests in `markdown_convert.rs` for the core: parse of a GFM table succeeds, fenced-table input parses as a `Table` node (pre-processing applied), `inline_runs` on `**bold** *it* `` `code` `` [t](u)` and on nested `**bold *both***`, and `node_plain_text` on an unsupported construct (e.g. an image or raw HTML node).
  - [x] 1.6 Run `cd backend && cargo test` — new tests pass, `prompt_utils` tests unchanged.

### 2.0 Org-Mode emitter and integration

**Specs / dependencies:** Depends on 1.0 (`parse_response`, `inline_runs`, `node_plain_text`). Public API: `pub fn markdown_to_orgmode(text: &str) -> String` — on `parse_response() == None`, returns the trimmed raw text. Construct mapping (PRD req. 7–8):

- Inline: `**b**`→`*b*`, `*i*`→`/i/`, `` `c` ``→`~c~`, link→`[[url][text]]`.
- Headings (any level) → bold standalone line `*Heading text*`, blank-line separated — never `*`-star headlines.
- Headline guard: no emitted body line may start with `*` + space or `*` at column 0 followed by text that Org would parse as a headline; prefix such lines with a zero-width space or restructure (also covers bold-at-line-start: `*bold*` at column 0 is emphasis, which is fine — the guard is for literal `* ` bullet/star lines from fallback text).
- Lists: `- ` unordered; ordered `N. ` honoring mdast `start`; nested items indented to align under the parent item's text (2 spaces per level for `- `, 3 for `N. `); "loose" list items (containing paragraphs) joined sensibly.
- Tables: `| a | b |` rows; after the header row emit `|---+---|` with one segment per column.
- Code: `#+begin_src <lang>` when `Code.lang` is `Some`, else `#+begin_example`; escape body lines starting with `*` or `#+` with a leading comma; matching `#+end_src`/`#+end_example`.
- Blockquote → `#+begin_quote`/`#+end_quote` (recurse into children); `ThematicBreak` → `-----`.
- Integration replaces, in `text_export.rs`: the `#+begin_src markdown\n{}\n#+end_src` wrappers in `gloss_paragraph_orgmode` (line ~164) and `chat_message_orgmode` (line ~318) with the converted org text, and deletes `markdown_bullets_for_org`.

- [ ] 2.0 Org-Mode emitter and integration
  - [ ] 2.1 Implement `markdown_to_orgmode()` block walk in `markdown_convert.rs`: paragraphs, headings-as-bold-lines, blockquotes, thematic breaks, with the raw-text fallback path.
  - [ ] 2.2 Implement list emission: nested unordered/ordered lists, `start` honored, indentation per spec, multi-paragraph (loose) items.
  - [ ] 2.3 Implement table emission with the `|---+---|` header separator, using `inline_runs` → org emphasis inside cells; escape literal `|` inside cell text (org has no in-cell pipe escape — replace with `\vert` or `¦`) so a cell cannot break the row.
  - [ ] 2.4 Implement code emission (`#+begin_src <lang>` / `#+begin_example`, comma-escaping of `*`/`#+` lines) and the headline guard applied to all emitted body lines.
  - [ ] 2.5 In `text_export.rs`: call `markdown_to_orgmode(&trans.response)` / `markdown_to_orgmode(&resp.response)` in the two org-mode branches; remove the src-block wrappers and `markdown_bullets_for_org`.
  - [ ] 2.6 Update existing tests (`gloss_orgmode_*`, `chat_orgmode_*` assert `#+begin_src markdown`) to assert converted org output; add emitter unit tests: each construct, nested list indentation, table with bold cell, code block with/without lang + comma-escape, headline guard, pipe-in-cell escaping, and the raw-text fallback (exercised directly — see Notes on `to_mdast` never failing).
  - [ ] 2.7 Run `cd backend && cargo test`.

### 3.0 DOCX emitter and integration

**Specs / dependencies:** Depends on 1.0; independent of 2.0 (but sequenced after). Public API: `pub fn markdown_to_docx_body(text: &str) -> String` returning OOXML `<w:p>`/`<w:tbl>` fragments in the string-concatenation idiom of `docx_export.rs`; on parse failure, one `BodyText` paragraph per non-empty raw line (current behavior). Mapping (PRD req. 9–11):

- Runs: extend/duplicate `run()` to support `code` (adds `<w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Courier New"/>`) alongside `w:b`/`w:i`; all text through `escape_xml`.
- Headings → `BodyText` paragraph with all runs forced bold (no Word Heading styles).
- Lists → `BodyText` paragraphs with literal prefixes `- ` / `N. ` (honor `start`) and `<w:ind w:left="…"/>` per nesting level (e.g. 360 twips × level).
- Tables → generalize `format_vocab_table` into a builder that takes rows of cell-run-lists (keep the vocab table calling the same builder or leave it untouched — decide in-code, but no duplicate border boilerplate); auto width, header row bold.
- Code blocks → one monospace `BodyText` paragraph per line (blank lines preserved as empty paragraphs); inline code → monospace run.
- Links → runs for `text (url)`; blockquotes → `BodyText` with `w:ind`; `ThematicBreak` → paragraph with `<w:pBdr><w:bottom …/></w:pBdr>`.
- Integration: in `format_paragraph` (AI translations, lines ~162–165) and the chat assistant loop (lines ~129–132), replace the plain-line loops with `markdown_to_docx_body(&…response)`. User messages / Pāli text unchanged.
- Note the existing paragraph helpers take `&[String]` runs — the emitter can reuse `styled_paragraph` but needs an indent-capable variant.

- [ ] 3.0 DOCX emitter and integration
  - [ ] 3.1 Extend the run/paragraph helpers in `docx_export.rs` (or expose them to `markdown_convert.rs` — prefer keeping OOXML emission in `docx_export.rs` and having it call the shared AST walk; decide by what keeps `markdown_convert.rs` free of OOXML details vs. duplication): code-font run support and an indented-paragraph variant.
  - [ ] 3.2 Implement the block walk → OOXML: paragraphs, bold-run headings, blockquote indent, thematic break border paragraph, raw-line fallback.
  - [ ] 3.3 Implement list emission: literal `- `/`N. ` prefixes, `start` honored, per-level `w:ind` indentation, loose items.
  - [ ] 3.4 Generalize the bordered-table builder; emit markdown tables with bold header row and emphasis-capable cells; parameterize the cell paragraph style (the vocab table uses `VocabEntry`, markdown tables should use `BodyText`); keep the vocab table rendering identical (shared builder or unchanged code, no duplicated `w:tblBorders` boilerplate).
  - [ ] 3.5 Implement code emission: inline code runs and per-line monospace code-block paragraphs with blank lines preserved.
  - [ ] 3.6 Wire into `format_paragraph` and the chat assistant response loop, replacing the plain-text line loops.
  - [ ] 3.7 Tests: unit tests asserting on the generated fragment/`document.xml` string — `**bold**` yields `<w:b/>` and no literal `**`; nested list has increasing `w:ind` and correct prefixes; table markdown yields `<w:tbl>` with bold header runs; code block yields Consolas runs; `<`/`&` in response text arrives XML-escaped; the raw-text fallback path (exercised directly) yields plain paragraphs; existing gloss/chat docx tests still pass.
  - [ ] 3.8 Run `cd backend && cargo test`.

### 4.0 Final verification and docs

**Specs / dependencies:** Depends on 1.0–3.0 all complete.

- [ ] 4.0 Final verification and docs
  - [ ] 4.1 Run `cd backend && cargo test` (full suite) and `make build -B`; confirm clean (ignore pre-existing unrelated failures per user preference).
  - [ ] 4.2 Update `PROJECT_MAP.md` with `backend/src/markdown_convert.rs`; if `docs/gloss-prompts-history.md` or gloss export docs describe the org/docx export formatting, update the relevant sentences.
