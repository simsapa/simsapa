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

function highlight_list (quotes) {
    let body = document.querySelector('body');
    let text = body.innerHTML;

    quotes.forEach((s) => {
        s = s.replaceAll('"', '["“”]');
        s = s.replaceAll("'", "['‘’]");
        let regex = new RegExp(s, 'gi');

        text = text.replace(regex, '<mark class="highlight">$&</mark>');
    })

    body.innerHTML = text;
}

function highlight_and_scroll_to (highlight_text) {
    let s = highlight_text;
    s = s.replaceAll('"', '["“”]');
    s = s.replaceAll("'", "['‘’]");
    const regex = new RegExp(s, 'gi');

    let body = document.querySelector('body');
    let text = body.innerHTML;

    const new_text = text.replace(regex, '<mark class="scrollto_highlight">$&</mark>');

    body.innerHTML = new_text;

    // only need the first result
    el = document.querySelector('mark.scrollto_highlight');
    el.scrollIntoView({behavior: "auto", block: "center", inline: "nearest"});
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
