import * as h from "./helpers";

function copy_id(id_text: string, msg_div_id: string) {
    document.qt_channel.objects.helper.emit_copy_clipboard_text(id_text);
    h.show_transient_message("Copied: ID " + id_text, msg_div_id);
}

function copy_word(word_text: string, msg_div_id: string) {
    document.qt_channel.objects.helper.emit_copy_clipboard_text(word_text);
    h.show_transient_message("Copied: " + word_text, msg_div_id);
}

function copy_meaning(db_schema: string, db_table: string, db_uid: string, msg_div_id: string) {
    document.qt_channel.objects.helper.emit_copy_meaning(db_schema, db_table, db_uid);
    h.show_transient_message("Copied: meaning", msg_div_id);
}

function copy_gloss(db_schema: string, db_table: string, db_uid: string, gloss_keys: string, msg_div_id: string) {
    document.qt_channel.objects.helper.emit_copy_gloss(db_schema, db_table, db_uid, gloss_keys);
    h.show_transient_message("Copied: gloss line as table row", msg_div_id);
}

function copy_clipboard_text(text: string, msg_div_id: string) {
    document.qt_channel.objects.helper.emit_copy_clipboard_text(text);
    h.show_transient_message("Copied!", msg_div_id);
}

function copy_clipboard_html(html: string, msg_div_id: string) {
    document.qt_channel.objects.helper.emit_copy_clipboard_html(html);
    h.show_transient_message("Copied!", msg_div_id);
}

function load_more_results() {
    document.qt_channel.objects.helper.emit_load_more_results();
}

function add_hover_events(el: Element, channel: any) {
    let href = el.getAttribute('href');
    if (href !== null && href.startsWith('ssp://')) {
        el.addEventListener("mouseover", function(i_el) {
            var t = <Element>i_el.target;
            if (!t) {
                return;
            }
            var coords = t.getBoundingClientRect();

            var data = {
                href: href,
                x: coords.x + window.screenX,
                y: coords.y + window.screenY,
                width: coords.width,
                height: coords.height,
            };

            channel.objects.helper.link_mouseover(JSON.stringify(data));
        });

        el.addEventListener("mouseleave", function(i_el) {
            var t = <Element>i_el.target;
            if (!t) {
                return;
            }
            // var coords = t.getBoundingClientRect();
            channel.objects.helper.link_mouseleave(href);
        });
    }
}

export {
    copy_id,
    copy_word,
    copy_meaning,
    copy_gloss,
    copy_clipboard_text,
    copy_clipboard_html,
    load_more_results,
    add_hover_events,
}
