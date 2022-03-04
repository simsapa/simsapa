#!/usr/bin/env python3

import os
import shutil
import sys
import re
import glob
from pathlib import Path
from typing import Any, List, Optional, Pattern, Tuple, TypedDict
from bs4 import BeautifulSoup
from dotenv import load_dotenv

from sqlalchemy.sql import func
from sqlalchemy.orm.session import Session

from simsapa.app.db import appdata_models as Am
from simsapa import logger
import helpers

load_dotenv()

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)

CST4_DIR = bootstrap_assets_dir.joinpath('cst4')
HTML_DIR = CST4_DIR.joinpath('roman')

TREES_DIR = CST4_DIR.joinpath('sutta-trees')

if TREES_DIR.exists():
    shutil.rmtree(TREES_DIR)

TREES_DIR.mkdir()

"""
ls -1 ../bootstrap-assets-resources/cst4/roman/

# --- Abhidhamma Pitaka ---
abh01m.mul.html
abh02m.mul.html
abh03m10.mul.html
abh03m11.mul.html
abh03m1.mul.html
abh03m2.mul.html
abh03m3.mul.html
abh03m4.mul.html
abh03m5.mul.html
abh03m6.mul.html
abh03m7.mul.html
abh03m8.mul.html
abh03m9.mul.html

abh01a.att.html
abh02a.att.html
abh03a.att.html

abh01t.tik.html
abh02t.tik.html
abh03t.tik.html

abh04t.nrf.html
abh05t.nrf.html
abh06t.nrf.html
abh07t.nrf.html
abh08t.nrf.html
abh09t.nrf.html

e0101n.mul.html
e0102n.mul.html

e0103n.att.html
e0104n.att.html

e0105n.nrf.html
e0201n.nrf.html
e0301n.nrf.html
e0401n.nrf.html
e0501n.nrf.html
e0601n.nrf.html
e0602n.nrf.html
e0603n.nrf.html
e0604n.nrf.html
e0605n.nrf.html
e0606n.nrf.html
e0607n.nrf.html
e0608n.nrf.html
e0701n.nrf.html
e0702n.nrf.html
e0703n.nrf.html
e0801n.nrf.html
e0802n.nrf.html
e0803n.nrf.html
e0804n.nrf.html
e0805n.nrf.html
e0806n.nrf.html
e0807n.nrf.html
e0808n.nrf.html
e0809n.nrf.html
e0810n.nrf.html
e0811n.nrf.html
e0812n.nrf.html
e0813n.nrf.html
e0901n.nrf.html
e0902n.nrf.html
e0903n.nrf.html
e0904n.nrf.html
e0905n.nrf.html
e0906n.nrf.html
e0907n.nrf.html
e1001n.nrf.html
e1002n.nrf.html
e1003n.nrf.html
e1004n.nrf.html
e1005n.nrf.html
e1006n.nrf.html
e1007n.nrf.html
e1008n.nrf.html
e1009n.nrf.html
e1010n.nrf.html
e1101n.nrf.html
e1102n.nrf.html
e1103n.nrf.html
e1201n.nrf.html
e1202n.nrf.html
e1203n.nrf.html
e1204n.nrf.html
e1205n.nrf.html
e1206n.nrf.html
e1207n.nrf.html
e1208n.nrf.html
e1209n.nrf.html
e1210n.nrf.html
e1211n.nrf.html
e1212n.nrf.html
e1213n.nrf.html
e1214n.nrf.html
e1215n.nrf.html

# --- Digha Nikaya: Mula ---
- [x] s0101m.mul.html
- [x] s0102m.mul.html
- [x] s0103m.mul.html

s0101t.tik.html
s0102t.tik.html
s0103t.tik.html

s0101a.att.html
s0102a.att.html
s0103a.att.html

s0104t.nrf.html
s0105t.nrf.html

# -- Majjhima Nikaya --
- [x] s0201m.mul.html
- [x] s0202m.mul.html
- [x] s0203m.mul.html

s0201t.tik.html
s0202t.tik.html
s0203t.tik.html

s0201a.att.html
s0202a.att.html
s0203a.att.html

# --- Samyutta Nikaya ---
- [x] s0301m.mul.html
- [x] s0302m.mul.html
- [x] s0303m.mul.html
- [x] s0304m.mul.html
- [x] s0305m.mul.html

s0301a.att.html
s0302a.att.html
s0303a.att.html
s0304a.att.html
s0305a.att.html

s0301t.tik.html
s0302t.tik.html
s0303t.tik.html
s0304t.tik.html
s0305t.tik.html

# --- Anguttara Nikaya: Mula ---
- [x] s0401m.mul.html
- [x] s0402m1.mul.html
- [x] s0402m2.mul.html
- [x] s0402m3.mul.html
- [x] s0403m1.mul.html
- [x] s0403m2.mul.html
- [x] s0403m3.mul.html
- [x] s0404m1.mul.html
- [x] s0404m2.mul.html
- [x] s0404m3.mul.html
- [x] s0404m4.mul.html

s0401a.att.html
s0402a.att.html
s0403a.att.html
s0404a.att.html

s0401t.tik.html
s0402t.tik.html
s0403t.tik.html
s0404t.tik.html

# --- Khuddaka Nikaya: Mula ---
s0501m.mul.html
s0502m.mul.html
s0503m.mul.html
s0504m.mul.html
s0505m.mul.html
s0506m.mul.html
s0507m.mul.html
s0508m.mul.html
s0509m.mul.html
s0510m1.mul.html
s0510m2.mul.html
s0511m.mul.html
s0512m.mul.html
s0513m.mul.html
s0514m.mul.html
s0515m.mul.html
s0516m.mul.html
s0517m.mul.html
s0519m.mul.html

s0501a.att.html
s0502a.att.html
s0503a.att.html
s0504a.att.html
s0505a.att.html
s0506a.att.html
s0507a.att.html
s0508a1.att.html
s0508a2.att.html
s0509a.att.html
s0510a.att.html
s0511a.att.html
s0512a.att.html
s0513a1.att.html
s0513a2.att.html
s0513a3.att.html
s0513a4.att.html
s0514a1.att.html
s0514a2.att.html
s0514a3.att.html
s0515a.att.html
s0516a.att.html
s0517a.att.html
s0519a.att.html

s0519t.tik.html

s0501t.nrf.html
s0518m.nrf.html
s0520m.nrf.html

# --- Vinaya Pitaka ---

vin01m.mul.html
vin02m1.mul.html
vin02m2.mul.html
vin02m3.mul.html
vin02m4.mul.html

vin01a.att.html
vin02a1.att.html
vin02a2.att.html
vin02a3.att.html
vin02a4.att.html

vin01t1.tik.html
vin01t2.tik.html
vin02t.tik.html

vin04t.nrf.html
vin05t.nrf.html
vin06t.nrf.html
vin07t.nrf.html
vin08t.nrf.html
vin09t.nrf.html
vin10t.nrf.html
vin11t.nrf.html
vin12t.nrf.html
vin13t.nrf.html
"""

# Pitaka > Nikaya > Book > [Part] > [Division] > Sutta

# The division after Nikaya is always Book (even when it's called ...vagga).
# The Part and Section levels are not used in every Nikaya.
# Sections are used for chunking, e.g. groups of tens of suttas.

# DIVISION_SEP = {
#     #      Book,    [Part],     [Division], Sutta
#     'dn': [RE_BOOK, None,       None,       RE_CHAPTER],
#     'mn': [RE_BOOK, RE_CHAPTER, None,       RE_SUBHEAD],
#     'sn': [RE_BOOK, RE_CHAPTER, RE_TITLE,   RE_SUBHEAD],
#     'an': [RE_BOOK, RE_TITLE,   RE_CHAPTER, RE_SUBHEAD],
# }

# Samyutta divisions:
"""
<p class="chapter">1. devatāsaṁyuttaṁ</p>
<p class="title">1. nal̤avaggo</p>
<p class="subhead">1. oghataraṇasuttaṁ</p>
"""

# Anguttara divisions:
"""
<p class="nikaya">aṅguttaranikāyo</p>
<a name="an7"></a>
<p class="book">sattakanipātapāl̤i</p>
<a name="an7_1"></a>
<p class="title">paṭhamapaṇṇāsakaṁ</p>
<a name="an7_1_1"></a>
<p class="chapter">1. dhanavaggo</p>
<p class="subhead">1. paṭhamapiyasuttaṁ</p>
"""


RE_BOOK = re.compile(r'<p class="book">([^<]+)</p>')
RE_BOOK_WITH_REF = re.compile(r'<a name="([^"]+)"></a>[\n ]*<p class="book">([^<]+)</p>')

RE_TITLE = re.compile(r'<p class="title">([^<]+)</p>')
RE_TITLE_NUM = re.compile(r'<p class="title">(\d+)\.[^<]+</p>')
RE_TITLE_WITH_REF = re.compile(r'<a name="([^"]+)"></a>[\n ]*<p class="title">([^<]+)</p>')

RE_CHAPTER = re.compile(r'<p class="chapter">([^<]+)</p>')
RE_CHAPTER_NUM = re.compile(r'<p class="chapter">(\d+)\.[^<]+</p>')
RE_CHAPTER_WITH_REF = re.compile(r'<a name="([^"]+)"></a>[\n ]*<p class="chapter">([^<]+)</p>')

RE_CHAPTER_PARENS_AND_NUM = re.compile(r'<p class="chapter">\(\d+\) *(\d+)\.[^<]+</p>')

# <p class="subhead">5. cūḷakammavibhaṅgasuttaṁ <span class="note">[subhasuttantipi vuccati]</span></p>
# <p class="subhead">2. pañcattayasuttaṁ <span class="note">[pañcāyatanasutta (ka॰)]</span></p>

#RE_SUBHEAD = re.compile(r'<p class="subhead">([^<]+)</p>')
RE_SUBHEAD = re.compile(r'<p class="subhead">(.+?)</p>')

# <p class="subhead">4. tulākūṭasuttaṁ</p>
# <p class="subhead">7-9. devacutinirayādisuttaṁ</p>
RE_SUBHEAD_NUM = re.compile(r'<p class="subhead">(\d+).+</p>')
RE_SUBHEAD_RANGE = re.compile(r'<p class="subhead">([\d\.-]+).+</p>')


def get_pitaka(p: Path) -> str:
    # Determine Pitaka:
    # - Sutta
    # - Vinaya
    # - Abhidhamma
    # - Misc

    if p.name.startswith('s'):
        r = 'sutta'
    elif p.name.startswith('vin'):
        r = 'vinaya'
    elif p.name.startswith('a'):
        r = 'abhidhamma'
    elif p.name.startswith('e'):
        r = 'misc'
    else:
        logger.error(f"Can't determine Pitaka for: {p.name}")
        sys.exit(1)

    return r


def get_collection(p: Path) -> str:
    # Determine collection:
    # - mul: mula
    # - tik: tika
    # - att: atthakatha
    # - nrf: misc (vism, etc)

    if p.stem.endswith('mul'):
        r = 'mul'
    elif p.stem.endswith('tik'):
        r = 'tik'
    elif p.stem.endswith('att'):
        r = 'att'
    elif p.stem.endswith('nrf'):
        r = 'nrf'
    else:
        logger.error(f"Can't determine Collection for: {p.name}")
        sys.exit(1)

    return r


def get_nikaya(p: Path) -> str:
    # Determine nikaya
    # rg --no-filename -e 'class="nikaya"' | sort | uniq
    # <p class="nikaya">dīghanikāyo</p>
    # <p class="nikaya">dīghanikāye</p>

    html_text = open(p, 'r', encoding='utf-8').read()
    soup = BeautifulSoup(html_text, 'html.parser')

    h = soup.find_all(class_='nikaya')
    a = list(map(lambda x: x.decode_contents(), h))

    if len(a) > 1:
        logger.error(f"Multiple nikayas in: {p.name}")
        sys.exit(1)

    s = str(a[0])

    if s.startswith('dīghanik'):
        r = 'dn'
    elif s.startswith('majjhimanik'):
        r = 'mn'
    elif s.startswith('saṁyuttanik'):
        r = 'sn'
    elif s.startswith('aṅguttaranik'):
        r = 'an'
    elif s.startswith('khuddakanik'):
        r = 'kn'
    else:
        logger.error(f"Can't determine nikaya in: {p.name}")
        sys.exit(1)

    return r


def is_sutta_title(title: str) -> bool:
    return (re.search(r'suttaṁ*$', title) is not None \
        or re.search(r'sutt.*kaṁ$', title) is not None)

# Sutta Pitaka references
class SuttaRef(TypedDict):
    ref: str # dn_1_5, an_4_5_1
    nikaya: str # dn, mn, sn, an, ...
    collection: str # mul, tik, ...
    book_num: Optional[int] # 4
    part_num: Optional[int] # 5
    section_num: Optional[int] # n/a
    sutta_num: Optional[int] # 1


def find_next_sub_group_sep(html_text: str) -> Optional[Pattern]:
    min_pos = len(html_text)
    next_re: Optional[Pattern] = None

    div_seps = [ RE_BOOK, RE_TITLE, RE_CHAPTER, RE_SUBHEAD ]

    for sep_re in div_seps:
        matches = re.finditer(sep_re, html_text)

        m = next(matches, None)
        if m is None:
            continue

        pos = m.start()
        if pos < min_pos:
            min_pos = pos
            next_re = sep_re

    return next_re


class GroupInterface:
    title: str
    ref: Optional[SuttaRef]
    wisdom_pubs_ref: Optional[str]
    text: Optional[str]
    parent_group: Optional[Any] # Optional[GroupInterface]
    sub_groups: List[Any] # List[GroupInterface]

    def add_sub_groups(self, group_num: Optional[int]):
        pass


class Group(GroupInterface):

    def __init__(self,
                 title: str,
                 group_text: str,
                 group_num: Optional[int] = None,
                 group_sep_text: Optional[str] = None,
                 ref: Optional[SuttaRef] = None,
                 group_re_sep: Optional[Pattern] = None,
                 parent_group: Optional[GroupInterface] = None):

        self.title = title
        self.group_sep_text = group_sep_text
        self.group_num = group_num
        self.group_text = group_text
        self.ref = ref
        self.wisdom_pubs_ref = None
        self.group_re_sep = group_re_sep
        self.parent_group = parent_group
        self.sub_re_sep: Optional[Pattern] = None
        self.sub_groups = []

    def as_string_node(self) -> str:
        if self.ref is None:
            lvl = 0
            label = 'x'
        elif self.ref['sutta_num'] is not None:
            lvl = 3
            label = 'S'
        elif self.ref['section_num'] is not None:
            lvl = 2
            label = 'D'
        elif self.ref['part_num'] is not None:
            lvl = 1
            label = 'P'
        elif self.ref['book_num'] is not None:
            lvl = 0
            label = 'B'
        else:
            lvl = 0
            label = 'ERROR'

        if self.ref is None:
            ref = 'x'
        else:
            ref = self.ref['ref']

        if self.wisdom_pubs_ref is None:
            wis_ref = 'x'
        else:
            wis_ref = self.wisdom_pubs_ref

        lvl_sep = lvl*"  "
        # S an_4_3_11  | ...
        s = f"{label} {ref:13} | {wis_ref:10} | {lvl_sep}{self.title}"

        return s

    def set_ref(self,
                book: Optional[int] = None,
                part: Optional[int] = None,
                section: Optional[int] = None,
                sutta: Optional[int] = None):

        num = book or part or section or sutta

        if self.parent_group is not None \
           and self.parent_group.ref is not None:
            self.ref = SuttaRef(
                ref = f"{self.parent_group.ref['ref']}_{num}",
                nikaya = self.parent_group.ref['nikaya'],
                collection = self.parent_group.ref['collection'],
                book_num = book,
                part_num = part,
                section_num = section,
                sutta_num = sutta,
            )
        else:
            logger.error("parent_group.ref is None")
            sys.exit(1)

    def add_sub_groups(self, group_num: Optional[int]):
        self.group_num = group_num
        self.determine_sutta_ref()

        if self.group_text is None:
            return

        self.sub_re_sep = find_next_sub_group_sep(self.group_text)

        if self.sub_re_sep is None or is_sutta_title(self.title):
            # If no more sub-groups (child items),
            # or title ends with sutta(ṁ),
            # then self.text is a sutta.
            self.sub_groups = []
            return

        sub_texts = []

        limits = list(map(lambda x: x.start(), re.finditer(self.sub_re_sep, self.group_text)))

        total = len(limits)
        for idx, pos in enumerate(limits):
            start_pos = pos

            if idx+1 < total:
                end_pos = limits[idx+1]
                text = self.group_text[start_pos:end_pos]
            else:
                end_pos = self.group_text.find('</body>')
                if end_pos == -1:
                    end_pos = len(self.group_text)
                text = self.group_text[start_pos:end_pos]

            sub_texts.append(text)

        if len(sub_texts) == 0:
            # There should have been sub-texts if re_sep was found.
            logger.error("No sub-texts")
            sys.exit(1)

        self.sub_groups = list(map(self._sub_text_to_group, sub_texts))

        for idx, grp in enumerate(self.sub_groups):
            grp.add_sub_groups(idx+1)


    def _sub_text_to_group(self, text: str) -> GroupInterface:
        if self.sub_re_sep is None:
            logger.error("sub_re_sep is None")
            sys.exit(1)

        matches = re.finditer(self.sub_re_sep, text)
        m = next(matches, None)
        if m is None:
            logger.error("No matches")
            sys.exit(1)

        group_sep_text = m.group(0)

        # Remove prefix numbers from title text
        # 5.
        # 12-13.
        title = re.sub(r'^[0-9\. –-]+', '', m.group(1))

        # <p class="subhead">5. cūḷakammavibhaṅgasuttaṁ <span class="note">[subhasuttantipi vuccati]</span></p>
        # <p class="subhead">2. pañcattayasuttaṁ <span class="note">[pañcāyatanasutta (ka॰)]</span></p>

        title = re.sub(r'<span.*</span>', '', title).strip()

        text_after_sep = text[m.end(0):]

        return Group(
            title = title,
            group_sep_text = group_sep_text,
            group_text = text_after_sep,
            group_re_sep = self.sub_re_sep,
            parent_group = self,
        )


    def determine_sutta_ref(self):
        if self.parent_group is None:
            # No parent, this is the tree top.
            return

        if self.parent_group.ref is None:
            return

        nikaya = self.parent_group.ref['nikaya']

        # === Nikaya > Book ===

        if self.parent_group.parent_group is None:

            nikaya = self.parent_group.ref['nikaya']
            self.set_ref(book=self.group_num)
            return

        if nikaya == 'dn':

            # === Digha ===

            if self.parent_group.parent_group.parent_group is None:

                # === Nikaya > Book > Sutta ===

                nikaya = self.parent_group.ref['nikaya']

                if self.group_sep_text is None:
                    logger.error("group_sep_text is None")
                    sys.exit(1)

                m = re.findall(RE_CHAPTER_NUM, self.group_sep_text)
                num = int(m[0])
                self.set_ref(sutta=num)
                return

        elif nikaya == 'mn':

            # === Majjhima ===

            if self.parent_group is not None:

                # === Nikaya > Book > [Part or Sutta] ===

                # Sometime this is a sutta, sometimes a group.

                if self.group_sep_text is None:
                    logger.error("group_sep_text is None")
                    sys.exit(1)

                # Either a class="chapter", or a class="subhead"

                m = re.findall(RE_CHAPTER_NUM, self.group_sep_text)
                if len(m) == 0:
                    m = re.findall(RE_SUBHEAD_NUM, self.group_sep_text)

                if len(m) == 0:
                    # There is no number, this is a section header
                    # <p class="subhead">dassanā pahātabbāsavā</p>
                    self.ref = None
                    return

                num = int(m[0])

                if is_sutta_title(self.title):
                    # This is a sutta
                    # <p class="chapter">1. brahmajālasuttaṁ</p>

                    self.set_ref(sutta=num)
                    return

                else:
                    # This is a Part
                    # <p class="chapter">1. mūlapariyāyavaggo</p>

                    self.set_ref(part=num)
                    return

            else:
                logger.error(f"Didn't parse sutta ref: {self.title}")
                return

        elif nikaya == 'sn':

            # === Samyutta ===

            if self.parent_group is not None:

                if self.group_sep_text is None:
                    logger.error("group_sep_text is None")
                    sys.exit(1)

                if re.search(r'saṁyuttaṁ*$', self.title):
                    # <p class="chapter">1. devatāsaṁyuttaṁ</p>
                    # This is a Part

                    m = re.findall(RE_CHAPTER_NUM, self.group_sep_text)
                    num = int(m[0])

                    self.set_ref(part=num)
                    return

                elif re.search(RE_TITLE, self.group_sep_text):
                    # <p class="title">1. nal̤avaggo</p>
                    # <p class="title">9. antarapeyyālaṁ</p>
                    # This is a Section

                    m = re.findall(RE_TITLE_NUM, self.group_sep_text)
                    num = int(m[0])

                    self.set_ref(section=num)
                    return

                elif is_sutta_title(self.title):
                    # Title can include the range in pali:
                    # <p class="subhead">2-10. rūpādisuttanavakaṁ</p>
                    #
                    # <p class="subhead">4. tulākūṭasuttaṁ</p>
                    # This is a Sutta
                    # Number can be a range, take num as the first number:
                    # <p class="subhead">7-9. devacutinirayādisuttaṁ</p>

                    m = re.findall(RE_SUBHEAD_NUM, self.group_sep_text)

                    if len(m) == 1:
                        num = int(m[0])
                    else:
                        # <p class="subhead">aññataraphalasuttaṁ</p>
                        # Some suttas don't have a number, increment the number of the previous one

                        if self.group_num is None:
                            logger.error("group_num is None")
                            sys.exit(1)

                        if self.group_num == 1:
                            num = 1
                        else:
                            # This item's array index: self.group_num - 1
                            # Previous index: self.group_num - 2
                            n = self.group_num - 2
                            num = self.parent_group.sub_groups[n].ref['sutta_num'] + 1

                    self.set_ref(sutta=num)
                    return

                elif re.search(RE_SUBHEAD, self.group_sep_text):
                    # <p class="subhead">sagāthāvaggo paṭhamo.</p>

                    m = re.findall(RE_SUBHEAD_NUM, self.group_sep_text)
                    if len(m) == 0:
                        # If there is no number, it is a section heading.
                        # <p class="subhead">sagāthāvaggo paṭhamo.</p>
                        self.ref = None
                        return
                    else:
                        logger.error(f"Didn't parse sutta ref: {self.group_sep_text}")

                else:
                    logger.error(f"Didn't parse sutta ref: {self.group_sep_text}")

            else:
                # No parent, this must be the root node.
                pass

        elif nikaya == 'an':

            # === Anguttara ===

            if self.parent_group is not None:

                if self.group_sep_text is None:
                    logger.error("group_sep_text is None")
                    sys.exit(1)

                # <a name="an1"></a>
                # <p class="book">ekakanipātapāl̤i</p>
                # <a name="an1_1"></a>
                # <p class="chapter">1. rūpādivaggo</p>
                # ...
                # <p class="chapter">2. nīvaraṇappahānavaggo</p>
                # ...
                # <p class="chapter">20. amatavaggo</p>

                if self.parent_group.ref['ref'] == 'an1':

                    # In AN 1, *vaggo is a sutta.
                    if re.search(r'vaggo*', self.title):

                        m = re.findall(RE_CHAPTER_NUM, self.group_sep_text)
                        if len(m) == 0:
                            logger.error(f"No chapter number: {self.group_sep_text}")
                            return
                        else:

                            num = int(m[0])
                            self.set_ref(sutta=num)
                            return

                # <p class="nikaya">aṅguttaranikāyo</p>
                # <p class="book">dukanipātapāl̤i</p>
                # <a name="an2_1"></a>
                # <p class="title">1. paṭhamapaṇṇāsakaṁ</p>
                # <a name="an2_1_1"></a>
                # <p class="chapter">1. kammakaraṇavaggo</p>
                # <p class="subhead">1. vajjasuttaṁ</p>
                # ...
                # <a name="an2_6"></a>
                # <p class="chapter">3. vinayapeyyālaṁ</p>
                # ...
                # <a name="an2_7"></a>
                # <p class="chapter">4. rāgapeyyālaṁ</p>

                if is_sutta_title(self.title) \
                   or re.search(RE_SUBHEAD_NUM, self.group_sep_text) \
                   or re.search(RE_SUBHEAD_RANGE, self.group_sep_text):
                    # Sutta title might not include 'sutta' in the case of ranges
                    # <p class="subhead">4-30. pariññādisuttāni</p>

                    m = re.findall(RE_SUBHEAD_NUM, self.group_sep_text)
                    if len(m) == 0:
                        logger.error(f"No sutta number: {self.group_sep_text}")
                        return
                    else:

                        num = int(m[0])
                        self.set_ref(sutta=num)
                        return

                elif re.search(RE_CHAPTER_NUM, self.group_sep_text):
                    # class="chapter" with a number is a Section
                    # <p class="chapter">15. aṭṭhānapāl̤i</p>

                    m = re.findall(RE_CHAPTER_NUM, self.group_sep_text)
                    if len(m) == 0:
                        logger.error(f"No Section number: {self.group_sep_text}")
                        return
                    else:

                        num = int(m[0])
                        self.set_ref(section=num)
                        return

                elif re.search(RE_CHAPTER_PARENS_AND_NUM, self.group_sep_text):
                    # Some sections have a parens number
                    # <p class="chapter">(10) 5. upālivaggo</p>

                    m = re.findall(RE_CHAPTER_PARENS_AND_NUM, self.group_sep_text)
                    if len(m) == 0:
                        logger.error(f"No Section number: {self.group_sep_text}")
                        return
                    else:

                        num = int(m[0])
                        self.set_ref(section=num)
                        return

                elif re.search(RE_TITLE, self.group_sep_text):
                    # class="title" is a Part
                    # <p class="title">1. paṭhamapaṇṇāsakaṁ</p>
                    # Sometimes without a number
                    # <p class="book">sattakanipātapāl̤i</p>
                    # <a name="an7_1"></a>
                    # <p class="title">paṭhamapaṇṇāsakaṁ</p>

                    m = re.findall(RE_TITLE_NUM, self.group_sep_text)
                    if len(m) == 0:
                        if self.group_num == 1:

                            self.set_ref(part=1)
                            return

                        else:
                            logger.error(f"No Part number: {self.group_sep_text}")
                            return
                    else:

                        num = int(m[0])
                        self.set_ref(part=num)
                        return

                elif self.group_sep_text == '<p class="chapter">(11). rāgapeyyālaṁ</p>':

                    self.set_ref(sutta=3)
                    return

                elif self.group_sep_text == '<p class="subhead">nasevitabbādisuttāni</p>':

                    # This is a section heading.
                    self.ref = None
                    return

                else:
                    logger.error(f"Didn't parse sutta ref: {self.group_sep_text}")
                    return

            else:
                # No parent, this must be the root node.
                pass

        else:
            logger.error(f"Didn't parse sutta ref: {self.group_sep_text}")
            return


class SuttaCounters(TypedDict):
    cur_nikaya: str
    cur_book_str: str
    cur_samyutta_num: int
    cur_sutta_num: int


def get_nikaya_tree(group: Group, c: SuttaCounters) -> str:
    tree = ''

    for i in group.sub_groups:
        s, c = set_wisdom_pubs_ref(i, c)
        i = s
        tree += i.as_string_node() + "\n"
        if len(i.sub_groups) > 0:
            tree += get_nikaya_tree(i, c)

    return tree


def get_sutta_groups(group: Group) -> List[Group]:
    suttas = []

    for i in group.sub_groups:
        if i.ref is None:
            continue

        if i.ref['sutta_num'] is not None:
            suttas.append(i)

        if len(i.sub_groups) > 0:
            a = get_sutta_groups(i)
            suttas.extend(a)

    return suttas


def set_wisdom_pubs_ref(sutta: Group,
                        c: SuttaCounters) -> Tuple[Group, SuttaCounters]:

    if sutta.ref is None or sutta.parent_group is None:
        return (sutta, c)

    if sutta.ref['sutta_num'] is None:
        return (sutta, c)

    if c['cur_nikaya'] != sutta.ref['nikaya']:
        c['cur_nikaya'] = sutta.ref['nikaya']
        c['cur_sutta_num'] = 1
        c['cur_book_str'] = '1'

    if sutta.ref['nikaya'] == 'dn' \
        or sutta.ref['nikaya'] == 'mn':

        if sutta.parent_group.parent_group is None:
            return (sutta, c)

        if re.search('vaggo$', sutta.title):
            return (sutta, c)

        sutta.wisdom_pubs_ref = f"{sutta.ref['nikaya']}{c['cur_sutta_num']}"
        c['cur_sutta_num'] += 1

    elif sutta.ref['nikaya'] == 'an':

        book_str = re.sub(r'^an_(\d+)_.*', r'\1', sutta.ref['ref'])
        if c['cur_book_str'] != book_str:
            c['cur_book_str'] = book_str
            c['cur_sutta_num'] = 1

        sutta.wisdom_pubs_ref = f"{sutta.ref['nikaya']}{c['cur_book_str']}.{c['cur_sutta_num']}"
        c['cur_sutta_num'] += 1

    elif sutta.ref['nikaya'] == 'sn':

        if re.search(r'saṁyuttaṁ*$', sutta.parent_group.title):
            g = sutta.parent_group
        elif re.search(r'saṁyuttaṁ*$', sutta.parent_group.parent_group.title):
            g = sutta.parent_group.parent_group
        else:
            logger.error(f"Can't find book parent group for: {sutta.group_sep_text}")
            return (sutta, c)

        book_str = g.title
        if c['cur_book_str'] != book_str:
            c['cur_book_str'] = book_str
            c['cur_samyutta_num'] += 1
            c['cur_sutta_num'] = 1

        sutta.wisdom_pubs_ref = f"{sutta.ref['nikaya']}{c['cur_samyutta_num']}.{c['cur_sutta_num']}"
        c['cur_sutta_num'] += 1

    else:
        logger.error(f"Unrecognized nikaya: {sutta.ref['nikaya']}")

    return (sutta, c)

def get_mula_suttas() -> List[Group]:
    sutta_groups: List[Group] = []

    collection = 'mul'

    nikaya_files = {'dn': [], 'mn': [], 'sn': [], 'an': []}
    nikaya_combined_texts = {'dn': '', 'mn': '', 'sn': '', 'an': ''}
    nikaya_groups: dict[str, Group] = {}

    # Digha Nikaya
    nikaya_files['dn'] = [
        's0101m.mul.html',
        's0102m.mul.html',
        's0103m.mul.html',
    ]

    # Majjhima Nikaya
    nikaya_files['mn'] = [
        's0201m.mul.html',
        's0202m.mul.html',
        's0203m.mul.html',
    ]

    # Samyutta Nikaya
    nikaya_files['sn'] = [
        's0301m.mul.html',
        's0302m.mul.html',
        's0303m.mul.html',
        's0304m.mul.html',
        's0305m.mul.html',
    ]

    # Anguttara Nikaya
    nikaya_files['an'] = [
        's0401m.mul.html',
        's0402m1.mul.html',
        's0402m2.mul.html',
        's0402m3.mul.html',
        's0403m1.mul.html',
        's0403m2.mul.html',
        's0403m3.mul.html',
        's0404m1.mul.html',
        's0404m2.mul.html',
        's0404m3.mul.html',
        's0404m4.mul.html',
    ]

    for nikaya in nikaya_files.keys():
        # Concatenate HTML <body>
        for i in nikaya_files[nikaya]:
            html_path = HTML_DIR.joinpath(i)
            html_text = open(html_path, 'r', encoding='utf-8').read()

            soup = BeautifulSoup(html_text, 'html.parser')
            h = soup.find(name = 'body')
            if h is not None:
                body = h.decode_contents() # type: ignore
            else:
                logger.error("No <body> in %s" % html_path)
                sys.exit(1)

            nikaya_combined_texts[nikaya] += body

        # Determine groups within a nikaya
        nikaya_groups[nikaya] = Group(
            title = nikaya,
            group_text = nikaya_combined_texts[nikaya],
            group_num = None,
            parent_group = None, # root node has no parent
            ref = SuttaRef(
                ref = nikaya,
                nikaya = nikaya,
                collection = collection,
                book_num = None,
                part_num = None,
                section_num = None,
                sutta_num = None,
            )
        )

        nikaya_groups[nikaya].add_sub_groups(group_num=None)

    # Create a string representation of the group tree for testing and inspection

    for nikaya in nikaya_files.keys():
        c = SuttaCounters(
            cur_nikaya = '',
            cur_book_str = '1',
            cur_samyutta_num = 0,
            cur_sutta_num = 1,
        )
        text = get_nikaya_tree(nikaya_groups[nikaya], c)
        with open(TREES_DIR.joinpath(f"{nikaya}-tree.txt"), 'w') as f:
            f.write(text)

    for nikaya in nikaya_files.keys():
        a = get_sutta_groups(nikaya_groups[nikaya])
        sutta_groups.extend(a)

    return sutta_groups


def get_other_texts_body() -> List[Am.Sutta]:
    processed_already = [
        's0101m.mul.html',
        's0102m.mul.html',
        's0103m.mul.html',
        's0201m.mul.html',
        's0202m.mul.html',
        's0203m.mul.html',
        's0301m.mul.html',
        's0302m.mul.html',
        's0303m.mul.html',
        's0304m.mul.html',
        's0305m.mul.html',
        's0401m.mul.html',
        's0402m1.mul.html',
        's0402m2.mul.html',
        's0402m3.mul.html',
        's0403m1.mul.html',
        's0403m2.mul.html',
        's0403m3.mul.html',
        's0404m1.mul.html',
        's0404m2.mul.html',
        's0404m3.mul.html',
        's0404m4.mul.html',
    ]

    suttas: List[Am.Sutta] = []

    for p in glob.glob(f"{HTML_DIR.joinpath('*.html')}"):
        p = Path(p)
        if p.name in processed_already:
            continue

        # get body text
        html_text = open(p, 'r', encoding='utf-8').read()
        soup = BeautifulSoup(html_text, 'html.parser')
        h = soup.find(name = 'body')
        if h is not None:
            body = h.decode_contents() # type: ignore
        else:
            logger.error("No <body> in %s" % p)
            sys.exit(1)

        # pitaka = get_pitaka(p)
        # nikaya = get_nikaya(p)
        # collection = get_collection(p)

        lang = 'pli'
        author = 'cst4'

        uid = f"{p.stem}/{lang}/{author}"

        sutta = Am.Sutta(
            title = p.stem,
            uid = uid,
            sutta_ref = uid,
            language = lang,
            content_html = body,
            created_at = func.now(),
        )

        suttas.append(sutta)

    return suttas


def group_to_sutta(g: Group) -> Am.Sutta:
    if g.ref is None \
       or g.wisdom_pubs_ref is None \
       or g.group_sep_text is None \
       or g.parent_group is None:
        logger.error("Error in sutta attrs")
        sys.exit(1)

    # Sutta has at least one parent, add it as a header

    content_html = g.parent_group.group_sep_text \
        + g.group_sep_text \
        + g.group_text

    # an_2_5_11 > an2.5.11
    # ref = re.sub(r'^(dn|mn|sn|an)_(\d)(.*)', r'\1\2\3', g.ref['ref'])
    # ref = ref.replace('_', '.')

    # dn34, mn152
    ref = g.wisdom_pubs_ref

    lang = "pli"
    author = "cst4"
    uid = f"{ref}/{lang}/{author}"

    return Am.Sutta(
        title = g.title,
        title_pali = g.title,
        uid = uid,
        sutta_ref = helpers.uid_to_ref(ref),
        language = lang,
        content_html = content_html,
        created_at = func.now(),
    )

def populate_suttas_from_cst4(appdata_db: Session):

    sutta_groups = get_mula_suttas()
    suttas = list(map(group_to_sutta, sutta_groups))

    logger.info(f"Adding CST4 mula, count {len(suttas)} ...")

    uids = []

    try:
        for i in suttas:
            n = 0
            orig_uid = i.uid
            uid = orig_uid
            while uid in uids:
                logger.error(f"Double uid: {uid}")
                n += 1
                uid = f"{orig_uid}({n})"
                i.uid = uid # type: ignore

            uids.append(i.uid)

            appdata_db.add(i)
            appdata_db.commit()
    except Exception as e:
        logger.error(e)
        exit(1)

    # suttas = get_other_texts_body()

    # logger.info(f"Adding CST4 remaining texts as html <body>, count {len(suttas)} ...")

    # try:
    #     for i in suttas:
    #         appdata_db.add(i)
    #         appdata_db.commit()
    # except Exception as e:
    #     logger.error(e)
    #     exit(1)

def main():
    logger.info("Extract suttas from CST4", start_new=True)

    sutta_groups = get_mula_suttas()

    logger.info(f"Count: {len(sutta_groups)}")

if __name__ == "__main__":
    main()
