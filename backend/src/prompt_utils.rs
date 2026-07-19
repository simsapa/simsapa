use regex::Regex;
use lazy_static::lazy_static;

use markdown::{to_html_with_options, Options};
use crate::helpers::consistent_niggahita;

pub fn clean_prompt(text: &str) -> String {
    lazy_static! {
        // Regex to match code blocks fence pairs with language identifier anywhere in the text and remove only the identifier
        static ref RE_CODE_BLOCK_WITH_LANG: Regex = Regex::new(r"(?s)``` *(\w+)\s*\n(.*?)\n```").unwrap();
    }

    // Trim whitespace before processing
    let trimmed_text = text.trim();

    // Remove language identifier from ALL code blocks but keep the code block structure
    let processed_text = RE_CODE_BLOCK_WITH_LANG.replace_all(trimmed_text, "```\n$2\n```");

    // If the whole response is wrapped in a single pair of code fences,
    // strip them. LLMs sometimes wrap their entire markdown reply in a fence
    // even though the content inside is prose/headers/lists meant to render
    // as markdown, not as a literal code block.
    let unfenced = strip_outer_code_fences(&processed_text);
    let processed_text: std::borrow::Cow<str> = match unfenced {
        Some(s) => std::borrow::Cow::Owned(s),
        None => processed_text,
    };

    // Remove wrapping bold/italics syntax if present (only if entire text is wrapped)
    let final_text = {
        let patterns = ["***", "**", "*", "___", "__", "_"];

        // Try each pattern and return the first match
        patterns.iter()
            .find_map(|&pattern| {
                if processed_text.starts_with(pattern) && processed_text.ends_with(pattern) && processed_text.len() > pattern.len() * 2 {
                    Some(processed_text.trim_start_matches(pattern).trim_end_matches(pattern))
                } else {
                    None
                }
            })
            .unwrap_or(&processed_text)
    };

    // Apply consistent_niggahita to the cleaned text
    consistent_niggahita(Some(final_text.to_string()))
}

/// If the trimmed text is wrapped in a single pair of triple-backtick code
/// fences AND the content between them clearly looks like markdown (has
/// headers, list bullets, table rows, or blockquotes), return the unwrapped
/// content. Otherwise return None.
///
/// We deliberately do NOT strip fences around plain prose/verse — the UI
/// renders fenced blocks with sans-serif styling as a quoted-content
/// affordance, and we want to preserve that when the LLM actually intended
/// a code-block presentation.
fn strip_outer_code_fences(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let mut lines = trimmed.lines();
    let first = lines.next()?;
    if !first.trim_end().starts_with("```") {
        return None;
    }
    let rest: Vec<&str> = lines.collect();
    let (last, middle) = rest.split_last()?;
    if last.trim() != "```" {
        return None;
    }
    if middle.iter().any(|l| l.trim_start().starts_with("```")) {
        return None;
    }
    let looks_like_markdown = middle.iter().any(|l| {
        let s = l.trim_start();
        is_markdown_heading(s)
            || s.starts_with("- ")
            || s.starts_with("* ")
            || s.starts_with("+ ")
            || s.starts_with('>')
            || s.starts_with('|')
    });
    if !looks_like_markdown {
        return None;
    }
    Some(middle.join("\n"))
}

fn is_markdown_heading(line: &str) -> bool {
    if !line.starts_with('#') {
        return false;
    }
    let after_hashes = line.trim_start_matches('#');
    let hash_count = line.len() - after_hashes.len();
    (1..=6).contains(&hash_count) && after_hashes.starts_with(' ')
}

/// The LLM response sometimes wraps tables in a code block.
/// Remove the code block fences from around tables with regex so that the
/// table gets parsed as a table (both for HTML rendering and export
/// conversion).
pub fn unwrap_fenced_tables(text: &str) -> String {
    lazy_static! {
        // Regex to match beginning of code fence (with optional language) followed by table content
        // Looks for: ``` followed by optional language, newline, then lines starting with |
        static ref RE_CODE_FENCE_START_WITH_TABLE: Regex = Regex::new(r"```[^\n]*\n(\|[^\n]*\n)").unwrap();

        // Regex to match end of table (line ending with |) followed by closing code fence
        // Looks for: line ending with |, optional whitespace, newline, then closing ```
        static ref RE_TABLE_END_WITH_CODE_FENCE: Regex = Regex::new(r"(\|[^\n]*)\s*\n```").unwrap();
    }

    // Remove code block fences from around tables using two-step process
    // First, remove opening code fence before tables
    let processed_text = RE_CODE_FENCE_START_WITH_TABLE.replace_all(text, "$1").to_string();

    // Then, remove closing code fence after tables
    RE_TABLE_END_WITH_CODE_FENCE.replace_all(&processed_text, "$1").to_string()
}

pub fn markdown_to_html(markdown_text: &str) -> String {
    let processed_text = unwrap_fenced_tables(markdown_text);

    match to_html_with_options(processed_text.trim(), &Options::gfm()) {
        Ok(html) => {
            // LLM responses sometimes use code blocks for verses or quotes;
            // display them with sans-serif and wrap long lines so the
            // paragraph stays readable inside narrow UI panes.
            html.replace(
                "<pre>",
                "<pre style='font-family: sans-serif; white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word;'>",
            )
            .replace(
                "<code>",
                "<code style='font-family: sans-serif; white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word;'>",
            )
        },
        Err(_) => markdown_text.trim().to_string(), // Fallback to plain text on error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_prompt_code_block_any_language() {
        // Test the case you mentioned with 'md' language
        let text = "```md\nBetter than horses, thoroughbreds from Sindh,\nElephants and mighty giants—the self-restrained one is superior to these.\n```";
        let result = clean_prompt(text);

        // Should keep code block but remove language identifier
        assert!(result.contains("```"));
        assert!(!result.contains("md"));
        assert!(result.contains("Better than horses, thoroughbreds from Sindh"));
        assert!(result.contains("Elephants and mighty giants"));
    }

    #[test]
    fn test_clean_prompt_code_block_rust() {
        let text = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let result = clean_prompt(text);

        // Should keep code block but remove language identifier
        assert!(result.contains("```"));
        assert!(!result.contains("rust"));
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_clean_prompt_code_block_space_markdown() {
        // There may be space after the code block fences
        let text = "``` markdown\nHere, friend, while I was alone and secluded...\n```";
        let result = clean_prompt(text);

        // Should keep code block but remove language identifier
        assert!(result.contains("```"));
        assert!(!result.contains("markdown"));
        assert!(result.contains("Here, friend, while I was alone and secluded"));
    }

    #[test]
    fn test_clean_prompt_code_block_no_language() {
        let text = "```\nSome code without language\n```";
        let result = clean_prompt(text);

        // Should remain unchanged
        assert!(result.contains("```"));
        assert!(result.contains("Some code without language"));
    }

    #[test]
    fn test_clean_prompt_multiple_code_blocks() {
        let text = "Here is some text with ```rust\nfn main() {}\n``` and also ```python\nprint('hello')\n``` in it.";
        let result = clean_prompt(text);

        // Should remove language identifiers from both code blocks
        assert!(!result.contains("rust"));
        assert!(!result.contains("python"));
        // But keep the code blocks themselves
        assert!(result.contains("```\nfn main() {}\n```"));
        assert!(result.contains("```\nprint('hello')\n```"));
        assert!(result.contains("Here is some text"));
    }

    #[test]
    fn test_clean_prompt_mixed_code_blocks() {
        let text = r#"Text with ```rust
code1
code1
```

and also

```
code2
```

and

``` md
code3
code 3
```

```
code 4

code 4
``` blocks."#;
        let result = clean_prompt(text);

        println!("{}", &result);

        // Should remove language identifier from first block only
        assert!(!result.contains("rust"));
        assert!(result.contains("```\ncode1\ncode1\n```"));
        assert!(result.contains("and also"));
        // Second block should remain unchanged
        assert!(result.contains("```\ncode2\n```"));
        assert!(result.contains("```\ncode3\ncode 3\n```"));
        assert!(result.contains("```\ncode 4\n\ncode 4\n```"));
        assert!(result.contains("Text with"));
        // Should not be removed because not in a code block fence pair
        assert!(result.contains("blocks."));
    }

    #[test]
    fn test_clean_prompt_bold_stripping() {
        let text = "**This is bold text**";
        let result = clean_prompt(text);

        // Should strip wrapping bold syntax
        assert!(result.contains("This is bold text"));
        assert!(!result.contains("**"));
    }

    #[test]
    fn test_clean_prompt_italics_stripping() {
        let text = "*This is italic text*";
        let result = clean_prompt(text);

        // Should strip wrapping italics syntax
        assert!(result.contains("This is italic text"));
        assert!(!result.contains("*This is italic text*"));
    }

    #[test]
    fn test_clean_prompt_niggahita_consistency() {
        // Test that consistent_niggahita is applied
        let text = "saṃvaro";  // Using ṃ
        let result = clean_prompt(text);

        // Should convert ṃ to ṁ
        assert!(result.contains("saṁvaro"));
        assert!(!result.contains("saṃvaro"));
    }

    #[test]
    fn test_clean_prompt_strips_outer_fence_wrapping_markdown() {
        // LLMs sometimes wrap the whole reply in a fence even though the
        // content is markdown meant to render normally.
        let text = "```\n# Title\n\nSome **bold** text.\n- bullet\n```";
        let result = clean_prompt(text);

        assert!(result.contains("# Title"));
        assert!(result.contains("**bold**"));
        assert!(result.contains("- bullet"));
        assert!(!result.contains("```"));
    }

    #[test]
    fn test_clean_prompt_strips_outer_fence_with_language() {
        let text = "```markdown\n# Heading\n\nBody text.\n```";
        let result = clean_prompt(text);

        assert!(result.contains("# Heading"));
        assert!(result.contains("Body text."));
        assert!(!result.contains("```"));
    }

    #[test]
    fn test_clean_prompt_preserves_multiple_inner_fences() {
        // Outer fence should NOT be stripped when there's a nested fence —
        // the content is genuinely a code-surrounded example.
        let text = "Some intro.\n\n```rust\nfn main() {}\n```\n\nAnd more.";
        let result = clean_prompt(text);

        // The rust fence should still be present (with lang stripped).
        assert!(result.contains("```\nfn main() {}\n```"));
        assert!(result.contains("Some intro."));
        assert!(result.contains("And more."));
    }

    #[test]
    fn test_clean_prompt_no_cleaning_needed() {
        let text = "Just plain text here";
        let result = clean_prompt(text);

        // Should return the same text
        assert_eq!(result, "Just plain text here");
    }

    #[test]
    fn test_markdown_to_html_basic() {
        let markdown = "# Test Header\n\n**Bold text** and *italic text*.";
        let html = markdown_to_html(markdown);

        assert!(html.contains("<h1>Test Header</h1>"));
        assert!(html.contains("<strong>Bold text</strong>"));
        assert!(html.contains("<em>italic text</em>"));
    }

    #[test]
    fn test_markdown_to_html_fallback() {
        // Even with invalid markdown, it should return something (fallback behavior)
        let markdown = "Some *plain text_";
        let html = markdown_to_html(markdown);

        // Should contain the text (either as HTML or as fallback)
        assert!(html.contains("Some *plain text_"));
    }

    #[test]
    fn test_markdown_to_html_code_blocks_in_text() {
        let markdown = "The following relevant text:\n\n```\nLine one\nLine two```\n";
        let html = markdown_to_html(markdown);

        // Should convert to HTML code block
        assert!(html.contains("<pre") || html.contains("<code"));
    }

    #[test]
    fn test_markdown_to_html_table() {
        let markdown = "| Column 1 | Column 2 |\n|----------|----------|\n| Data 1   | Data 2   |";
        let html = markdown_to_html(markdown);

        // Should contain table elements (GFM tables)
        assert!(html.contains("<table>") || html.contains("Column 1"));
    }

    #[test]
    fn test_markdown_to_html_no_cleaning() {
        // Test that markdown_to_html no longer strips code blocks or formatting
        let markdown = "```markdown\n**Bold text**\n```";
        let html = markdown_to_html(markdown);

        // Should convert to HTML code block, not strip the wrapper
        assert!(html.contains("<pre") || html.contains("<code"));
        // The bold text should remain as literal text inside code block
        assert!(html.contains("**Bold text**"));
    }

    #[test]
    fn test_markdown_to_html_empty_input() {
        // Test empty input
        let html = markdown_to_html("");

        // Should handle empty input gracefully
        assert_eq!(html.trim(), "");
    }

    #[test]
    fn test_markdown_to_html_plain_text() {
        // Test plain text without any markdown
        let markdown = "Just plain text here";
        let html = markdown_to_html(markdown);

        // Should return the text, possibly wrapped in HTML
        assert!(html.contains("Just plain text here"));
    }

    #[test]
    fn test_markdown_to_html_table_in_code_block() {
        // Test the specific case from the user's example
        let markdown = r#"```
| **Pāli**      | **Grammar**               | **Meaning**                     | **Contextual Notes**                     |
|---------------|---------------------------|---------------------------------|------------------------------------------|
| Kāle          | Loc. sg. (m.)             | At the right time               | From *kāla* (time)                       |
| dadanti       | 3rd pl. pres. (*dā*)      | Give                            | Verb: to give                             |
| sappaññā      | Nom. pl. (adj.)           | Wise persons                    | *sapañña* (with wisdom)                  |
```

**Structure Notes**:
- **1st Line**: Subject (*sappaññā, vadaññū, vītamaccharā*) + Verb (*dadanti*) + Adverb (*Kāle*).
"#;
        let html = markdown_to_html(markdown);

        // Should contain table elements, not code block elements
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>"));
        assert!(html.contains("<td>"));
        assert!(html.contains("Pāli"));
        assert!(html.contains("Grammar"));
        // Should not contain code block elements for the table
        assert!(!html.contains("<pre"));
        assert!(!html.contains("<code"));
    }

    #[test]
    fn test_markdown_to_html_table_with_language_in_code_block() {
        // Test table wrapped in code block with language identifier
        let markdown = r#"```md
| Column 1 | Column 2 |
|----------|----------|
| Data 1   | Data 2   |
```"#;
        let html = markdown_to_html(markdown);

        // Should contain table elements
        assert!(html.contains("<table>"));
        assert!(html.contains("Column 1"));
        assert!(html.contains("Data 1"));
    }

    #[test]
    fn test_markdown_to_html_regular_code_block_preserved() {
        // Test that regular code blocks (not tables) are preserved
        let markdown = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let html = markdown_to_html(markdown);

        // Should contain code block elements
        assert!(html.contains("<pre") || html.contains("<code"));
        assert!(html.contains("fn main()"));
        // Should not be converted to table
        assert!(!html.contains("<table"));
    }
}
