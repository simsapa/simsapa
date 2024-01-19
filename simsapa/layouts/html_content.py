from typing import Optional
from string import Template

from simsapa import PAGE_HTML, ICONS_HTML, SUTTAS_CSS, SUTTAS_JS

# open_sutta_links_js_tmpl: Optional[Template] = None
page_tmpl = Template(PAGE_HTML)

def html_page(content: str,
              api_url: Optional[str] = None,
              css_extra: Optional[str] = None,
              js_extra: Optional[str] = None):

    # global open_sutta_links_js_tmpl
    # if open_sutta_links_js_tmpl is None:
    #     b = pkgutil.get_data(__name__, str(PACKAGE_ASSETS_RSC_DIR.joinpath('templates/open_sutta_links.js')))
    #     if b is not None:
    #         open_sutta_links_js_tmpl = Template(b.decode("utf-8"))

    global page_tmpl

    css = SUTTAS_CSS
    if api_url is not None:
        css = css.replace("http://localhost:8000", api_url)

    if css_extra:
        css += "\n\n" + css_extra

    # NOTE not using this atm
    # js = str(open_sutta_links_js_tmpl.render(api_url=api_url))

    js = ""

    if js_extra:
        js += "\n\n" + js_extra

    if not js_extra or 'SHOW_BOOKMARKS' not in js_extra:
        js += " const SHOW_BOOKMARKS = false;"

    if not js_extra or 'SHOW_QUOTE' not in js_extra:
        js += " const SHOW_QUOTE = null;"

    # In suttas.js we expect SHOW_BOOKMARKS to be already set.
    js += SUTTAS_JS

    html = str(page_tmpl.substitute(content=content,
                                    css_head=css,
                                    js_head=js,
                                    js_body='',
                                    icons_html=ICONS_HTML,
                                    api_url=api_url))

    return html
