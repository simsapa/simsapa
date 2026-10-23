# Glossing and Extracting Words with Context: Algorithm Documentation

**Status:** Current implementation (as of 2025-10-12)  
**Test Coverage:** 156/156 backend tests pass  
**Implementation:** `backend/src/helpers.rs`

How this extraction fits with the search and dictionary handling of iti sandhi
and quote marks: [pali-sandhi-and-quote-marks.md](./pali-sandhi-and-quote-marks.md).

## Overview

The word extraction and glossing system extracts individual words from Pāli texts, finds their positions in the original text, and generates context snippets with HTML highlighting. This is used for:

- **GlossTab.qml**: Generating vocabulary glosses with context
- **Anki CSV Export**: Creating flashcards with sentence context
- **Word Lookup**: Providing click-to-define functionality

## Core Function

```rust
pub fn extract_words_with_context(text: &str) -> Vec<GlossWordContext>
```

**Input:** Raw Pāli text with diacritics, punctuation, and sandhi patterns

**Output:** Vector of word contexts:
```rust
pub struct GlossWordContext {
    pub clean_word: String,      // Normalized word for dictionary lookup
    pub original_word: String,   // Word as it appears in text (preserves sandhi)
    pub context_snippet: String, // Sentence context with <b>word</b> tags
}
```

## Algorithm Architecture

The algorithm uses a **5-stage pipeline** with character-based position tracking to avoid Unicode byte/character confusion:

```
┌─────────────────────────────────────────────────────────────┐
│ Stage 1: Text Preprocessing                                  │
│ ─────────────────────────────────────────────────────────── │
│ Input:  Original Pāli text                                   │
│ Output: Preprocessed text with sandhi splits                 │
│ Action: dhārayāmī'ti → dhārayāmi ti                          │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ Stage 2: Clean Word Extraction                               │
│ ─────────────────────────────────────────────────────────── │
│ Input:  Preprocessed text                                    │
│ Output: List of clean words (tokens)                         │
│ Action: ["dhārayāmi", "ti", "sikkhāpadesu", ...]            │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ Stage 3: Word Position Finding (in original text)            │
│ ─────────────────────────────────────────────────────────── │
│ Input:  Clean word + original text + search position         │
│ Output: WordPosition {char_start, char_end, original_word}   │
│ Action: Find dhārayāmi → match dhārayāmī'ti (sandhi-aware)  │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ Stage 4: Context Boundary Calculation                        │
│ ─────────────────────────────────────────────────────────── │
│ Input:  Word position + original text                        │
│ Output: ContextBoundaries {context_start, context_end, ...}  │
│ Action: Find sentence boundaries (. ? ! ;)                   │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ Stage 5: Context Snippet Building                            │
│ ─────────────────────────────────────────────────────────── │
│ Input:  Original text + boundaries                           │
│ Output: HTML snippet with bold tags                          │
│ Action: "word1 <b>target_word</b> word2"                     │
└─────────────────────────────────────────────────────────────┘
```

## Stage 1: Text Preprocessing

**Function:** `preprocess_text_for_word_extraction(text: &str) -> String`

**Purpose:** Transform Pāli sandhi patterns to expose individual words for extraction.

### Sandhi Transformations

Pāli uses sandhi (euphonic combinations) where word boundaries are obscured:

| Original Pattern | Preprocessed | Components |
|-----------------|--------------|------------|
| `dhārayāmī'ti` | `dhārayāmi ti` | word + quotative marker |
| `passāmī"ti` | `passāmi ti` | (curly quotes supported) |
| `dassanāyā'ti` | `dassanāya ti` | long ā → short a |
| `sikkhāpadesū'ti` | `sikkhāpadesu ti` | long ū → short u |
| `gantun'ti` | `gantuṁ ti` | n → niggahita ṁ |
| `vilapi"nti` | `vilapiṁ ti` | "nti → ṁ + ti |
| `Manopubbaṅ’gamā` | `manopubbaṅgamā` | apostrophe inside a compound: joined |
| `so’pi` | `sopi` | elision: joined (DPD: so + api) |
| `cā”tipi` | `cātipi` | iti + the next word: joined (DPD: ca + iti + api) |

The iti rules match only at the end of a word. Words written with quote marks
inside them are joined by `join_mid_word_apostrophes`, after the iti split, and
found in the original text by `match_with_inner_apostrophes`. See
[pali-sandhi-and-quote-marks.md](./pali-sandhi-and-quote-marks.md) §3–5.

### Implementation Details

```rust
// Vowel + quote + ti patterns
let text = RE_IITI_BEFORE.replace_all(&text, "i ti");  // ī'ti → i ti
let text = RE_AATI_BEFORE.replace_all(&text, "a ti");  // ā'ti → a ti
let text = RE_UUTI_BEFORE.replace_all(&text, "u ti");  // ū'ti → u ti

// Niggahita + quote + nti patterns  
let text = RE_NTI_BEFORE.replace_all(&text, "ṁ ti");   // n'ti → ṁ ti
```

**Quote Support:** Single (`'`, U+2019), double (`"`, U+201D), and curly quotes.

**Additional Processing:**
- Join quote marks inside a word (`join_mid_word_apostrophes`)
- Remove non-word characters (punctuation)
- Remove digits
- Normalize whitespace

## Stage 2: Clean Word Extraction

**Function:** `extract_clean_words(preprocessed_text: &str) -> Vec<String>`

**Purpose:** Split preprocessed text into individual word tokens.

**Implementation:**
```rust
preprocessed_text
    .split_whitespace()
    .map(|s| s.to_string())
    .collect()
```

**Output:** `["dhārayāmi", "ti", "sikkhāpadesu", "ti", ...]`

## Stage 3: Word Position Finding

**Function:** `find_word_position(..., iti_follows) -> Option<WordPosition>`
(`find_word_position_char_based()` is the public form, with `iti_follows = false`).

`iti_follows` is true when the next clean word is `ti`: Stage 1 split an iti
sandhi off this word, which the text may write without a quote mark
(`viharāmīti`, `abhinandunti`). Methods 1 and 2 below then also accept a bare
`ti`. Only with the flag: otherwise `pi` would match inside `pīti`.

**Purpose:** Find where each clean word appears in the **original** (untransformed) text.

### Key Challenge: Sandhi Reversal

The clean word from Stage 2 may not exist as-is in the original text:
- **Clean word:** `dhārayāmi` (after preprocessing)
- **Original text:** `dhārayāmī'ti` (before preprocessing)

The algorithm must **intelligently match** across these transformations.

### Character-Based Approach

**Critical Design Decision:** All position tracking uses **character indices**, never byte indices.

**Why this matters:**
```
Text:    "jānāmi"
Bytes:   [106, 195, 129, 110, 195, 129, 109, 105]  // ā = 2 bytes (UTF-8)
Chars:   ['j', 'ā', 'n', 'ā', 'm', 'i']            // ā = 1 character
         0    1    2    3    4    5
```

Mixing byte and character positions causes position drift with Unicode diacritics.

### Matching Strategy

The algorithm tries **three matching methods** in order:

#### 1. Vowel+Ti Expansion Matching

Handles patterns like `dhārayāmī'ti` where long vowel + quote + ti becomes short vowel:

```rust
fn try_match_with_vowel_ti_expansion(
    original_chars: &[char],
    search_chars: &[char],
    char_pos: usize,
) -> Option<(usize, usize, String)>
```

**Logic:**
1. Check if search word ends with short vowel (a/i/u)
2. Try matching with long vowel (ā/ī/ū) + quote chars + 'ti' in original, or a
   bare 'ti' (`viharāmīti`) when `iti_follows`
3. If match found, return full sandhi unit: `dhārayāmī'ti`

#### 2. Niggahita Expansion Matching

Handles patterns like `gantun'ti` → `gantuṁ ti`:

```rust
fn try_match_with_niggahita_expansion(
    original_chars: &[char],
    search_chars: &[char],
    char_pos: usize,
) -> Option<(usize, usize, String)>
```

**Logic:**
1. Check if search word ends with `ṁ` or `ṃ`
2. Try matching prefix + 'n' + quotes + 'ti' or 'nti' in original, or prefix +
   'nti' with no quote (`abhinandunti`) when `iti_follows`
3. Handles multiple consecutive quotes: `gantun'"ti`
4. Returns full sandhi unit: `gantun'ti`

#### 3. Sandhi-Aware Vowel Matching

Handles simple vowel length variations without quote patterns:

```rust
fn slice_matches_with_sandhi(
    original_slice: &[char],
    search_chars: &[char]
) -> bool
```

**Logic:**
- Character-by-character comparison
- Treats long/short vowel pairs as equivalent: ā↔a, ī↔i, ū↔u
- Used when exact match fails but vowel variation might work

### Sandhi Unit Detection

After finding a basic match, check if it's part of a larger sandhi unit:

```rust
fn detect_sandhi_unit(
    original_chars: &[char],
    search_word: &str,
    match_start: usize,
    match_end: usize,
) -> Option<(usize, usize)>
```

**Detection Logic:**
- Check if word followed by a run of quote char(s) + 'ti' or 'nti'
  (`parāyaṇo’”ti`, a quote closing inside a quote)
- If found, expand boundaries to include entire unit
- Returns adjusted `(start, end)` positions

### Sequential Search with Position Tracking

```rust
let mut current_search_pos = 0;

for clean_word in clean_words {
    if let Some(word_position) = find_word_position_char_based(
        &original_chars,
        &original_lower_chars,
        &clean_word,
        current_search_pos,  // ← Start search from here
    ) {
        // Process word...
        current_search_pos = word_position.char_end;  // ← Advance position
    }
}
```

**Critical Feature:** Sequential search **guarantees** repeated words are found in order.

**Trap: a word that is not placed at its spot costs the rest of the
paragraph.** The search does not stop at the next word: it finds the clean
word's *next* occurrence further on and advances past everything in between.
Those words can then no longer be found, and each one takes the fallback branch
(`original_word` = the clean word, a context with no `<b>`). Any spelling that
Stage 1 rewrites must be matchable here, which is why the unquoted iti and the
inter-word hyphen (`Todeyya-kappā`, matched by `match_with_inner_apostrophes`)
are handled. `test_extract_words_with_context.rs` asserts that every token of a
real passage is placed.

### Handling Standalone 'ti' Particles

**Problem:** After sandhi expansion, both the full unit and standalone 'ti' appear in clean_words:
- Preprocessed: `dhārayāmī'ti` → `dhārayāmi ti`
- Clean words: `["dhārayāmi", "ti", ...]`
- Original text: `dhārayāmī'ti` (only one instance)

If we search for standalone `ti`, it matches the `'ti` already consumed by the sandhi unit, causing position tracking errors.

**Solution:** Skip standalone `ti` particles after sandhi units, quoted or, when
`iti_follows` matched it, bare:

```rust
let mut skip_next_ti = false;

for clean_word in clean_words {
    // Skip if previous word was sandhi unit ending in 'ti
    if skip_next_ti && clean_word == "ti" {
        skip_next_ti = false;
        continue;
    }
    
    // ... find word position ...
    
    // Check if word is sandhi unit ending in quote+'ti or quote+'nti
    let orig_lower = word_position.original_word.to_lowercase();
    if orig_lower.ends_with("'ti") || orig_lower.ends_with("\"ti")
        || orig_lower.ends_with("'nti") || orig_lower.ends_with("\"nti")
        || orig_lower.ends_with("\u{2019}ti") || orig_lower.ends_with("\u{201D}ti")
        || orig_lower.ends_with("\u{2019}nti") || orig_lower.ends_with("\u{201D}nti")
    {
        skip_next_ti = true;  // Skip next standalone 'ti
    }
}
```

**Rationale:** The quotative particle `ti` (abbreviation of `iti`) is not useful for vocabulary extraction—it's a grammatical marker, not a content word.

## Stage 4: Context Boundary Calculation

**Function:** `calculate_context_boundaries(...) -> ContextBoundaries`

**Purpose:** Determine the span of text to include in the context snippet.

### Boundary Types

```rust
pub struct ContextBoundaries {
    pub context_start: usize,  // Where context begins
    pub context_end: usize,    // Where context ends
    pub word_start: usize,     // Where word begins (for bold tag)
    pub word_end: usize,       // Where word ends
}
```

### Sentence Boundary Detection

```rust
fn find_sentence_start(text: &str, word_start: usize) -> usize
fn find_sentence_end(text: &str, word_end: usize) -> usize
```

**Sentence delimiters:** `. ` (period+space), `?`, `!`, `;` (semicolon)

**Logic:**
1. Search backward from word position for sentence start
2. Search forward from word position for sentence end
3. If no boundary found, use text boundaries (0 or text length)

### Context Window

If sentence boundaries are too far (or not found):
- **Before word:** Up to 50 characters
- **After word:** Up to 50 characters

**Final boundaries:**
```rust
let context_start = sentence_start.max(word_start - 50);
let context_end = sentence_end.min(word_end + 50);
```

### Examples

**Case 1: Word mid-sentence**
```
Text: "Katamañca, bhikkhave, samādhindriyaṁ? Idha bhikkhave..."
Word: bhikkhave (at position 12)
Result: context_start=0, context_end=40 (sentence boundary at '?')
```

**Case 2: Multiple sentences**
```
Text: "Sentence 1. Sentence 2 contains word here. Sentence 3."
Word: word (at position 30)
Result: context="Sentence 2 contains word here." (trimmed to sentence)
```

## Stage 5: Context Snippet Building

**Function:** `build_context_snippet(chars: &[char], boundaries: &ContextBoundaries) -> String`

**Purpose:** Extract context substring and wrap target word in `<b>` tags.

### Algorithm

```rust
// 1. Extract context substring
let context_slice: String = chars[context_start..context_end].iter().collect();

// 2. Calculate relative word position within context
let relative_word_start = word_start - context_start;
let relative_word_end = word_end - context_start;

// 3. Split context into: before + word + after
let context_chars: Vec<char> = context_slice.chars().collect();
let before: String = context_chars[..relative_word_start].iter().collect();
let word: String = context_chars[relative_word_start..relative_word_end].iter().collect();
let after: String = context_chars[relative_word_end..].iter().collect();

// 4. Build HTML snippet
format!("{}<b>{}</b>{}", before, word, after)
```

### Examples

**Input:**
- Original text: `"tucchaṁ musā vilapi"nti, aññatra"`
- Word position: `vilapi"nti` at chars 12-22
- Context boundaries: 0-32

**Output:**
```html
tucchaṁ musā <b>vilapi"nti</b>, aññatra
```

**Note:** Only the **first occurrence** in the context is bolded, even if the word appears multiple times.

## Helper Functions

### Character Type Detection

```rust
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() 
        || c == 'ā' || c == 'ī' || c == 'ū' 
        || c == 'ṁ' || c == 'ṃ'
        || c == 'ṅ' || c == 'ñ' 
        || c == 'ṭ' || c == 'ḍ' || c == 'ṇ' || c == 'ḷ'
}
```

**Purpose:** Identify Pāli word characters including diacritics.

### Quote Character Detection

```rust
fn is_quote_char(c: char) -> bool {
    matches!(c, '"' | '\u{201C}' | '\u{201D}' | '\'' | '\u{2018}' | '\u{2019}')
}

fn skip_quote_chars(chars: &[char], pos: usize) -> usize {
    let mut current_pos = pos;
    while current_pos < chars.len() && is_quote_char(chars[current_pos]) {
        current_pos += 1;
    }
    current_pos
}
```

**Purpose:** Handle various quote styles and multiple consecutive quotes (e.g., `'"nti`).

### Sandhi Vowel Normalization

```rust
fn normalize_sandhi_vowel(c: char) -> char {
    match c {
        'ā' => 'a',
        'ī' => 'i',
        'ū' => 'u',
        _ => c,
    }
}
```

**Purpose:** Normalize vowels for fuzzy matching across sandhi boundaries.

## Complete Algorithm Flow

### Example: Processing "dhārayāmī'ti sikkhāpadesū'ti"

**Stage 1: Preprocessing**
```
Input:  "dhārayāmī'ti sikkhāpadesū'ti"
Output: "dhārayāmi ti sikkhāpadesu ti"
```

**Stage 2: Word Extraction**
```
Clean words: ["dhārayāmi", "ti", "sikkhāpadesu", "ti"]
```

**Stage 3: Position Finding (in original text)**

Loop iteration 1:
- **Search for:** `dhārayāmi`
- **Try vowel+ti expansion:** Match `dhārayāmī'ti` ✓
- **Result:** WordPosition {
    - clean_word: "dhārayāmi"
    - original_word: "dhārayāmī'ti"
    - char_start: 0, char_end: 13
  }
- **Detect sandhi ending:** ends_with("'ti") → skip_next_ti = true
- **Advance position:** current_search_pos = 13

Loop iteration 2:
- **Search for:** `ti`
- **Skip:** skip_next_ti == true, continue to next word
- **Advance position:** unchanged (still 13)

Loop iteration 3:
- **Search for:** `sikkhāpadesu`
- **Try vowel+ti expansion:** Match `sikkhāpadesū'ti` ✓
- **Result:** WordPosition {
    - clean_word: "sikkhāpadesu"
    - original_word: "sikkhāpadesū'ti"
    - char_start: 14, char_end: 30
  }
- **Detect sandhi ending:** ends_with("'ti") → skip_next_ti = true
- **Advance position:** current_search_pos = 30

Loop iteration 4:
- **Search for:** `ti`
- **Skip:** skip_next_ti == true, done

**Final Output:**
```rust
vec![
    GlossWordContext {
        clean_word: "dhārayāmi",
        original_word: "dhārayāmī'ti",
        context_snippet: "<b>dhārayāmī'ti</b> sikkhāpadesū'ti"
    },
    GlossWordContext {
        clean_word: "sikkhāpadesu",
        original_word: "sikkhāpadesū'ti",
        context_snippet: "dhārayāmī'ti <b>sikkhāpadesū'ti</b>"
    }
]
```

**Note:** Standalone `ti` particles are skipped entirely—they don't appear in the output.

## Edge Cases and Error Handling

### 1. Word Not Found

If `find_word_position_char_based()` returns `None`:
```rust
results.push(GlossWordContext {
    clean_word: clean_word.clone(),
    original_word: clean_word.clone(),  // Fallback to clean form
    context_snippet: snippet,            // Context around current position
});
// Don't advance current_search_pos
```

### 2. Repeated Words

**Sequential search guarantees correct handling:**

```
Text: "iti jānāmi, iti passāmi"
Words: ["iti", "jānāmi", "iti", "passāmi"]

Iteration 1: Find "iti" at position 0
Iteration 2: Find "jānāmi" at position 4, advance to 11
Iteration 3: Find "iti" at position 13 (not position 0!)
Iteration 4: Find "passāmi" at position 17
```

Each occurrence gets **unique context**:
- First "iti": `<b>iti</b> jānāmi, iti passāmi`
- Second "iti": `iti jānāmi, <b>iti</b> passāmi`

### 3. Text Boundaries

**Word near start:**
```rust
if word_start < 50 {
    context_start = sentence_start.max(0);  // Use whatever's available
}
```

**Word near end:**
```rust
if word_end + 50 > text_len {
    context_end = sentence_end.min(text_len);  // Use whatever's available
}
```

### 4. Unicode Diacritics

**All positions tracked as character indices:**
```rust
let chars: Vec<char> = original_text.chars().collect();
let text_len = chars.len();  // Character count, not byte count
```

**Slicing uses character positions:**
```rust
let original_word: String = chars[start..end].iter().collect();
```

### 5. Empty or Whitespace-Only Text

```rust
let original_text = text.trim();
if original_text.is_empty() {
    return Vec::new();
}
```

## Performance Characteristics

### Time Complexity

- **Preprocessing:** O(n) where n = text length
- **Word extraction:** O(w) where w = word count
- **Position finding:** O(n × w) - sequential search through text for each word
- **Context building:** O(w × c) where c = average context size

**Overall:** O(n × w) for typical texts

### Space Complexity

- Character arrays: O(n) for original text
- Word list: O(w × avg_word_length)
- Results: O(w × avg_context_length)

**Overall:** O(n + w × c)

### Optimization Notes

- Character-based matching is slower than byte-based string search
- **Correctness is prioritized over speed**
- For typical sutta passages (50-200 words), performance is instantaneous
- Could optimize with:
  - KMP/Boyer-Moore string matching for long texts
  - Caching character vectors
  - Parallel processing for large corpora

## Testing

### Test Coverage

**Test suites:**
- `test_extract_words_with_context.rs` - 19 tests
- `test_sandhi_split_detection.rs` - 3 tests
- `test_staged_word_extraction.rs` - 8 tests
- `test_dpd_lookup.rs` - 2 tests (integration)

**Total:** 156 backend tests pass

### Key Test Cases

**1. Repeated words (`test_repeated_words_no_skipping`)**
```rust
// 40-word Pārājika passage with repeated iti, jānāmi, passāmi, vā
// Validates: All words extracted, unique contexts, no position drift
```

**2. Sandhi patterns (`test_nti_sandhi_cases`)**
```rust
// Tests: gantun'ti, gantu'nti, gantun'"ti, dhārayāmī'ti, dassanāyā'ti
// Validates: Correct sandhi expansion, quote handling, bold tags
```

**3. Multiple consecutive sandhi (`test_multiple_sandhi_splits_in_sequence`)**
```rust
// Tests: dhārayāmī'ti sikkhāpadesū'ti gantun'ti
// Validates: Position tracking through multiple sandhi units
```

**4. Unicode diacritics (`test_pali_diacritics_preservation`)**
```rust
// Tests: sammādiṭṭhi, samādhindriyaṁ, bhikkhave
// Validates: Character position tracking, diacritic preservation
```

**5. Sentence boundaries (`test_semicolon_sentence_boundary`)**
```rust
// Tests: Context trimming at semicolons, periods, colons
// Validates: Correct boundary detection
```

## Common Issues and Solutions

### Issue 1: Position Drift with Unicode

**Symptom:** Words after diacritics have empty `original_word`

**Cause:** Mixing byte and character positions

**Solution:** Use character indices throughout:
```rust
// ✗ WRONG
current_search_pos = byte_pos + search_word.len();

// ✓ CORRECT
current_search_pos = word_position.char_end;
```

### Issue 2: Repeated Words Jump Back

**Symptom:** Second "iti" points to first "iti" position

**Cause:** Search always starts from position 0

**Solution:** Sequential search with position advancement:
```rust
for clean_word in clean_words {
    if let Some(pos) = find_word(..., current_search_pos) {
        current_search_pos = pos.char_end;  // Advance!
    }
}
```

### Issue 3: Sandhi Units Not Recognized

**Symptom:** `passāmi` found as `passāmi` instead of `passāmī'ti`

**Cause:** Missing expansion matching logic

**Solution:** Implement vowel+ti and niggahita+nti expansion:
```rust
// Try expansion matchers before simple match
if let Some(expanded) = try_match_with_vowel_ti_expansion(...) {
    return Some(expanded);
}
```

### Issue 4: Multiple Consecutive Sandhi Position Errors

**Symptom:** After `dhārayāmī'ti`, the word `sikkhāpadesū'ti` has no bold tags

**Cause:** Standalone `ti` from first sandhi matches the `'ti` suffix, advancing position past second word

**Solution:** Skip standalone `ti` after sandhi units ending in quote+ti

## Integration Points

### From QML (GlossTab.qml)

**Generating glosses:**
```qml
function generate_gloss(text) {
    let words = sutta_bridge.extract_words_with_context(text);
    for (let word of words) {
        // word.clean_word - for dictionary lookup
        // word.original_word - for display
        // word.context_snippet - for context display (has <b> tags)
    }
}
```

**Anki CSV export:**
```qml
function gloss_as_anki_csv(words) {
    for (let word of words) {
        let front = `<p>${word.clean_word}</p><p>${word.context_snippet}</p>`;
        let back = dictionary_lookup(word.clean_word);
        csv += `"${front}","${back}"\n`;
    }
}
```

### From Rust (sutta_bridge.rs)

**Bridge function:**
```rust
#[qinvokable]
pub fn extract_words_with_context(&self, text: QString) -> QVariantList {
    let text_str = text.to_string();
    let words = helpers::extract_words_with_context(&text_str);
    // Convert to QVariant for QML
    words_to_qvariant_list(words)
}
```

## Future Enhancements

### Potential Improvements

1. **Configurable context window:** Allow customization beyond ±50 characters
2. **More sandhi patterns:** Handle complex sandhi beyond current patterns
3. **Performance optimization:** KMP/Boyer-Moore for very long texts
4. **Metrics/telemetry:** Track extraction success rates
5. **Paragraph-aware context:** Respect paragraph boundaries in addition to sentences

### Known Limitations

**None currently:** The edge case with multiple consecutive sandhi splits has been fully resolved as of 2025-10-12.

## References

### Source Files

- **Implementation:** `backend/src/helpers.rs` (lines 431-990)
- **Tests:** `backend/tests/test_extract_words_with_context.rs`
- **Sandhi tests:** `backend/tests/test_sandhi_split_detection.rs`
- **Staged tests:** `backend/tests/test_staged_word_extraction.rs`

### Related Documentation

- Rust Unicode strings: [The Rust Book - Strings](https://doc.rust-lang.org/book/ch08-02-strings.html)
- Pāli orthography: Digital Pāli Dictionary documentation
- Qt/QML integration: CXX-Qt bridge documentation

### Git History

- Initial implementation: Bug fix for position drift (2025-10-11)
- Sandhi expansion: Added vowel+ti and niggahita+nti matching (2025-10-11)
- Curly quote support: Fixed Unicode quote recognition (2025-10-12)
- Skip standalone 'ti': Fixed multiple consecutive sandhi (2025-10-12)

---

**Last Updated:** 2025-10-12  
**Test Status:** 156/156 tests pass  
**Production Status:** Deployed and stable
