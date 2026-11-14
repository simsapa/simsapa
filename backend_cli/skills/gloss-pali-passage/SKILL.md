---
name: gloss-pali-passage
description: Make a word-by-word analysis document of a Pāli passage or sutta reference (such as dhp23, sn1.71 or pasted Pāli text) with the `simsapa` command line program - every word with its grammar, contextual meaning, definition, construction and root, checked word senses, and a Translation and a Literal Translation. Use when the user asks to gloss, analyse or translate a Pāli passage.
argument-hint: "[headwords|word-in-sentence] <Pāli text or reference>"
metadata:
  generator: simsapa-cli
  version: "{{version}}"
---

# Gloss a Pāli passage

Usage: `/gloss-pali-passage [headwords|word-in-sentence] <text or reference>`

- The first word is the gloss format when it is `headwords` or
  `word-in-sentence`; the default is `word-in-sentence`.
- The rest is a sutta reference (`dhp23`, `sn1.71`, `mn4`, `ud1`, `iti1`) or
  pasted Pāli text.

Examples: `/gloss-pali-passage dhp23`, `/gloss-pali-passage headwords sn1.71`,
`/gloss-pali-passage <pasted Pāli text>`.

**Make no LLM API request of your own, and do not run `simsapa gloss` without
`--no-ai`.** You are the AI: you choose the senses and write the translations
yourself, from the data `simsapa` prints.

## Steps

1. Check that the program works: `simsapa health`.
2. Get the deterministic table. Use `--format markdown` for the document and
   `--format json` when you need the candidates of a word:

   ``` sh
   simsapa gloss --no-ai --format markdown dhp23 > /tmp/gloss.md
   simsapa gloss --no-ai --format json dhp23     > /tmp/gloss.json
   ```

   Add `--gloss-format headwords` for the Headwords format, `--paragraphs 1-2`
   to select paragraphs of a reference, `--edition cst|ms` to choose the
   edition. For pasted text, pass it as the argument, or `-` and the text on
   stdin. Never edit the ID, Word in Sentence, Grammar, Construction or Root
   columns by hand: they come from the dictionary.
3. In the JSON, `session.paragraphs[n].rows` has one entry per word, in passage
   order, with `candidates` (the dictionary senses of that word, each with its
   `id`, `grammar_options`, `meaning_combo`, `construction`, `root`),
   `selected_candidate`, `grammar_index` and `confidence`. For every row marked
   `N` that has several candidates, read the word in its sentence, choose the
   sense that fits, and mark the row `A`. Take the row's ID, Grammar,
   Construction and Root from the chosen candidate, as written there. A row
   with an empty confidence had one candidate: leave it. Component rows
   (a leading `- `) are chosen the same way. Write a short Contextual value
   for every row with a sense (see the notes under the document structure),
   and keep the Definition as printed.
4. Write the **Translation** (natural, readable English) and the **Literal
   Translation** (close to word-by-word, following the Pāli word order and
   grammar where English allows), for each paragraph, from the chosen senses.
   These are the instructions of the app's own translation template; follow
   them:

   > You are a Pāli translator. Using the word senses in the table below,
   > translate the Pāli passage into English. Each word's sense in this
   > passage is its Contextual value, a rendering fitted to the sentence, or
   > its Definition when it has none. Do not introduce senses that are not
   > listed; a word without a meaning in the table you may translate as you
   > see fit. Component rows (starting with "- ") give the parts of the
   > compound or sandhi word above them.
   >
   > Return two translations: "translation", in natural, readable English; and
   > "literal_translation", close to word-by-word, following the Pāli word
   > order and grammar where English allows.
5. Write the document and save it as `<reference-or-slug>-gloss.md` in the
   current folder (for example `dhp23-gloss.md`), unless the user names a path.
   Tell the user the path.

## Document structure (Word in Sentence format)

``` text
# Gloss of: <reference or first words>

<the passage text>

### English Translation

**Translation:** <natural English>

**Literal Translation:** <word-by-word English>

### Variants            (only when the text has `//` spelling alternatives)

### Word-by-Word Analysis

| ID | Word in Sentence | Contextual / Grammar | Definition / Construction | Root | Conf. |
| :--- | :--- | :--- | :--- | :--- | :--- |
| 62145 | sātatikā | persevering | diligent; persevering; consistent; practising continuously |  |  |
|  |  | adj, masc nom pl of sātatika | [sa + √tan + ta + *ika] | √tan 7 o (stretch, extend) |  |
| 32215 | daḷhaparakkamā | of firm effort | having persistent effort; making continuous effort; striving diligently |  |  |
|  |  | adj, masc nom pl of daḷhaparakkama | [daḷha + parakkama] |  |  |
| 32201 | - daḷha | firm | strong; firm; steady; lit. become strong |  | A |
|  |  | pp | [√dah + ta] | √dah 1 a (be strong) |  |

- N: Not checked, plain dictionary lookup, no saved selection. The first of several meanings was chosen; may need manual review.
- A: AI-checked, picked by a machine (runtime AI request, or built-in data).
- H: Human-checked, confirmed by you, or by selections and set phrases built into the app.
```

- Keep `simsapa`'s own table and legend text as printed; the example above
  only shows the shape. The `ṁ` (niggahīta) is used in every column.
- Each word takes two table rows (the default, `--lines-per-word 2`): the
  first has the Contextual and the Definition, the second the Grammar, the
  Construction (in square brackets) and the Root. With `--lines-per-word 1`
  the header is `| ID | Word in Sentence | Grammar | Contextual | Definition |
  Construction | Root | Conf. |`, one row per word.
- Definition is the dictionary's text as `simsapa` printed it; never change
  it. Contextual is yours to fill: a short English rendering of the word as
  used in this sentence, following the case, number and person of its
  grammar (`of the nobleman`, `I would dwell`). It stands beside the
  definition, never in its place.
- Under a multi-paragraph passage, repeat the translation, variants and table
  for each paragraph under a `## Paragraph <n>` heading.
- A word that could not be found has an empty ID and "not found" as its
  Definition.
- Mark `A` only the rows you chose yourself. Leave `N` on a row you did not
  check, and `H` where `simsapa` printed it.
- Because you made the choices, do not write the sentence "No AI provider is
  enabled" that `simsapa` prints under the legend; replace it with one line:
  "Senses marked A and the translations were made by the agent that wrote this
  document."

## Headwords format

With `headwords`, run `simsapa gloss --no-ai --gloss-format headwords --format
markdown …`. The table lists the dictionary entries of the passage (each once
per paragraph) instead of every word as written. Choose senses for `N` rows and
write the two translations in the same way, and keep the layout that `simsapa`
printed.

## Errors

`simsapa` exits with a non-zero code and a message on stderr when a reference
does not resolve or the databases are missing (`simsapa health` says which). Say
so to the user; do not guess a text from memory.
