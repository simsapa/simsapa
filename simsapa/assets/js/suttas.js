function toggle_variant (event) {
    let el = event.target;
    el.parentNode.querySelectorAll(".variant").forEach((i) => {
        i.classList.toggle("hide");
    })
}

function toggle_comment (event) {
    let el = event.target;
    el.parentNode.querySelectorAll(".comment").forEach((i) => {
        i.classList.toggle("hide");
    })
}

function expand_quote_to_pattern (text) {
    s = text
    // Normalize quote marks to '
    s = s.replaceAll('"', "'");
    // Quote mark should match all types, and may not be present
    s = s.replaceAll("'", '[\'"“”‘’]*');
    // Normalize spaces
    s = s.replaceAll(/ +/g, " ")
    // Common spelling variations
    s = s.replaceAll(/[iī]/g, '[iī]')
    // Punctuation may not be present
    // Space may have punctuation in the text, but not in the link quote param
    // HTML tags, linebreaks, or quote marks may follow the word
    s = s.replace(/[ \.,;\?\!…—-]/g, '[ \n\'"“”‘’\.,;\?\!…—-]*(<[^>]+>[ \n]*)*');

    return s
}

function add_hover_events (el, channel) {
    let href = el.getAttribute('href');
    if (href !== null && href.startsWith('ssp://')) {
        el.addEventListener("mouseover", function(i_el) {
            coords = i_el.target.getBoundingClientRect();

            data = {
                href: href,
                x: coords.x + window.screenX,
                y: coords.y + window.screenY,
                width: coords.width,
                height: coords.height,
            };

            channel.objects.helper.link_mouseover(JSON.stringify(data));
        });

        el.addEventListener("mouseleave", function(i_el) {
            coords = i_el.target.getBoundingClientRect();
            channel.objects.helper.link_mouseleave(href);
        });
    }
}

function process_bookmarks_with_quote_only(bookmarks) {
    // NOTE: This method is adding bookmarks highlights is best to avoid.
    // It changes the DOM in ways that break applying ranges.
    let body = document.querySelector('body');
    let text = body.innerHTML;

    bookmarks.forEach((bm) => {
        let quote = expand_quote_to_pattern(bm.quote);
        let regex = new RegExp(s, 'gi');

        text = text.replace(regex, '<mark class="highlight">$&</mark>');
    });

    body.innerHTML = text;
}

function edit_comment (bookmark_schema_id) {
    if (document.qt_channel !== null) {
        document.qt_channel.objects.helper.send_bookmark_edit(bookmark_schema_id);
    }
}

function toggle_show_comment (bookmark_schema_id) {
    el = document.querySelector("div.bookmark-comment[data-bookmark-schema-id=" + bookmark_schema_id + "]")
    el.classList.toggle("hide");
}

function add_bookmark_comment (el) {
    let comment_text = el.getAttribute('data-comment-text');
    let comment_attr_json = el.getAttribute('data-comment-attr-json');

    let bookmark_schema_id = el.getAttribute('data-bookmark-schema-id');

    let wrap_span = document.createElement("span");
    wrap_span.setAttribute('style', 'position: relative;');

    let open_btn = document.createElement("div");
    open_btn.classList.add('open-button');
    open_btn.classList.add('btn');
    open_btn.setAttribute('style', 'position: absolute; top: -10px; right: 5px;');

    if (comment_text == null || comment_text == '') {
        icon = "icon-circle-solid";
    } else {
        icon = "icon-circle-dot-solid";
    }

    open_btn.innerHTML = '<a href="#"><svg class="icon icon-open"><use xlink:href="#' + icon + '"></use></svg></a>';

    open_btn.addEventListener("click", () => toggle_show_comment(bookmark_schema_id));

    wrap_span.appendChild(open_btn);

    let comment_div = document.createElement("div");
    comment_div.classList.add('bookmark-comment');
    comment_div.classList.add('hide');
    comment_div.setAttribute('style', 'position: absolute; bottom: 35px; left: 0px; width: 300px; height: 200px;');
    comment_div.setAttribute('data-bookmark-schema-id', bookmark_schema_id);

    let title_div = document.createElement("div");
    title_div.classList.add('bookmark-title');

    let edit_btn = document.createElement("div");
    edit_btn.classList.add('btn');
    edit_btn.classList.add('pull-left');
    edit_btn.innerHTML = '<a href="#"><svg class="icon icon-edit"><use xlink:href="#icon-square-pen"></use></svg></a>';
    title_div.appendChild(edit_btn);

    edit_btn.addEventListener("click", () => edit_comment(bookmark_schema_id));

    let close_btn = document.createElement("div");
    close_btn.classList.add('btn');
    close_btn.classList.add('pull-right');
    close_btn.innerHTML = '<a href="#"><svg class="icon icon-close"><use xlink:href="#icon-circle-xmark-solid"></use></svg></a>';
    title_div.appendChild(close_btn);

    close_btn.addEventListener("click", () => toggle_show_comment(bookmark_schema_id));

    let content_div = document.createElement("div");
    content_div.classList.add('bookmark-content');
    content_div.innerHTML = marked.parse(comment_text);

    let arr = content_div.querySelectorAll("a");
    arr.forEach(el => add_hover_events(el, document.qt_channel));

    comment_div.appendChild(title_div);
    comment_div.appendChild(content_div);
    wrap_span.appendChild(comment_div);

    el.appendChild(wrap_span);
}

function process_bookmarks_with_range(bookmarks) {
    bookmarks.forEach((bm) => {
        // Create the highlighter object here, so a custom ClassApplier can be
        // added for each item, to create the bookmark comment box.
        let highlighter = rangy.createHighlighter();

        highlighter.addClassApplier(rangy.createClassApplier("highlight", {
            ignoreWhiteSpace: true,
            elementTagName: "mark",
            elementAttributes: {
                'data-comment-text': bm.comment_text,
                'data-comment-attr-json': bm.comment_attr_json,
                'data-bookmark-schema-id': bm.bookmark_schema_id,
            },
            onElementCreate: add_bookmark_comment,
        }));

        rangy.deserializeSelection(bm.selection_range);
        highlighter.highlightSelection("highlight");
    });

    window.getSelection().removeAllRanges();
}

function get_bookmarks_with_range(sutta_uid) {
    params = {"sutta_uid": sutta_uid};
    fetch(API_URL + "/get_bookmarks_with_range_for_sutta", {
        method: "POST",
        headers: {"content-type": "application/json"},
        body: JSON.stringify(params),
    })
        .then(res => res.json())
        .then(data => process_bookmarks_with_range(data));
}

function get_bookmarks_with_quote_only(sutta_uid) {
    params = {"sutta_uid": sutta_uid};
    fetch(API_URL + "/get_bookmarks_with_quote_only_for_sutta", {
        method: "POST",
        headers: {"content-type": "application/json"},
        body: JSON.stringify(params),
    })
        .then(res => res.json())
        .then(data => process_bookmarks_with_quote_only(data));
}

function get_bookmarks_and_highlight() {
    if (SHOW_BOOKMARKS && SUTTA_UID) {
        get_bookmarks_with_range(SUTTA_UID);
        // NOTE: disable because the DOM change breaks applying ranges.
        // get_bookmarks_with_quote_only(SUTTA_UID);
    }
}

function highlight_and_scroll_to (highlight_text) {
    let s = expand_quote_to_pattern(highlight_text);
    const regex = new RegExp(s, 'gi');

    let main = document.querySelector('#ssp_main');
    let main_html = main.innerHTML;

    const m = main_html.match(regex);
    if (m == null) {
        return;
    }
    matched_html = m[0];

    const marked_html = '<mark class="scrollto-highlight">' + matched_html.replaceAll(/<[^>]+>/g, '</mark>$&<mark class="scrollto-highlight">') + '</mark>';

    const new_main_html = main_html.replace(matched_html, marked_html);

    // NOTE This loses eventlisteners. Add document.addEventListener() after this, or use onclick HTML attributes.
    main.innerHTML = new_main_html;

    // only need the first result
    el = document.querySelector('mark.scrollto-highlight');
    if (el !== null) {
        el.scrollIntoView({behavior: "auto", block: "center", inline: "nearest"});
    }
}

function get_selection_data () {
    // test for selection to exist
    let sel = document.getSelection();
    let sel_text = sel.toString();
    if (sel_text.length == 0) {
        return '';
    }

    let anchor_text = sel.anchorNode.textContent;

    let before = anchor_text.slice(0, sel.anchorOffset);

    let nth_in_anchor = before.split(sel_text).length;

    let sel_range = rangy.serializeSelection(null, true);

    let data = {
        sel_range: sel_range,
        sel_text: sel_text,
        anchor_text: anchor_text,
        nth_in_anchor: nth_in_anchor,
    };

    return JSON.stringify(data);
}

document.qt_channel = null;

document.addEventListener("DOMContentLoaded", function(event) {
    rangy.init();

    get_bookmarks_and_highlight();

    document.querySelectorAll(".variant-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_variant);
    });
    document.querySelectorAll(".comment-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_comment);
    });

    new QWebChannel(qt.webChannelTransport, function (channel) {
        document.qt_channel = channel;
        var res = document.querySelectorAll("a");
        var arr = [];

        res.forEach((el) => {
            var href = el.getAttribute('href');
            if (href !== null && href.startsWith('ssp://')) {
                arr.push(el);
            }
        });

        arr.forEach(el => add_hover_events(el, channel));
    });

});
