document.addEventListener('DOMContentLoaded', function() {
    links = document.getElementsByTagName('a');
    for (var i=0; i<links.length; i++) {
        links[i].onclick = function(e) {
            url = e.target.href;
            if (!url.startsWith('sutta:') && !url.startsWith('word:')) {
                return;
            }

            e.preventDefault();

            var params = {};

            if (url.startsWith('sutta:')) {
                s = url.replace('sutta:', '');
                params = {
                    action: 'show_sutta_by_uid',
                    arg: {'uid': s},
                };
            } else if (url.startsWith('word:')) {
                s = url.replace('word:', '');
                params = {
                    action: 'show_word_by_uid',
                    arg: {'uid': s},
                };
            }
            const options = {
                method: 'POST',
                headers: {
                  'Accept': 'application/json',
                  'Content-Type': 'application/json'
                },
                body: JSON.stringify(params),
            };
            fetch('@{api_url}/queues', options);
        }
    }
});
