import * as h from "./helpers";
import * as qch from "./qt_channel_helpers";

function handle_infinite_scroll(): void {
    const {
        scrollTop,
        scrollHeight,
        clientHeight
    } = document.documentElement;

    if (scrollTop + clientHeight >= scrollHeight - 5) {
        qch.load_more_results();
    }
};

function remove_infinite_scroll(): void {
    window.removeEventListener("scroll", handle_infinite_scroll);
}

function if_page_not_full_load_results(): void {
    let el = document.getElementById("page_bottom");
    if (!el) {
        return;
    }
    if (h.is_scrolled_into_view(el)) {
        qch.load_more_results();
    }
}

document.addEventListener("DOMContentLoaded", function(_event) {
    new QWebChannel(qt.webChannelTransport, function (channel) {
        document.qt_channel = channel;

        let res = document.querySelectorAll("a");
        let arr = [];

        res.forEach((el) => {
            var href = el.getAttribute('href');
            if (href !== null && href.startsWith('ssp://')) {
                arr.push(el);
            }
        });

        arr.forEach(el => qch.add_hover_events(el, channel));

        let body = document.querySelector("body");
        if (body) {
            body.addEventListener("dblclick", function (_body) {
                channel.objects.helper.page_dblclick();
            });
        }

        if (typeof SHOW_QUOTE !== "undefined" && SHOW_QUOTE !== null) {
            channel.objects.helper.emit_show_find_panel(SHOW_QUOTE);
        }

        if_page_not_full_load_results();
    });

    window.addEventListener(
        "scroll",
        handle_infinite_scroll,
        { passive: true },
    );
});

document.SSP = {
    fade_toggle: h.fade_toggle,
    button_toggle_visible: h.button_toggle_visible,
    show_transient_message: h.show_transient_message,
    html_to_element: h.html_to_element,
    copy_id: qch.copy_id,
    copy_word: qch.copy_word,
    copy_meaning: qch.copy_meaning,
    copy_gloss: qch.copy_gloss,
    copy_clipboard_text: qch.copy_clipboard_text,
    copy_clipboard_html: qch.copy_clipboard_html,
    load_more_results: qch.load_more_results,
    handle_infinite_scroll: handle_infinite_scroll,
    remove_infinite_scroll: remove_infinite_scroll,
    add_bottom_message: h.add_bottom_message,
    if_page_not_full_load_results: if_page_not_full_load_results,
};
