use serde::Serialize;
use tinytemplate::TinyTemplate;

use crate::{get_app_globals, is_mobile};

static PAGE_HTML: &str = include_str!("../../assets/templates/page.html");
static FIND_HTML: &str = include_str!("../../assets/templates/find.html");
static READING_MODE_HTML: &str = include_str!("../../assets/templates/reading_mode.html");
pub static PREV_NEXT_CHAPTER_HTML: &str = include_str!("../../assets/templates/prev_next_chapter.html");
// Appended to the prev/next navigation on book chapter pages only — sutta
// pages share PREV_NEXT_CHAPTER_HTML but have no table of contents.
pub static TOC_BUTTON_HTML: &str = include_str!("../../assets/templates/toc_button.html");
static MENU_HTML: &str = include_str!("../../assets/templates/menu.html");
static CONFIRM_MODAL_HTML: &str = include_str!("../../assets/templates/confirm_modal.html");
static FOOTNOTE_MODAL_HTML: &str = include_str!("../../assets/templates/footnote_modal.html");
static INVALID_LINK_MODAL_HTML: &str = include_str!("../../assets/templates/invalid_link_modal.html");
static ICONS_HTML: &str = include_str!("../../assets/templates/icons.html");
static DISPLAY_SETTINGS_HTML: &str = include_str!("../../assets/templates/display_settings.html");
static COLUMN_BAR_HTML: &str = include_str!("../../assets/templates/column_bar.html");

static SUTTAS_CSS: &str = include_str!("../../assets/css/suttas.css");
static SUTTAS_JS: &str = include_str!("../../assets/js/suttas.js");

#[derive(Serialize)]
struct TmplContext {
    css_head: String,
    api_url: String,
    js_head: String,
    js_body: String,
    reading_mode_html: String,
    prev_next_chapter_html: String,
    find_html: String,
    menu_html: String,
    confirm_modal_html: String,
    footnote_modal_html: String,
    invalid_link_modal_html: String,
    icons_html: String,
    // Sutta-page-only chrome (cogwheel display-settings panel and bottom
    // column bar). Must default to empty: sutta_html_page() also renders
    // dictionary/DPPN/blank pages, which get no display chrome.
    display_settings_html: String,
    column_bar_html: String,
    content: String,
    body_class: String,
}

impl Default for TmplContext {
    fn default() -> Self {
        let g = get_app_globals();
        TmplContext {
            css_head: "".to_string(),
            api_url: g.api_url.clone(),
            js_head: "".to_string(),
            js_body: "".to_string(),
            reading_mode_html: READING_MODE_HTML.replace("{api_url}", &g.api_url).to_string(),
            prev_next_chapter_html: "".to_string(),  // Default to empty for suttas
            find_html: FIND_HTML.replace("{api_url}", &g.api_url).to_string(),
            menu_html: MENU_HTML.replace("{api_url}", &g.api_url).to_string(),
            confirm_modal_html: CONFIRM_MODAL_HTML.to_string(),
            footnote_modal_html: FOOTNOTE_MODAL_HTML.to_string(),
            invalid_link_modal_html: INVALID_LINK_MODAL_HTML.to_string(),
            icons_html: ICONS_HTML.to_string(),
            display_settings_html: "".to_string(),
            column_bar_html: "".to_string(),
            content: "".to_string(),
            body_class: "".to_string(),
        }
    }
}

pub fn sutta_html_page(content: &str,
                       api_url: Option<String>,
                       css_extra: Option<String>,
                       js_extra: Option<String>,
                       body_class: Option<String>) -> String {
    sutta_html_page_with_nav(content, api_url, css_extra, js_extra, body_class, None, false)
}

/// `sutta_display_chrome`: include the sutta-page-only display chrome (the
/// cogwheel display-settings panel and the bottom column bar). Only the sutta
/// render path passes true — dictionary, DPPN, book and blank pages stay
/// chrome-free.
pub fn sutta_html_page_with_nav(content: &str,
                                 api_url: Option<String>,
                                 css_extra: Option<String>,
                                 js_extra: Option<String>,
                                 body_class: Option<String>,
                                 prev_next_chapter_html: Option<String>,
                                 sutta_display_chrome: bool) -> String {

    let mut tt = TinyTemplate::new();
    tt.set_default_formatter(&tinytemplate::format_unescaped);
    tt.add_template("page_html", PAGE_HTML).expect("Template error in page.html!");

    let mut ctx = TmplContext::default();

    if let Some(s) = body_class {
        ctx.body_class = s.clone();
    }

    if let Some(nav_html) = prev_next_chapter_html {
        ctx.prev_next_chapter_html = nav_html;
    }

    let mut css = String::new();

    if let Some(s) = api_url {
        ctx.api_url = s.clone();
    }
    css.push_str(&SUTTAS_CSS.to_string().replace("http://localhost:8000", &ctx.api_url));

    if sutta_display_chrome {
        ctx.display_settings_html = DISPLAY_SETTINGS_HTML.replace("{api_url}", &ctx.api_url);
        ctx.column_bar_html = COLUMN_BAR_HTML.to_string();
    }

    if let Some(s) = css_extra {
        css.push_str("\n\n");
        css.push_str(&s);
    }

    let mut js = String::new();

    if let Some(js_extra) = &js_extra {
        if !js_extra.contains("SHOW_BOOKMARKS") {
            js.push_str(" const SHOW_BOOKMARKS = false;");
        }
    } else {
        js.push_str(" const SHOW_BOOKMARKS = false;");
    }

    if let Some(js_extra) = &js_extra {
        if !js_extra.contains("SHOW_QUOTE") {
            js.push_str(" const SHOW_QUOTE = null;");
        }
    } else {
        js.push_str(" const SHOW_QUOTE = null;");
    }

    js.push_str(&format!(" const IS_MOBILE = {};", is_mobile()));

    if let Some(js_extra) = &js_extra {
        js.push_str(js_extra);
    }

    // In suttas.js we expect SHOW_BOOKMARKS to be already set.
    js.push_str(SUTTAS_JS);

    ctx.css_head = css;
    ctx.js_head = js;
    ctx.content = String::from(content);

    tt.render("page_html", &ctx).unwrap_or_default()
}

static DICTIONARY_CSS: &str = include_str!("../../assets/css/dictionary.css");

/// Render a DPD bold-definition row as a complete HTML page.
///
/// Structure (per PRD §5.1):
///   header: `.bold-definition-header` — breadcrumb of nikāya/book/ref/title/subhead
///   body:   `.bold-definition-body`   — `.headword` span + commentary HTML (preserved)
///   footer: `.bold-definition-footer` — source file name
pub fn render_bold_definition(
    bd: &crate::db::dpd_models::BoldDefinition,
    window_id: &str,
    body_class: Option<String>,
) -> String {
    let header = format!(
        r#"<div class="bold-definition-header">{} › {} ({}) › {} › {}</div>"#,
        html_escape::encode_text(&bd.nikaya),
        html_escape::encode_text(&bd.book),
        html_escape::encode_text(&bd.ref_code),
        html_escape::encode_text(&bd.title),
        html_escape::encode_text(&bd.subhead),
    );
    // commentary is raw HTML from the DPD source — preserve it as-is.
    let body = format!(
        r#"<div class="bold-definition-body"><span class="headword">{}</span> {}</div>"#,
        html_escape::encode_text(&bd.bold),
        bd.commentary,
    );
    let footer = format!(
        r#"<div class="bold-definition-footer">{}</div>"#,
        html_escape::encode_text(&bd.file_name),
    );
    let content = format!("{}\n{}\n{}", header, body, footer);

    // Route through sutta_html_page so the page picks up suttas.js (double-click
    // / single-tap / long-press selection lookup hooks, IS_MOBILE, etc.) the
    // same way sutta and dictionary pages do. Dictionary CSS is layered on top
    // for `.bold-definition-*` and `.headword` styling.
    // page.html already declares API_URL from ctx.api_url; only inject WINDOW_ID
    // here so suttas.js (summary_selection, menu actions) can reach it.
    let js_extra = format!(
        " const WINDOW_ID = '{}'; window.WINDOW_ID = WINDOW_ID;",
        window_id,
    );

    sutta_html_page(
        &content,
        None,
        Some(DICTIONARY_CSS.to_string()),
        Some(js_extra),
        body_class,
    )
}

/// Render a DPPN dictionary entry as a complete HTML page.
///
/// The `definition_html` is already wrapped in `<div class="dppn">…</div>`
/// at bootstrap time (see `cli/src/bootstrap/dppn.rs::transform_dppn_definition_html`),
/// so this routes through `sutta_html_page` with `DICTIONARY_CSS` for the
/// `.dppn` styling, plus `WINDOW_ID` injection so the click handlers in
/// suttas.js can reach the owning window for the `ssp://dppn_lookup/` callback.
pub fn render_dppn_entry(
    word: &crate::db::dictionaries_models::DictWord,
    window_id: &str,
    body_class: Option<String>,
) -> String {
    let definition_html = word.definition_html.clone().unwrap_or_default();

    let js_extra = format!(
        " const WINDOW_ID = '{}'; window.WINDOW_ID = WINDOW_ID;",
        window_id,
    );

    sutta_html_page(
        &definition_html,
        None,
        Some(DICTIONARY_CSS.to_string()),
        Some(js_extra),
        body_class,
    )
}

pub fn blank_html_page(body_class: Option<String>) -> String {
    let mut tt = TinyTemplate::new();
    tt.set_default_formatter(&tinytemplate::format_unescaped);
    tt.add_template("page_html", PAGE_HTML).expect("Template error in page.html!");

    let mut ctx = TmplContext {
        reading_mode_html: "".to_string(),
        find_html: "".to_string(),
        menu_html: "".to_string(),
        confirm_modal_html: "".to_string(),
        footnote_modal_html: "".to_string(),
        icons_html: "".to_string(),
        body_class: body_class.unwrap_or_default(),
        ..Default::default()
    };

    let mut css = String::new();

    css.push_str(&SUTTAS_CSS.to_string().replace("http://localhost:8000", &ctx.api_url));

    ctx.css_head = css;

    tt.render("page_html", &ctx).unwrap_or_default()
}

/// Complete a user-imported dictionary entry into an HTML document with
/// `<html>`, `<head>…</head>` and `<body>` tags.
///
/// StarDict `h`-type entries are either full documents (e.g. whitney-gd) or bare
/// fragments (e.g. nyanatiloka-gd, peu-gd, reader.dict). The word renderer
/// injects the dictionary CSS/JS before `</head>` and the theme class and word
/// heading at `<html>` / `<body>`, so a fragment without those tags would render
/// unstyled and without the double-click lookup handlers. Tags that are already
/// present are kept as they are. An entry with no tags at all (e.g. peu-gd) is
/// plain text and is converted with `plain_text_to_html`.
pub fn ensure_html_document(html: &str) -> String {
    use regex::Regex;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref RE_DOCTYPE: Regex = Regex::new(r"(?i)^\s*<!DOCTYPE[^>]*>").unwrap();
        static ref RE_HTML_OPEN: Regex = Regex::new(r"(?i)<html(\s[^>]*)?>").unwrap();
        static ref RE_HTML_CLOSE: Regex = Regex::new(r"(?i)</html\s*>").unwrap();
        static ref RE_HEAD_OPEN: Regex = Regex::new(r"(?i)<head(\s[^>]*)?>").unwrap();
        static ref RE_HEAD_CLOSE: Regex = Regex::new(r"(?i)</head\s*>").unwrap();
        static ref RE_BODY_OPEN: Regex = Regex::new(r"(?i)<body(\s[^>]*)?>").unwrap();
        static ref RE_BODY_CLOSE: Regex = Regex::new(r"(?i)</body\s*>").unwrap();
        static ref RE_ANY_TAG: Regex = Regex::new(r"<[a-zA-Z!/][^>]*>").unwrap();
    }

    if !RE_ANY_TAG.is_match(html) {
        return format!("<!DOCTYPE html><html><head></head><body>{}</body></html>", plain_text_to_html(html));
    }

    let has_html = RE_HTML_OPEN.is_match(html);
    let has_head = RE_HEAD_CLOSE.is_match(html);
    let has_body = RE_BODY_OPEN.is_match(html);

    if has_html && has_head && has_body {
        return html.to_string();
    }

    if !has_html && !has_head && !has_body {
        return format!("<!DOCTYPE html><html><head></head><body>{}</body></html>", html);
    }

    // Partial document: split off the doctype and the outer <html> tags, then
    // rebuild the head and body around what remains.
    let doctype = RE_DOCTYPE.find(html).map(|m| m.as_str().trim()).unwrap_or("<!DOCTYPE html>");
    let mut inner = RE_DOCTYPE.replace(html, "").to_string();
    let html_open = match RE_HTML_OPEN.find(&inner) {
        Some(m) => m.as_str().to_string(),
        None => "<html>".to_string(),
    };
    inner = RE_HTML_OPEN.replace(&inner, "").to_string();
    inner = RE_HTML_CLOSE.replace_all(&inner, "").to_string();

    let (head, rest) = match (RE_HEAD_OPEN.find(&inner), RE_HEAD_CLOSE.find(&inner)) {
        (Some(open), Some(close)) if open.start() < close.start() => (
            inner[open.start()..close.end()].to_string(),
            format!("{}{}", &inner[..open.start()], &inner[close.end()..]),
        ),
        (None, Some(close)) => (
            format!("<head>{}</head>", &inner[..close.start()]),
            inner[close.end()..].to_string(),
        ),
        _ => ("<head></head>".to_string(), inner),
    };

    let body = if RE_BODY_OPEN.is_match(&rest) {
        if RE_BODY_CLOSE.is_match(&rest) {
            rest
        } else {
            format!("{}</body>", rest)
        }
    } else {
        format!("<body>{}</body>", rest)
    };

    format!("{}{}{}{}</html>", doctype, html_open, head, body)
}

/// Convert a plain-text dictionary entry to HTML: blocks separated by blank
/// lines become `<p>` paragraphs, and single line breaks within a block become
/// `<br>`. `<`, `>` and bare `&` are escaped, while HTML entities the text
/// already carries (peu-gd has `&quot;`) are kept.
pub fn plain_text_to_html(text: &str) -> String {
    use regex::{Captures, Regex};
    use lazy_static::lazy_static;

    lazy_static! {
        // A blank line may carry spaces or no-break spaces.
        static ref RE_BLANK_LINES: Regex = Regex::new(r"\n\s*\n").unwrap();
        static ref RE_AMPERSAND: Regex = Regex::new(r"&(#[0-9]+;|#[xX][0-9a-fA-F]+;|[a-zA-Z][a-zA-Z0-9]*;)?").unwrap();
    }

    let normalized = text.replace("\r\n", "\n");
    let escaped = RE_AMPERSAND.replace_all(&normalized, |caps: &Captures| {
        if caps.get(1).is_some() { caps[0].to_string() } else { "&amp;".to_string() }
    });
    let escaped = escaped.replace('<', "&lt;").replace('>', "&gt;");

    RE_BLANK_LINES
        .split(&escaped)
        .map(|block| block.trim())
        .filter(|block| !block.is_empty())
        .map(|block| {
            let lines: Vec<&str> = block.lines().map(|line| line.trim()).collect();
            format!("<p>{}</p>", lines.join("<br>"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ensure_html_document, plain_text_to_html};

    #[test]
    fn full_document_is_unchanged() {
        let doc = "<html><head><style>b{}</style></head><body><h2>√ac</h2></body></html>";
        assert_eq!(ensure_html_document(doc), doc);
    }

    #[test]
    fn fragment_is_wrapped() {
        let frag = "<p><b>Sostantivo</b></p><ol><li>x</li></ol>";
        assert_eq!(
            ensure_html_document(frag),
            format!("<!DOCTYPE html><html><head></head><body>{}</body></html>", frag),
        );
    }

    #[test]
    fn plain_text_is_wrapped_in_paragraphs() {
        let text = "(One) who is a senior (in years of monkhood).";
        assert_eq!(
            ensure_html_document(text),
            format!("<!DOCTYPE html><html><head></head><body><p>{}</p></body></html>", text),
        );
    }

    #[test]
    fn plain_text_blank_lines_split_paragraphs() {
        let text = "About race; 6 kinds of karma\n\n\nThe cause of race,\nThe six kinds of cause";
        assert_eq!(
            plain_text_to_html(text),
            "<p>About race; 6 kinds of karma</p><p>The cause of race,<br>The six kinds of cause</p>",
        );
    }

    #[test]
    fn plain_text_whitespace_only_lines_and_trailing_space() {
        let text = "The moment of praying.\u{a0}\n \u{a0}\nSecond.\n";
        assert_eq!(plain_text_to_html(text), "<p>The moment of praying.</p><p>Second.</p>");
    }

    #[test]
    fn plain_text_escapes_but_keeps_entities() {
        let text = "Arisen & Bright, a < b, the word &quot;elder&quot; &#257; &#x101;";
        assert_eq!(
            plain_text_to_html(text),
            "<p>Arisen &amp; Bright, a &lt; b, the word &quot;elder&quot; &#257; &#x101;</p>",
        );
    }

    #[test]
    fn body_without_head_gets_head() {
        let out = ensure_html_document("<body class=\"x\"><p>hi</p></body>");
        assert_eq!(out, "<!DOCTYPE html><html><head></head><body class=\"x\"><p>hi</p></body></html>");
    }

    #[test]
    fn head_without_body_gets_body() {
        let out = ensure_html_document("<HTML><HEAD><style>i{}</style></HEAD><p>hi</p></HTML>");
        assert_eq!(out, "<!DOCTYPE html><HTML><HEAD><style>i{}</style></HEAD><body><p>hi</p></body></html>");
    }

    #[test]
    fn style_before_fragment_without_head_open_tag() {
        let out = ensure_html_document("<style>i{}</style></head><p>hi</p>");
        assert_eq!(out, "<!DOCTYPE html><html><head><style>i{}</style></head><body><p>hi</p></body></html>");
    }
}
