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

function highlight_list (quotes) {
    let body = document.querySelector('body');
    let text = body.innerHTML;

    quotes.forEach((s) => {
        s = expand_quote_to_pattern(s)
        let regex = new RegExp(s, 'gi');

        text = text.replace(regex, '<mark class="highlight">$&</mark>');
    })

    body.innerHTML = text;
}

function highlight_and_scroll_to (highlight_text) {
    let s = expand_quote_to_pattern(highlight_text);
    const regex = new RegExp(s, 'gi');

    let body = document.querySelector('body');
    let body_html = body.innerHTML;

    const m = body_html.match(regex);
    if (m == null) {
        return;
    }
    matched_html = m[0];

    const marked_html = '<mark class="scrollto_highlight">' + matched_html.replaceAll(/<[^>]+>/g, '</mark>$&<mark class="scrollto_highlight">') + '</mark>';

    const new_body_html = body_html.replace(matched_html, marked_html);

    body.innerHTML = new_body_html;

    // only need the first result
    el = document.querySelector('mark.scrollto_highlight');
    if (el !== null) {
        el.scrollIntoView({behavior: "auto", block: "center", inline: "nearest"});
    }
}

document.addEventListener("DOMContentLoaded", function(event) {
    document.querySelectorAll(".variant-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_variant);
    });
    document.querySelectorAll(".comment-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_comment);
    });

    new QWebChannel(qt.webChannelTransport, function (channel) {
        let arr = document.querySelectorAll("a");
        arr.forEach((el) => {
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

                    channel.objects.link_hover_helper.link_mouseover(JSON.stringify(data));
                });

                el.addEventListener("mouseleave", function(i_el) {
                    coords = i_el.target.getBoundingClientRect();
                    channel.objects.link_hover_helper.link_mouseleave(href);
                });
            }
        });
    });
});
