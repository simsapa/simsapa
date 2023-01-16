#!/usr/bin/env python3

import os
import shutil
import sys
import glob
from pathlib import Path
from dotenv import load_dotenv
from aksharamukha import transliterate
from bs4 import BeautifulSoup
from lxml import etree

from simsapa import logger

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

SRC_XML_DIR = bootstrap_assets_dir.joinpath('cst4/extracted/Cst4/Xml/')

DEST_HTML_DIR = bootstrap_assets_dir.joinpath('cst4/roman')

if DEST_HTML_DIR.exists():
    shutil.rmtree(DEST_HTML_DIR)

DEST_HTML_DIR.mkdir()


def main():
    logger.info("Convert CST4 from Devanagari to Roman (ISO Pali)")

    xslt = etree.parse(SRC_XML_DIR.joinpath('tipitaka-deva.xsl'))
    transform = etree.XSLT(xslt)

    for p in glob.glob(f"{SRC_XML_DIR.joinpath('*.xml')}"):
        p = Path(p)

        # https://lxml.de/xpathxslt.html#xslt
        # https://stackoverflow.com/questions/16698935/how-to-transform-an-xml-file-using-xslt-in-python

        # The CST4 xml files are encoded as UTF-16LE (with BOM).
        # etree.parse() reads the file with correct encoding.

        old_xml = etree.parse(p)
        new_xml = transform(old_xml)

        d = etree.tostring(new_xml, encoding='utf-8')
        deva_html = d.decode('utf-8')

        # Add doctype.
        if '<!doctype' not in deva_html or '<!DOCTYPE' not in deva_html:
            deva_html = '<!DOCTYPE html>\n' + deva_html

        # Add charset.
        if 'charset="utf-8"' not in deva_html:
            deva_html = deva_html.replace('<head>', '<head>\n<meta charset="utf-8"/>')

        # Remove Javascript. Not needed, and also contains &lt; conversion error.
        soup = BeautifulSoup(str(deva_html), 'html.parser')
        h = soup.find_all(name = 'script')
        for i in h:
            i.decompose()

        # str(soup) also fixes closing tags:
        # <title/>
        # <a name="para8"/>

        deva_html = str(soup)

        # ISO Pali: cūḷakammavibhaṅgasuttaṁ
        roman_html = transliterate.process('Devanagari', 'ISOPali', deva_html)

        if roman_html.__class__ != str:
            logger.error(f"Can't convert: {p.name}")
            continue

        roman_html = str(roman_html)

        out_path = DEST_HTML_DIR.joinpath(p.name).with_suffix('.html')

        with open(out_path, 'w', encoding='utf-8') as f:
            f.write(roman_html)


if __name__ == "__main__":
    main()
