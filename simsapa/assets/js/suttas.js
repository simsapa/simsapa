function toggle_variant (event) {
    el = event.target;
    el.parentNode.querySelectorAll(".variant").forEach((i) => {
        console.log(i);
        i.classList.toggle("hide");
    })
}

function toggle_comment (event) {
    el = event.target;
    el.parentNode.querySelectorAll(".comment").forEach((i) => {
        console.log(i);
        i.classList.toggle("hide");
    })
}

document.addEventListener("DOMContentLoaded", function(event) {
    document.querySelectorAll(".variant-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_variant);
    });
    document.querySelectorAll(".comment-wrap .mark").forEach((i) => {
        i.addEventListener("click", toggle_comment);
    });
});
