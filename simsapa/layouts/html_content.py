from typing import Optional
from simsapa import PACKAGE_ASSETS_DIR, logger
from mako.template import Template

open_sutta_links_js_tmpl = Template(filename=str(PACKAGE_ASSETS_DIR.joinpath('templates/open_sutta_links.js')))
page_tmpl = Template(filename=str(PACKAGE_ASSETS_DIR.joinpath('templates/page.html')))

def html_page(content: str, api_url: Optional[str] = None):
    try:
        with open(PACKAGE_ASSETS_DIR.joinpath('css/suttas.css'), 'r') as f:
            css = f.read()
            if api_url is not None:
                css = css.replace("http://localhost:8000", api_url)
    except Exception as e:
        logger.error(f"Can't read suttas.css: {e}")
        css = ""

    # NOTE not using this atm
    # js = str(open_sutta_links_js_tmpl.render(api_url=api_url))

    html = str(page_tmpl.render(content=content,
                                css_head=css,
                                js_head='',
                                js_body='',
                                api_url=api_url))

    return html
