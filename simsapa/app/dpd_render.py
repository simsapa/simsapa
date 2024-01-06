from typing import Dict, List, Set, Tuple, Optional
import json, re

from mako.template import Template

from sqlalchemy.orm import object_session

from simsapa.app.db_session import get_db_session_with_schema
from simsapa.app.db import dpd_models as Dpd

from simsapa.app.db.dpd_models import FamilyCompound, FamilySet, PaliRoot, PaliWord, DerivedData, FamilyRoot, FamilyWord
from simsapa.app.helpers import strip_html, root_info_clean_plaintext

from simsapa.dpd_db.exporter.export_dpd import PaliWordDbParts, PaliWordDbRowItems, PaliWordRenderData, PaliWordTemplates, get_family_compounds_for_pali_word, get_family_set_for_pali_word, render_example_templ, render_family_compound_templ, render_family_root_templ, render_family_set_templ, render_family_word_templ, render_feedback_templ, render_frequency_templ, render_grammar_templ, render_inflection_templ
from simsapa.dpd_db.exporter.export_roots import render_root_buttons_templ, render_root_definition_templ, render_root_families_templ, render_root_info_templ, render_root_matrix_templ
from simsapa.dpd_db.exporter.helpers import EXCLUDE_FROM_FREQ, TODAY

from simsapa.dpd_db.tools.meaning_construction import make_meaning_html, summarize_construction
from simsapa.dpd_db.tools.niggahitas import add_niggahitas
from simsapa.dpd_db.tools.pos import CONJUGATIONS, DECLENSIONS
from simsapa.dpd_db.tools.utils import RenderResult
from simsapa.dpd_db.tools.paths import ProjectPaths
from simsapa.dpd_db.tools.sandhi_contraction import SandhiContractions

from simsapa import DPD_DB_PATH, DbSchemaName, DetailsTab, logger

DPD_PROJECT_PATHS = ProjectPaths()
DPD_PALI_WORD_TEMPLATES = PaliWordTemplates(DPD_PROJECT_PATHS)

DPD_CF_SET: Optional[Set[str]] = None
DPD_ROOTS_COUNT_DICT: Optional[Dict[str, int]] = None
DPD_SANDHI_CONTRACTIONS: Optional[SandhiContractions] = None

def get_dpd_caches() -> Tuple[Set[str], Dict[str, int], SandhiContractions]:
    logger.info("get_dpd_caches()")

    db_eng, db_conn, db_session = get_db_session_with_schema(DPD_DB_PATH, DbSchemaName.Dpd)

    dpd_cf_set: Set[str] = set()
    dpd_sandhi_contractions: SandhiContractions = dict()

    # === cf_set ===

    r = db_session.query(Dpd.DbInfo) \
                  .filter(Dpd.DbInfo.key == "cf_set") \
                  .first()

    assert(r is not None)

    dpd_cf_set = set(json.loads(str(r.value)))

    # === roots_count_dict ===

    r = db_session.query(Dpd.DbInfo) \
                  .filter(Dpd.DbInfo.key == "roots_count_dict") \
                  .first()

    assert(r is not None)

    dpd_roots_count_dict = json.loads(str(r.value))

    # === sandhi_contractions ===

    r = db_session.query(Dpd.DbInfo) \
                  .filter(Dpd.DbInfo.key == "sandhi_contractions") \
                  .first()

    assert(r is not None)

    data = json.loads(str(r.value))
    dpd_sandhi_contractions: SandhiContractions = dict()
    for k, v in data.items():
        dpd_sandhi_contractions[k] = {
            'contractions': set(v['contractions']),
            'ids': v['ids'],
        }

    db_conn.close()
    db_session.close()
    db_eng.dispose()

    return (dpd_cf_set, dpd_roots_count_dict, dpd_sandhi_contractions)

def _get_render_data() -> PaliWordRenderData:
    global DPD_CF_SET
    global DPD_ROOTS_COUNT_DICT
    global DPD_SANDHI_CONTRACTIONS
    if DPD_CF_SET is None or DPD_ROOTS_COUNT_DICT is None or DPD_SANDHI_CONTRACTIONS is None:
        DPD_CF_SET, DPD_ROOTS_COUNT_DICT, DPD_SANDHI_CONTRACTIONS = get_dpd_caches()

    render_data = PaliWordRenderData(
        pth = DPD_PROJECT_PATHS,
        word_templates = DPD_PALI_WORD_TEMPLATES,
        sandhi_contractions = DPD_SANDHI_CONTRACTIONS,
        cf_set = DPD_CF_SET,
        roots_count_dict = DPD_ROOTS_COUNT_DICT,
        make_link = True,
    )

    return render_data

def make_meaning_plaintext(i: PaliWord) -> str:
    """Compile plaintext of meaning_1 and literal meaning, or return meaning_2."""

    if i.meaning_1:
        meaning: str = f"{i.meaning_1}"
        if i.meaning_lit:
            meaning += f"; lit. {i.meaning_lit}"
        return meaning
    else:
        return i.meaning_2

def render_dpd_definition_plaintext_templ(i: PaliWord, dpd_definition_templ: Template) -> str:
    plus_case: str = ""
    if i.plus_case is not None and i.plus_case:
        plus_case: str = i.plus_case

    meaning = make_meaning_plaintext(i)
    summary = summarize_construction(i)

    return str(
        dpd_definition_templ.render(
            i=i,
            pos=i.pos,
            plus_case=plus_case,
            meaning=meaning,
            summary=summary,
            complete=""))

def render_grammar_plaintext_templ(i: PaliWord, grammar_templ: Template) -> str:
    """plaintext grammatical information"""

    if i.meaning_1 is not None and i.meaning_1:
        grammar = i.grammar
        if i.neg:
            grammar += f", {i.neg}"
        if i.verb:
            grammar += f", {i.verb}"
        if i.trans:
            grammar += f", {i.trans}"
        if i.plus_case:
            grammar += f" ({i.plus_case})"

        return str(
            grammar_templ.render(
                i=i,
                rt=i.rt,
                grammar=grammar,
                meaning=make_meaning_plaintext(i),
                today=TODAY))

    else:
        return ""

def pali_word_index_plaintext(pali_word: PaliWord) -> str:
    db_session = object_session(pali_word)
    assert(db_session is not None)

    tt = DPD_PALI_WORD_TEMPLATES

    plaintext = ""

    definition = render_dpd_definition_plaintext_templ(pali_word, tt.dpd_definition_plaintext_templ)
    plaintext += definition.replace("\n", " ")

    plaintext += "\n\n"

    grammar = render_grammar_plaintext_templ(pali_word, tt.grammar_plaintext_templ)
    plaintext += grammar.replace("\n", " ")

    return plaintext

def pali_root_index_plaintext(pali_root: PaliRoot) -> str:
    rd = _get_render_data()

    plaintext = ""

    definition = strip_html(render_root_definition_templ(rd['pth'], pali_root, rd['roots_count_dict'], plaintext=True))

    plaintext += definition.replace("\n", " ")

    plaintext += "\n\n"

    html = render_root_info_templ(rd['pth'], pali_root, [DetailsTab.RootInfo], plaintext=True)
    root_info = root_info_clean_plaintext(html)
    plaintext += root_info.replace("\n", " ")

    return plaintext

def pali_word_dpd_html(pali_word: PaliWord, open_details: List[DetailsTab] = []) -> RenderResult:
    db_session = object_session(pali_word)
    assert(db_session is not None)

    dpd_db = db_session.query(
        PaliWord, DerivedData, FamilyRoot, FamilyWord
    ).outerjoin(
        DerivedData,
        PaliWord.id == DerivedData.id
    ).outerjoin(
        FamilyRoot,
        PaliWord.root_family_key == FamilyRoot.root_family_key
    ).outerjoin(
        FamilyWord,
        PaliWord.family_word == FamilyWord.word_family
    ) \
                       .filter(PaliWord.id == pali_word.id) \
                       .first()

    assert(dpd_db is not None)

    def _add_parts(i: PaliWordDbRowItems) -> PaliWordDbParts:
        pw: PaliWord
        dd: DerivedData
        fr: FamilyRoot
        fw: FamilyWord
        pw, dd, fr, fw = i

        return PaliWordDbParts(
            pali_word = pw,
            pali_root = pw.rt,
            derived_data = dd,
            family_root = fr,
            family_word = fw,
            family_compounds = get_family_compounds_for_pali_word(pw),
            family_set = get_family_set_for_pali_word(pw),
        )

    db_parts = _add_parts(dpd_db._tuple())

    render_res = render_pali_word_dpd_simsapa_html(db_parts, _get_render_data(), open_details)

    return render_res

def pali_root_dpd_html(pali_root: PaliRoot, open_details: List[DetailsTab] = []) -> RenderResult:
    return render_pali_root_dpd_simsapa_html(pali_root, _get_render_data(), open_details)

def render_button_box_simsapa_templ(
        i: PaliWord,
        cf_set: Set[str],
        button_box_templ: Template,
        open_details: List[DetailsTab] = []
) -> str:
    """render buttons for each section of the dictionary"""

    button_html = """
    <button class="ssp-button {active}" onclick="document.SSP.button_toggle_visible(this, '#{target}')">
        <svg class="ssp-icon-button__icon"><use xlink:href="#{icon}"></use></svg>
        <span class="ssp-button-text">{name}</span>
    </button>
    """

    # example_button
    if i.meaning_1 and i.example_1 and not i.example_2:
        active = "active" if DetailsTab.Examples in open_details else ""
        example_button = button_html.format(
            target=f"example_{i.pali_1_}", name="Example", icon="icon-card-text", active=active)
    else:
        example_button = ""

    # examples_button
    if i.meaning_1 and i.example_1 and i.example_2:
        active = "active" if DetailsTab.Examples in open_details else ""
        examples_button = button_html.format(
            target=f"examples_{i.pali_1_}", name="Examples", icon="icon-card-text", active=active)
    else:
        examples_button = ""

    # conjugation_button
    if i.pos in CONJUGATIONS:
        active = "active" if DetailsTab.Inflections in open_details else ""
        conjugation_button = button_html.format(
            target=f"conjugation_{i.pali_1_}", name="Conjugation", icon="icon-table-bold", active=active)
    else:
        conjugation_button = ""

    # declension_button
    if i.pos in DECLENSIONS:
        active = "active" if DetailsTab.Inflections in open_details else ""
        declension_button = button_html.format(
            target=f"declension_{i.pali_1_}", name="Declensions", icon="icon-table-bold", active=active)
    else:
        declension_button = ""

    # root_family_button
    if i.family_root:
        active = "active" if DetailsTab.RootFamily in open_details else ""
        root_family_button = button_html.format(
            target=f"root_family_{i.pali_1_}", name="Root Family", icon="icon-list-bullets-bold", active=active)
    else:
        root_family_button = ""

    # word_family_button
    if i.family_word:
        active = "active" if DetailsTab.WordFamily in open_details else ""
        word_family_button = button_html.format(
            target=f"word_family_{i.pali_1_}", name="Word Family", icon="icon-list-bullets-bold", active=active)
    else:
        word_family_button = ""

    # compound_family_button
    if (
        i.meaning_1 and
        (
            # sometimes there's an empty compound family, so
            any(item in cf_set for item in i.family_compound_list) or
            # add a button to the word itself
            i.pali_clean in cf_set)
    ):

        active = "active" if DetailsTab.CompoundFamily in open_details else ""
        if i.family_compound is not None and " " not in i.family_compound:
            compound_family_button = button_html.format(
                target=f"compound_family_{i.pali_1_}", name="Compound Family", icon="icon-list-bullets-bold", active=active)

        else:
            compound_family_button = button_html.format(
                target=f"compound_family_{i.pali_1_}", name="Compound Familes", icon="icon-list-bullets-bold", active=active)

    else:
        compound_family_button = ""

    # set_family_button
    if (i.meaning_1 and
            i.family_set):

        if len(i.family_set_list) > 0:
            active = "active" if DetailsTab.SetFamily in open_details else ""
            set_family_button = button_html.format(
                target=f"set_family_{i.pali_1_}", name="Set", icon="icon-list-bullets-bold", active=active)
        else:
            set_family_button = ""
    else:
        set_family_button = ""

    # frequency_button
    if i.pos not in EXCLUDE_FROM_FREQ:
        active = "active" if DetailsTab.FrequencyMap in open_details else ""
        frequency_button = button_html.format(
            target=f"frequency_{i.pali_1_}", name="Frequency", icon="icon-table-bold", active=active)
    else:
        frequency_button = ""

    # feedback_button
    active = "active" if DetailsTab.Feedback in open_details else ""
    feedback_button = button_html.format(
        target=f"feedback_{i.pali_1_}", name="Feedback", icon="icon-send-email", active=active)

    return str(
        button_box_templ.render(
            div_id=f"button_box_{i.pali_1_}",
            example_button=example_button,
            examples_button=examples_button,
            conjugation_button=conjugation_button,
            declension_button=declension_button,
            root_family_button=root_family_button,
            word_family_button=word_family_button,
            compound_family_button=compound_family_button,
            set_family_button=set_family_button,
            frequency_button=frequency_button,
            feedback_button=feedback_button))

def degree_of_completion_simsapa(i):
    """Return html styled symbol of a word data degree of completion."""
    if i.meaning_1:
        if i.source_1:
            return """<span class="gray" title="This word has been reviewed and confirmed.">✓</span>"""
        else:
            return """<span class="gray" title="This word has been reviewed but not yet confirmed.">~</span>"""
    else:
        return """<span class="gray" title="This word has been added but the details are preliminary.">✗</span>"""

def render_dpd_definition_simsapa_templ(i: PaliWord, dpd_definition_templ: Template) -> str:
    """render the definition of a word's most relevant information:
    1. pos
    2. case
    3 meaning
    4. summary
    5. degree of completition"""

    # pos
    pos: str = i.pos

    # plus_case
    plus_case: str = ""
    if i.plus_case is not None and i.plus_case:
        plus_case: str = i.plus_case

    meaning = make_meaning_html(i)
    summary = summarize_construction(i)
    complete = degree_of_completion_simsapa(i)

    return str(
        dpd_definition_templ.render(
            i=i,
            pos=pos,
            plus_case=plus_case,
            meaning=meaning,
            summary=summary,
            complete=complete))

def render_pali_word_dpd_simsapa_html(db_parts: PaliWordDbParts,
                                      render_data: PaliWordRenderData,
                                      open_details: List[DetailsTab] = []) -> RenderResult:
    rd = render_data

    i: PaliWord = db_parts["pali_word"]
    rt: PaliRoot = db_parts["pali_root"]
    dd: DerivedData = db_parts["derived_data"]
    fr: FamilyRoot = db_parts["family_root"]
    fw: FamilyWord = db_parts["family_word"]
    fc: List[FamilyCompound] = db_parts["family_compounds"]
    fs: List[FamilySet] = db_parts["family_set"]

    tt = rd['word_templates']
    pth = rd['pth']
    sandhi_contractions = rd['sandhi_contractions']

    # replace \n with html line break
    if i.meaning_1:
        i.meaning_1 = i.meaning_1.replace("\n", "<br>")
    if i.sanskrit:
        i.sanskrit = i.sanskrit.replace("\n", "<br>")
    if i.phonetic:
        i.phonetic = i.phonetic.replace("\n", "<br>")
    if i.compound_construction:
        i.compound_construction = i.compound_construction.replace("\n", "<br>")
    if i.commentary:
        i.commentary = i.commentary.replace("\n", "<br>")
    if i.link:
        i.link = i.link.replace("\n", "<br>")
    if i.sutta_1:
        i.sutta_1 = i.sutta_1.replace("\n", "<br>")
    if i.sutta_2:
        i.sutta_2 = i.sutta_2.replace("\n", "<br>")
    if i.example_1:
        i.example_1 = i.example_1.replace("\n", "<br>")
    if i.example_2:
        i.example_2 = i.example_2.replace("\n", "<br>")

    html = ""

    summary = render_dpd_definition_simsapa_templ(i, tt.dpd_definition_templ)
    html += summary

    # In Simsapa, show the grammar info without needing button clicks.
    grammar = render_grammar_templ(pth, i, rt, tt.grammar_simsapa_templ)
    html += grammar

    if len(open_details) > 0:
        more_info_active = "active"
        details_classes = ""
    else:
        more_info_active = ""
        details_classes = "hidden"

    more_info_button = f"""
    <div class="more-info-button-wrap">
        <div class="more-info-button-box">
            <button
                title="Show details"
                class="ssp-button ssp-icon-button {more_info_active}"
                onclick="document.SSP.button_toggle_visible(this, '#word_details_{i.pali_1_}')"
            >
                <svg class="ssp-icon-button__icon"><use xlink:href="#icon-more-filled"></use></svg>
            </button>
        </div>
    </div>
    """

    html += more_info_button

    html += f"<div id='word_details_{i.pali_1_}' class='{details_classes}'>"

    button_box = render_button_box_simsapa_templ(i, rd['cf_set'], tt.button_box_simsapa_templ, open_details)
    html += button_box

    example = render_example_templ(pth, i, rd['make_link'], tt.example_templ, open_details)
    html += example

    inflection_table = render_inflection_templ(pth, i, dd, tt.inflection_templ, open_details)
    html += inflection_table

    family_root = render_family_root_templ(pth, i, fr, tt.family_root_templ, open_details)
    html += family_root

    family_word = render_family_word_templ(pth, i, fw, tt.family_word_templ, open_details)
    html += family_word

    family_compound = render_family_compound_templ(pth, i, fc, rd['cf_set'], tt.family_compound_templ, open_details)
    html += family_compound

    family_sets = render_family_set_templ(pth, i, fs, tt.family_set_templ, open_details)
    html += family_sets

    frequency = render_frequency_templ(pth, i, dd, tt.frequency_templ, open_details)
    html += frequency

    feedback = render_feedback_templ(pth, i, tt.feedback_templ, open_details)
    html += feedback

    html += "</div>"

    # FIXME improve specifying Simsapa and DPD version in feedback link
    # Example feedback form link in DPD:
    # https://docs.google.com/forms/d/e/1FAIpQLSf9boBe7k5tCwq7LdWgBHHGIPVc4ROO5yjVDo1X5LDAxkmGWQ/viewform?usp=pp_url&entry.438735500=${i.pali_link}&entry.1433863141=GoldenDict+${today}
    html = html.replace("&entry.1433863141=GoldenDict+", "&entry.1433863141=Simsapa+")

    synonyms: List[str] = dd.inflections_list
    synonyms = add_niggahitas(synonyms)
    for synonym in synonyms:
        if synonym in sandhi_contractions:
            contractions = sandhi_contractions[synonym]["contractions"]
            synonyms.extend(contractions)
    synonyms += dd.sinhala_list
    synonyms += dd.devanagari_list
    synonyms += dd.thai_list
    synonyms += i.family_set_list
    synonyms += [str(i.id)]

    res = RenderResult(
        word = i.pali_1,
        definition_html = html,
        definition_plain = "",
        synonyms = synonyms,
    )

    return res

def render_pali_root_dpd_simsapa_html(r: PaliRoot,
                                      render_data: PaliWordRenderData,
                                      open_details: List[DetailsTab] = []) -> RenderResult:
    # Compare with
    # exporter/export_roots.py::generate_root_html()

    db_session = object_session(r)
    assert(db_session is not None)

    rd = render_data
    pth = rd['pth']

    # replace \n with html line break
    if r.panini_root:
        r.panini_root = r.panini_root.replace("\n", "<br>")
    if r.panini_sanskrit:
        r.panini_sanskrit = r.panini_sanskrit.replace("\n", "<br>")
    if r.panini_english:
        r.panini_english = r.panini_english.replace("\n", "<br>")

    html = ""

    definition = render_root_definition_templ(pth, r, rd['roots_count_dict'])
    html += definition

    root_buttons = render_root_buttons_templ(pth, r, db_session, open_details)
    html += root_buttons

    root_info = render_root_info_templ(pth, r, open_details)
    html += root_info

    root_matrix = render_root_matrix_templ(pth, r, rd['roots_count_dict'])
    html += root_matrix

    root_families = render_root_families_templ(pth, r, db_session)
    html += root_families

    # FIXME improve specifying Simsapa and DPD version in feedback link
    # Example feedback form link in DPD:
    # https://docs.google.com/forms/d/e/1FAIpQLSf9boBe7k5tCwq7LdWgBHHGIPVc4ROO5yjVDo1X5LDAxkmGWQ/viewform?usp=pp_url&entry.438735500=${i.pali_link}&entry.1433863141=GoldenDict+${today}
    html = html.replace("&entry.1433863141=GoldenDict+", "&entry.1433863141=Simsapa+")

    synonyms: set = set()
    synonyms.add(r.root_clean)
    synonyms.add(re.sub("√", "", r.root))
    synonyms.add(re.sub("√", "", r.root_clean))

    frs = db_session.query(FamilyRoot)\
        .filter(FamilyRoot.root_key == r.root).all()

    for fr in frs:
        synonyms.add(fr.root_family)
        synonyms.add(re.sub("√", "", fr.root_family))

    synonyms = set(add_niggahitas(list(synonyms)))

    res = RenderResult(
        word = r.root,
        definition_html = html,
        definition_plain = "",
        synonyms = list(synonyms),
    )

    return res
