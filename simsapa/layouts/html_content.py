from simsapa import SIMSAPA_PACKAGE_DIR, logger

def html_page(content: str, messages_url: str):
    try:
        with open(SIMSAPA_PACKAGE_DIR.joinpath('assets/css/main.css'), 'r') as f:
            css = f.read()
    except Exception as e:
        logger.error("Can't read main.css")
        css = ""

    js = """
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
            fetch('%s', options);
        }
    }
});
""" % (messages_url,)

    page_html = """
<!doctype html>
<html>
<head>
    <meta charset="utf-8">
    <style>%s</style>
    <script>%s</script>
</head>
<body>
%s
</body>
</html>
""" % (css, js, content)

    return page_html
