function fade_toggle(selector: string): void {
    const el = document.querySelector(selector);
    if (!el) {
        console.error("Element not found: " + selector);
        return;
    }

    el.classList.add('transition-visible');

    if (el.classList.contains('hidden')) {
        el.classList.add('transition-hidden');
        el.classList.remove('hidden');
        setTimeout(function() {
            el.classList.remove('transition-hidden');
        }, 2);

    } else {
        el.classList.add('transition-hidden');
        el.addEventListener('transitionend', function(_e: Event) {
            el.classList.add('hidden');
        }, {
            capture: false,
            once: true,
            passive: false,
        });
    }
}

function button_toggle_visible(button_el: Element, section_selector: string): void {
    const section_el = document.querySelector(section_selector);
    if (!section_el) {
        console.error("Element not found: " + section_selector);
        return;
    }

    button_el.classList.toggle('active');
    fade_toggle(section_selector);
}

function show_transient_message(text: string, msg_div_id: string): void {
    const div = document.createElement('div');
    div.className = 'message';

    const content = document.createElement('div');
    content.className = 'msg-content';
    content.textContent = text;
    div.appendChild(content);

    let el = document.getElementById(msg_div_id);
    if (!el) {
        console.error("Cannot find: transient-messages");
        return;
    }

    el.appendChild(div);

    div.style.transition = 'opacity 1.5s ease-in-out';
    div.style.opacity = '1';

    // After 3 seconds, start fading out
    setTimeout(() => {
        div.style.opacity = '0';
    }, 1000);

    // After the transition ends, remove the div from the DOM
    div.addEventListener('transitionend', () => {
        div.remove();
    });
}

function html_to_element(html: string): ChildNode {
    // The html argument should contain one root node (e.g. a <div>).
    let parser = new DOMParser();
    let doc = parser.parseFromString(html, "text/html");
    return doc.body.firstChild;
}

function add_bottom_message(msg: string): void {
    let page_bottom_el = document.getElementById('page_bottom');
    let html = `
<div style="margin-top: 2em;">
    <div class="message">
        <div class="msg-content">
            ${msg}
        </div>
    </div>
</div>`;
    let el = html_to_element(html);
    page_bottom_el.before(el);
}

function is_scrolled_into_view(el: Element): boolean {
    let el_rect = el.getBoundingClientRect();
    let is_visible = (el_rect.top >= 0) && (el_rect.bottom <= window.innerHeight);
    return is_visible;
}

export {
    fade_toggle,
    button_toggle_visible,
    show_transient_message,
    html_to_element,
    add_bottom_message,
    is_scrolled_into_view,
}

