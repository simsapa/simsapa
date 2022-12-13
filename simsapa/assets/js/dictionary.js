document.addEventListener("DOMContentLoaded", function(event) {
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
