function toggle_variant (event) {
    let el = event.target;
    el.parentNode.querySelectorAll(".variant").forEach((i) => {
        console.log(i);
        i.classList.toggle("hide");
    })
}

function toggle_comment (event) {
    let el = event.target;
    el.parentNode.querySelectorAll(".comment").forEach((i) => {
        console.log(i);
        i.classList.toggle("hide");
    })
}

function highlight_and_scroll_to (highligh_text) {
    const regex = new RegExp(highligh_text, 'gi');

    let body = document.querySelector('body');
    let text = body.innerHTML;

    text = text.replace(/(<span class="highlight" id="highlight_text">|<\/span>)/gim, '');

    const new_text = text.replace(regex, '<mark class="highlight" id="highlight_text">$&</mark>');

    body.innerHTML = new_text;

    el = document.getElementById('highlight_text');
    el.scrollIntoView();
}

document.addEventListener("DOMContentLoaded", function(event) {
    document.querySelectorAll(".variant-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_variant);
    });
    document.querySelectorAll(".comment-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_comment);
    });
});
