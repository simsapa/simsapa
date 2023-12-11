import * as h from "./helpers";
import * as qch from "./qt_channel_helpers";

document.addEventListener("DOMContentLoaded", function(_event) {
    new QWebChannel(qt.webChannelTransport, function (channel) {
        document.qt_channel = channel;
        let arr = document.querySelectorAll("a");
        arr.forEach(el => qch.add_hover_events(el, channel));

        let body = document.querySelector("body");
        if (body) {
            body.addEventListener("dblclick", function (_body) {
                channel.objects.helper.page_dblclick();
            });
        }
    });
});

document.SSP = {
    fade_toggle: h.fade_toggle,
    button_toggle_visible: h.button_toggle_visible,
    show_transient_message: h.show_transient_message,
    copy_meaning: qch.copy_meaning,
    copy_gloss: qch.copy_gloss,
    copy_clipboard_text: qch.copy_clipboard_text,
    copy_clipboard_html: qch.copy_clipboard_html,
};
