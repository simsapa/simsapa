from typing import Optional
from simsapa import PACKAGE_ASSETS_DIR, SUTTAS_CSS
from mako.template import Template

open_sutta_links_js_tmpl = Template(filename=str(PACKAGE_ASSETS_DIR.joinpath('templates/open_sutta_links.js')))
page_tmpl = Template(filename=str(PACKAGE_ASSETS_DIR.joinpath('templates/page.html')))

def html_page(content: str, api_url: Optional[str] = None, css_extra = None):
    css = SUTTAS_CSS
    if api_url is not None:
        css = css.replace("http://localhost:8000", api_url)

    # NOTE not using this atm
    # js = str(open_sutta_links_js_tmpl.render(api_url=api_url))

    if css_extra:
        css += css_extra

    html = str(page_tmpl.render(content=content,
                                css_head=css,
                                js_head='',
                                js_body='',
                                api_url=api_url))

    return html
