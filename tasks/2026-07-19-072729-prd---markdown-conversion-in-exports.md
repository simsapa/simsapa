# PRD: Markdown Conversion in Exports (Org-Mode and DOCX)

## Introduction / Overview

AI model responses shown in the Gloss tab ("AI Translations" sections) and in
the Prompts tab (assistant chat responses) arrive as **Markdown**. In the UI
they are rendered by converting to HTML (`markdown_to_html` in
`backend/src/prompt_utils.rs`). The export paths, however, do not consistently
convert this Markdown to the target format:

- **HTML export** (`backend/src/text_export.rs`) — already converts via
  `markdown_to_html`. No change needed.
- **Markdown export** — the response is already Markdown; passed through. No
  change needed.
- **Org-Mode export** — currently wraps the raw Markdown response in a
  `#+begin_src markdown … #+end_src` block (with only a `* ` → `- ` bullet
  fix-up via `markdown_bullets_for_org`). The user ends up with a large
  Markdown chunk in the middle of an Org document that they must convert by
  hand.
- **DOCX export** (`backend/src/docx_export.rs`) — responses are flattened to
  plain-text `BodyText` paragraphs, one per non-empty line. All Markdown
  syntax (`**bold**`, `| tables |`, `- lists`) appears literally in the Word
  document.

**Goal:** convert the Markdown content of AI/assistant responses into the
target format at export time — real Org-Mode markup in `.org` exports, and
real formatted paragraphs/runs/tables in `.docx` exports — so the exported
document is uniformly in its own format with no embedded Markdown.

## Goals

1. Org-Mode exports contain no `#+begin_src markdown` wrapper blocks around AI
   responses; the response content is genuine Org-Mode markup.
2. DOCX exports render AI responses with formatting (bold, italic, lists,
   tables, code, quotes) instead of literal Markdown syntax characters.
3. Both converters share a single parse of the response (one Markdown AST, two
   emitters) so their feature coverage cannot drift apart.
4. Conversion is lossless enough for typical LLM responses: any construct
   outside the supported set degrades gracefully to readable plain text, never
   to dropped content or a crashed export.

## User Stories

1. As a user exporting a glossed passage with AI translations to Org-Mode, I
   want the translation sections to be native Org markup (headings as bold
   text, `- ` lists, `|…|` tables, `/italic/`, `*bold*`) so I can use the file
   in Emacs directly without hand-converting a Markdown island.
2. As a user exporting a Prompts-tab chat session to DOCX, I want assistant
   responses to appear as formatted text — bold section titles, bulleted and
   numbered lists, real Word tables — so the document is presentable without
   manual cleanup.
3. As a user whose AI response contains an unusual Markdown construct (e.g.
   footnote syntax, HTML fragment), I still get all the text content in the
   export, rendered as plain text rather than lost or crashing the export.

## Functional Requirements

### Shared conversion core

1. The system must parse each AI/assistant response **once** into an AST using
   the existing `markdown` crate's `to_mdast()` (no new parser dependency),
   and drive both the Org-Mode emitter and the DOCX emitter from that AST.
2. The conversion module must live in the backend (e.g. a new
   `backend/src/markdown_convert.rs` or similar), unit-testable without Qt.
3. The existing pre-processing that strips code fences wrongly wrapped around
   Markdown tables (the two regexes in `markdown_to_html`) must also be
   applied before parsing for export conversion, so tables an LLM wrapped in
   ``` fences still convert as tables. This pre-processing should be factored
   so it is shared, not duplicated.
4. Supported Markdown constructs (minimum set):
   - Bold (`**`), italic (`*`/`_`), and bold-italic runs.
   - Headings (`#` through `######`).
   - Unordered and ordered lists, **including nested lists** (list inside a
     list item), with correct indentation.
   - Tables (GFM pipe tables), including inline emphasis inside cells.
   - Inline code (`` ` ``) and fenced code blocks (```), preserving the
     fence's language tag where the target format can carry it.
   - Links `[text](url)`.
   - Blockquotes (`>`) and horizontal rules (`---`).
   - Paragraphs and hard/soft line breaks.
5. Unknown or unsupported AST node types must fall back to emitting their
   plain-text content (children flattened to text). The export must never
   panic or return an error because of response content; on a catastrophic
   parse failure the raw response text is emitted as-is (mirroring the
   fallback behavior of `markdown_to_html`).

### Org-Mode emitter

6. The Org-Mode export of Gloss AI translations and Prompts chat responses
   must replace the current `#+begin_src markdown` block with converted
   Org-Mode content (the `markdown_bullets_for_org` helper and the src-block
   wrapper are removed/retired for this path).
7. Construct mapping:
   - `**bold**` → `*bold*`; `*italic*` → `/italic/`; inline code → `~code~`.
   - Headings inside a response must **not** become Org headline stars
     (`*`-prefixed lines), because they would interleave with the document's
     own outline structure (`***`/`****` section headings already emitted by
     the exporter). Render them as bold standalone lines (`*Heading text*`)
     instead.
   - Unordered lists → `- item`; ordered lists → `1. item`, honoring a
     non-1 `start` value from the source Markdown; nested lists indented by
     the Org convention (aligned under the parent item's text).
   - Tables → Org tables (`| a | b |` with a `|---+---|` separator row after
     the header row).
   - Fenced code blocks → `#+begin_src <lang>` / `#+end_src` when a language
     tag is present (use the tag as-is, even if Org may not know it), or
     `#+begin_example` when there is none. Lines inside that begin with `*`
     or `#+` must be escaped per Org convention (leading comma: `,*`, `,#+`).
   - Blockquotes → `#+begin_quote` / `#+end_quote`.
   - Links → `[[url][text]]`.
   - Horizontal rule → `-----`.
8. Emitted Org text must not accidentally create headlines: any generated line
   that would start with `*` followed by a space (e.g. from bold at line
   start) must be prevented from being parsed as a headline (e.g. emit a
   zero-width/leading escape or restructure the line).

### DOCX emitter

9. The DOCX export of Gloss AI translations and Prompts chat responses must
   render the converted AST instead of the current plain-text line loop.
10. Construct mapping (within the existing hand-built OOXML approach in
    `docx_export.rs`):
    - Bold/italic/bold-italic → `w:r` runs with `w:b`/`w:i` (the existing
      `run(bold, italic, text)` helper, extended as needed).
    - Headings inside a response → **bold `BodyText` paragraphs**, not Word
      Heading styles, so they never appear in the document outline/TOC or
      compete with the exporter's own `Heading1`–`Heading3` structure.
    - Lists → literal-text prefixes: `- ` (or `• `) for bullets, `1. `, `2. `
      for ordered items (honoring a non-1 `start` value), with per-level
      indentation (e.g. `w:ind` left indent per nesting level). No
      `numbering.xml` machinery.
    - Tables → real Word tables, reusing/generalizing the existing
      `format_vocab_table` OOXML table builder; cell content supports inline
      emphasis runs.
    - Inline code and code blocks → monospace runs/paragraphs: set
      `w:rFonts` to `Consolas` (with `Courier New` as fallback) on the run;
      code blocks as one paragraph per line, preserving blank lines within
      the block.
    - Links → the link text followed by the URL in parentheses as plain text
      (`text (url)`); a native `w:hyperlink` (which requires a rels entry) is
      not required.
    - Blockquotes → indented `BodyText` paragraphs (via `w:ind`).
    - Horizontal rule → a paragraph with a bottom border (`w:pBdr`), or an
      empty spacer paragraph if simpler — must be visually distinct.
11. All text placed into OOXML must go through the existing `escape_xml`
    helper; the converter must not open an XML-injection path via response
    content.

### Integration points

12. `text_export.rs`: the Org-Mode branches for Gloss AI translations
    (`gloss_paragraph_orgmode` path) and chat responses (`chat_message`
    org path) call the new Org emitter. HTML and Markdown branches are
    unchanged.
13. `docx_export.rs`: `format_paragraph` (AI translations) and the chat
    message loop call the new DOCX emitter for response content. User
    messages and Pāli paragraph text remain plain text as today.
14. Existing unit tests in `text_export.rs` that assert the
    `#+begin_src markdown` wrapper must be updated to assert the converted
    Org output instead.

## Non-Goals (Out of Scope)

1. No conversion changes for the HTML and Markdown export targets (already
   correct).
2. No Markdown rendering of **user**-authored chat messages or Pāli source
   text — only model responses (Gloss `ai_translations[].response`, chat
   `responses[].response`).
3. No native Word list numbering (`numbering.xml` / `w:numPr`).
4. No native Word hyperlinks (relationship entries).
5. No new template styles; the embedded DOCX template is unchanged (any new
   formatting is inline run/paragraph properties).
6. No support for exotic Markdown: footnotes, definition lists, task-list
   checkboxes, images, raw HTML blocks — these fall back to plain text per
   requirement 5.
7. No changes to the UI rendering path (`markdown_to_html` behavior in-app).

## Technical Considerations

- The `markdown` crate (markdown-rs 1.0) is already a backend dependency;
  `to_mdast()` with GFM options (needed for tables) is the parse entry point.
  Verify the same `Options`/constructs used by `markdown_to_html` (it enables
  table support) are used for `to_mdast`, so UI and exports see the same
  parse.
- The mdast node set relevant here: `Root`, `Paragraph`, `Heading`, `Text`,
  `Strong`, `Emphasis`, `InlineCode`, `Code`, `List`/`ListItem`, `Table`/
  `TableRow`/`TableCell`, `Link`, `Blockquote`, `ThematicBreak`, `Break`.
- Suggested shape: an intermediate walk that flattens inline children into
  styled runs `(text, bold, italic, code)` — both emitters need exactly that
  for paragraphs, list items, and table cells.
- `docx_export.rs` builds `word/document.xml` by string concatenation around
  an embedded template; keep the new emitter in that idiom (return OOXML
  `String` fragments) rather than introducing a docx crate.
- Keep the shared structs in `export_types.rs` untouched — this feature only
  changes how `response` strings are rendered.
- Per project convention, this is backend-only Rust work: unit tests in the
  new module plus updated tests in `text_export.rs`; a DOCX test can assert
  on the generated `document.xml` string content (e.g. that `**bold**` became
  `<w:b/>` and no literal `**` remains).

## Success Metrics / Acceptance

1. Exporting a Gloss session with an AI translation containing headings, bold,
   nested lists, and a table to `.org` yields a file with no
   `#+begin_src markdown` and no literal Markdown syntax; opening it in Emacs
   Org-Mode shows correct emphasis, lists, and an aligned table; the document
   outline contains only the exporter's own headings.
2. Exporting the same session to `.docx` and opening in LibreOffice/Word shows
   bold/italic text, indented bullet and numbered lists, and a real table; no
   literal `**`, `|`, or `#` syntax remains; the Word outline (View →
   Outline) shows only the exporter's structural headings.
3. The same applies to a Prompts-tab chat export for assistant responses.
4. `cd backend && cargo test` passes with new tests covering: each supported
   construct in both emitters, nested list indentation, table with emphasis
   in cells, the code-fence-wrapped-table pre-processing, org headline
   escaping, XML escaping of `<`/`&` in DOCX output, and the plain-text
   fallback for an unsupported construct.

## Open Questions

None — all resolved:

1. **Code-block language tags:** use `#+begin_src <lang>` whenever a tag is
   present (even for languages Org may not recognize); `#+begin_example`
   only when there is no tag. (Folded into requirement 7.)
2. **DOCX monospace font:** `Consolas`, with `Courier New` as fallback.
   (Folded into requirement 10.)
3. **Ordered-list start values:** honor the mdast `start` value in both
   emitters. (Folded into requirements 7 and 10.)
