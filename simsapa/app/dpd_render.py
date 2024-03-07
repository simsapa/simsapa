from typing import List, Set
import re

from mako.template import Template

from sqlalchemy.orm import object_session

from simsapa.app.db.dpd_models import FamilyCompound, FamilySet, FamilyIdiom, DpdRoots, DpdHeadwords, FamilyRoot, FamilyWord, Russian, SBS, DPD_PALI_WORD_TEMPLATES, get_render_data
from simsapa.app.helpers import strip_html, root_info_clean_plaintext

from simsapa.dpd_db.exporter.export_dpd import DpdHeadwordsDbParts, DpdHeadwordsDbRowItems, DpdHeadwordsRenderData, render_example_templ, render_family_compound_templ, render_family_idioms_templ, render_family_root_templ, render_family_set_templ, render_family_word_templ, render_feedback_templ, render_frequency_templ, render_grammar_templ, render_inflection_templ
from simsapa.dpd_db.exporter.export_roots import render_root_buttons_templ, render_root_definition_templ, render_root_families_templ, render_root_info_templ, render_root_matrix_templ

from simsapa.dpd_db.tools.exporter_functions import get_family_compounds, get_family_idioms, get_family_set

from simsapa.dpd_db.tools.meaning_construction import degree_of_completion, make_meaning_html, summarize_construction
from simsapa.dpd_db.tools.niggahitas import add_niggahitas
# from simsapa.dpd_db.tools.pos import CONJUGATIONS, DECLENSIONS, EXCLUDE_FROM_FREQ
from simsapa.dpd_db.tools.utils import RenderResult

from simsapa import DetailsTab, TODAY

def make_meaning_plaintext(i: DpdHeadwords) -> str:
    """Compile plaintext of meaning_1 and literal meaning, or return meaning_2."""

    if i.meaning_1:
        meaning: str = f"{i.meaning_1}"
        if i.meaning_lit:
            meaning += f"; lit. {i.meaning_lit}"
        return meaning
    else:
        return i.meaning_2

def render_dpd_definition_plaintext_templ(i: DpdHeadwords, dpd_definition_templ: Template) -> str:
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

def render_grammar_plaintext_templ(i: DpdHeadwords, grammar_templ: Template) -> str:
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

def pali_word_index_plaintext(pali_word: DpdHeadwords) -> str:
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

def pali_root_index_plaintext(pali_root: DpdRoots) -> str:
    rd = get_render_data()

    plaintext = ""

    definition = strip_html(render_root_definition_templ(rd['pth'], pali_root, rd['roots_count_dict'], False, plaintext=True))

    plaintext += definition.replace("\n", " ")

    plaintext += "\n\n"

    html = render_root_info_templ(rd['pth'], pali_root, [DetailsTab.RootInfo], plaintext=True)
    root_info = root_info_clean_plaintext(html)
    plaintext += root_info.replace("\n", " ")

    return plaintext

def pali_word_dpd_html(pali_word: DpdHeadwords, open_details: List[DetailsTab] = []) -> RenderResult:
    db_session = object_session(pali_word)
    assert(db_session is not None)

    dpd_db = db_session.query(
        DpdHeadwords, FamilyRoot, FamilyWord, SBS, Russian
    ).outerjoin(
        SBS,
        DpdHeadwords.id == SBS.id
    ).outerjoin(
        Russian,
        DpdHeadwords.id == Russian.id
    ).outerjoin(
        FamilyRoot,
        DpdHeadwords.root_family_key == FamilyRoot.root_family_key
    ).outerjoin(
        FamilyWord,
        DpdHeadwords.family_word == FamilyWord.word_family
    ) \
                       .filter(DpdHeadwords.id == pali_word.id) \
                       .first()

    assert(dpd_db is not None)

    def _add_parts(i: DpdHeadwordsDbRowItems) -> DpdHeadwordsDbParts:
        pw: DpdHeadwords
        fr: FamilyRoot
        fw: FamilyWord
        sbs: SBS
        ru: Russian
        pw, fr, fw, sbs, ru = i

        return DpdHeadwordsDbParts(
            pali_word = pw,
            pali_root = pw.rt,
            sbs = sbs,
            ru = ru,
            family_root = fr,
            family_word = fw,
            family_compounds = get_family_compounds(pw),
            family_idioms = get_family_idioms(pw),
            family_set = get_family_set(pw),
        )

    db_parts = _add_parts(dpd_db._tuple())

    render_res = render_pali_word_dpd_simsapa_html(db_parts, get_render_data(), open_details)

    return render_res

def pali_root_dpd_html(pali_root: DpdRoots, open_details: List[DetailsTab] = []) -> RenderResult:
    return render_pali_root_dpd_simsapa_html(pali_root, get_render_data(), open_details)

def render_button_box_simsapa_templ(
        i: DpdHeadwords,
        __sbs__: SBS,
        __dps_data__: bool,
        __cf_set__: Set[str],
        __idioms_set__: Set[str],
        button_box_templ: Template,
        open_details: List[DetailsTab] = []
) -> str:
    """render buttons for each section of the dictionary"""

    button_html = """
    <button class="ssp-button {active}"
            onclick="document.SSP.button_toggle_visible(this, '#{target}')"
    >
        <svg class="ssp-icon-button__icon"><use xlink:href="#{icon}"></use></svg>
        <span class="ssp-button-text">{name}</span>
    </button>
    """

    # button_link_html = (
    #     '<a class="button" '
    #     'href="{href}" '
    #     'style="text-decoration: none;">{name}</a>')

    # NOTE: in Simsapa the grammar is rendered as part of the definition.
    # # grammar_button
    # if i.needs_grammar_button or dps_data:
    #     grammar_button = button_html.format(
    #         target=f"grammar_{i.lemma_1_}", name="grammar")
    # else:
    #     grammar_button = ""

    # example_button
    if i.needs_example_button:
        active = "active" if DetailsTab.Examples in open_details else ""
        example_button = button_html.format(
            target=f"example_{i.lemma_1_}", name="Example", icon="icon-card-text", active=active)
    else:
        example_button = ""

    # examples_button
    if i.needs_examples_button:
        active = "active" if DetailsTab.Examples in open_details else ""
        examples_button = button_html.format(
            target=f"examples_{i.lemma_1_}", name="Examples", icon="icon-card-text", active=active)
    else:
        examples_button = ""

    # # sbs_example_button
    # sbs_example_button = ""
    # if (
    #     dps_data and
    #     sbs and
    #     (sbs.needs_sbs_example_button or sbs.needs_sbs_examples_button)
    # ):
    #     sbs_example_button = button_html.format(
    #         target=f"sbs_example_{i.lemma_1_}", name="<b>SBS</b>")

    # conjugation_button
    if i.needs_conjugation_button:
        active = "active" if DetailsTab.Inflections in open_details else ""
        conjugation_button = button_html.format(
            target=f"conjugation_{i.lemma_1_}", name="Conjugation", icon="icon-table-bold", active=active)
    else:
        conjugation_button = ""

    # declension_button
    if i.needs_declension_button:
        active = "active" if DetailsTab.Inflections in open_details else ""
        declension_button = button_html.format(
            target=f"declension_{i.lemma_1_}", name="Declensions", icon="icon-table-bold", active=active)
    else:
        declension_button = ""

    # root_family_button
    if i.needs_root_family_button:
        active = "active" if DetailsTab.RootFamily in open_details else ""
        root_family_button = button_html.format(
            target=f"root_family_{i.lemma_1_}", name="Root Family", icon="icon-list-bullets-bold", active=active)
    else:
        root_family_button = ""

    # word_family_button
    if i.needs_word_family_button:
        active = "active" if DetailsTab.WordFamily in open_details else ""
        word_family_button = button_html.format(
            target=f"word_family_{i.lemma_1_}", name="Word Family", icon="icon-list-bullets-bold", active=active)
    else:
        word_family_button = ""

    # compound_family_button
    if i.needs_compound_family_button:
        active = "active" if DetailsTab.CompoundFamily in open_details else ""
        compound_family_button = button_html.format(
            target=f"compound_family_{i.lemma_1_}", name="Compound Family", icon="icon-list-bullets-bold", active=active)

    elif i.needs_compound_families_button:
        active = "active" if DetailsTab.CompoundFamily in open_details else ""
        compound_family_button = button_html.format(
            target=f"compound_families_{i.lemma_1_}", name="Compound familes", icon="icon-list-bullets-bold", active=active)

    else:
        compound_family_button = ""

    # idioms button
    if i.needs_idioms_button:
        active = "active" if DetailsTab.IdiomFamily in open_details else ""
        idioms_button = button_html.format(
            target=f"idioms_{i.lemma_1_}", name="Idioms", icon="icon-list-bullets-bold", active=active)
    else:
        idioms_button = ""

    # set_family_button
    if i.needs_set_button:
        active = "active" if DetailsTab.SetFamily in open_details else ""
        set_family_button = button_html.format(
            target=f"set_family_{i.lemma_1_}", name="Set", icon="icon-list-bullets-bold", active=active)

    elif i.needs_sets_button:
        active = "active" if DetailsTab.SetFamily in open_details else ""
        set_family_button = button_html.format(
            target=f"set_families_{i.lemma_1_}", name="Sets", icon="icon-list-bullets-bold", active=active)

    else:
        set_family_button = ""

    # frequency_button
    if i.needs_frequency_button:
        active = "active" if DetailsTab.FrequencyMap in open_details else ""
        frequency_button = button_html.format(
            target=f"frequency_{i.lemma_1_}", name="Frequency", icon="icon-table-bold", active=active)
    else:
        frequency_button = ""

    # feedback_button
    active = "active" if DetailsTab.Feedback in open_details else ""
    feedback_button = button_html.format(
        target=f"feedback_{i.lemma_1_}", name="Feedback", icon="icon-send-email", active=active)

    return str(
        button_box_templ.render(
            div_id=f"button_box_{i.lemma_1_}",
            grammar_button="",
            example_button=example_button,
            examples_button=examples_button,
            sbs_example_button="",
            conjugation_button=conjugation_button,
            declension_button=declension_button,
            root_family_button=root_family_button,
            word_family_button=word_family_button,
            compound_family_button=compound_family_button,
            idioms_button=idioms_button,
            set_family_button=set_family_button,
            frequency_button=frequency_button,
            feedback_button=feedback_button))

def render_dpd_definition_simsapa_templ(
        i: DpdHeadwords,
        dpd_definition_templ: Template,
        make_link: bool,
        show_id: bool,
        show_ebt_count: bool,
        dps_data: bool,
        sbs: SBS|None,
) -> str:
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
    complete = degree_of_completion(i)

    # id
    id: int = i.id

    # ebt_count
    ebt_count: int = i.ebt_count

    return str(
        dpd_definition_templ.render(
            i=i,
            sbs=sbs,
            make_link=make_link,
            pos=pos,
            plus_case=plus_case,
            meaning=meaning,
            summary=summary,
            complete=complete,
            id=id,
            show_id=show_id,
            show_ebt_count=show_ebt_count,
            dps_data=dps_data,
            ebt_count=ebt_count,
            )
        )


def render_pali_word_dpd_simsapa_html(db_parts: DpdHeadwordsDbParts,
                                      render_data: DpdHeadwordsRenderData,
                                      open_details: List[DetailsTab] = []) -> RenderResult:
    extended_synonyms = True
    dps_data = False
    rd = render_data

    i: DpdHeadwords = db_parts["pali_word"]
    rt: DpdRoots = db_parts["pali_root"]
    sbs: SBS = db_parts["sbs"]
    ru: Russian = db_parts["ru"]
    fr: FamilyRoot = db_parts["family_root"]
    fw: FamilyWord = db_parts["family_word"]
    fc: List[FamilyCompound] = db_parts["family_compounds"]
    fi: List[FamilyIdiom] = db_parts["family_idioms"]
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
    if dps_data and sbs:
        if sbs.sbs_sutta_1:
            sbs.sbs_sutta_1 = sbs.sbs_sutta_1.replace("\n", "<br>")
        if sbs.sbs_sutta_2:
            sbs.sbs_sutta_2 = sbs.sbs_sutta_2.replace("\n", "<br>")
        if sbs.sbs_sutta_3:
            sbs.sbs_sutta_3 = sbs.sbs_sutta_3.replace("\n", "<br>")
        if sbs.sbs_sutta_4:
            sbs.sbs_sutta_4 = sbs.sbs_sutta_4.replace("\n", "<br>")
        if sbs.sbs_example_1:
            sbs.sbs_example_1 = sbs.sbs_example_1.replace("\n", "<br>")
        if sbs.sbs_example_2:
            sbs.sbs_example_2 = sbs.sbs_example_2.replace("\n", "<br>")
        if sbs.sbs_example_3:
            sbs.sbs_example_3 = sbs.sbs_example_3.replace("\n", "<br>")
        if sbs.sbs_example_4:
            sbs.sbs_example_4 = sbs.sbs_example_4.replace("\n", "<br>")

    html: str = ""
    # header = render_header_templ(pth, tt.dpd_css, tt.button_js, tt.header_templ)
    # html += header
    # size_dict["dpd_header"] += len(header)

    summary = render_dpd_definition_simsapa_templ(i, tt.dpd_definition_templ, rd['make_link'], rd["show_id"], rd['show_ebt_count'], rd['dps_data'], sbs)
    html += summary

    # NOTE: In Simsapa, show the grammar info without needing button clicks.
    grammar = render_grammar_templ(pth, i, rt, sbs, ru, rd['dps_data'], tt.grammar_simsapa_templ)
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
                onclick="document.SSP.button_toggle_visible(this, '#word_details_{i.lemma_1_}')"
            >
                <svg class="ssp-icon-button__icon"><use xlink:href="#icon-more-filled"></use></svg>
            </button>
        </div>
    </div>
    """

    html += more_info_button

    html += f"<div id='word_details_{i.lemma_1_}' class='{details_classes}'>"

    button_box = render_button_box_simsapa_templ(
        i, sbs, rd['dps_data'], rd['cf_set'], rd['idioms_set'], tt.button_box_simsapa_templ, open_details)
    html += button_box
    # size_dict["dpd_button_box"] += len(button_box)

    # NOTE: In Simsapa the grammar is rendered as part of the definition.
    # if i.needs_grammar_button or dps_data:
    #     grammar = render_grammar_templ(pth, i, rt, sbs, ru, rd['dps_data'], tt.grammar_templ)
    #     html += grammar
    #     # size_dict["dpd_grammar"] += len(grammar)

    if i.needs_example_button or i.needs_examples_button:
        example = render_example_templ(pth, i, rd['make_link'], tt.example_templ, open_details)
        html += example
        # size_dict["dpd_example"] += len(example)

    if i.needs_conjugation_button or i.needs_declension_button:
        inflection_table = render_inflection_templ(pth, i, tt.inflection_templ, open_details)
        html += inflection_table
        # size_dict["dpd_inflection_table"] += len(inflection_table)

    if i.needs_root_family_button:
        family_root = render_family_root_templ(pth, i, fr, tt.family_root_templ, open_details)
        html += family_root
        # size_dict["dpd_family_root"] += len(family_root)

    if i.needs_word_family_button:
        family_word = render_family_word_templ(pth, i, fw, tt.family_word_templ, open_details)
        html += family_word
        # size_dict["dpd_family_word"] += len(family_word)

    if i.needs_compound_family_button or i.needs_compound_families_button:
        family_compound = render_family_compound_templ(
            pth, i, fc, rd['cf_set'], tt.family_compound_templ, open_details)
        html += family_compound
        # size_dict["dpd_family_compound"] += len(family_compound)

    if i.needs_idioms_button:
        family_idiom = render_family_idioms_templ(
            pth, i, fi, rd['idioms_set'], tt.family_idiom_templ, open_details)
        html += family_idiom
        # size_dict["dpd_family_idiom"] += len(family_idiom)

    if i.needs_set_button or i.needs_sets_button:
        family_sets = render_family_set_templ(pth, i, fs, tt.family_set_templ, open_details)
        html += family_sets
        # size_dict["dpd_family_sets"] += len(family_sets)

    if i.needs_frequency_button:
        frequency = render_frequency_templ(pth, i, tt.frequency_templ, open_details)
        html += frequency
        # size_dict["dpd_frequency"] += len(frequency)

    # if dps_data and sbs:
    #     sbs_example = render_sbs_example_templ(pth, i, sbs, rd['make_link'], tt.sbs_example_templ)
    #     html += sbs_example
    #     size_dict["sbs_example"] += len(sbs_example)

    if not dps_data:
        feedback = render_feedback_templ(pth, i, tt.feedback_templ, open_details)
        html += feedback
        # size_dict["dpd_feedback"] += len(feedback)

    html += "</div>"

    # FIXME improve specifying Simsapa and DPD version in feedback link
    # Example feedback form link in DPD:
    # https://docs.google.com/forms/d/e/1FAIpQLSf9boBe7k5tCwq7LdWgBHHGIPVc4ROO5yjVDo1X5LDAxkmGWQ/viewform?usp=pp_url&entry.438735500=${i.pali_link}&entry.1433863141=GoldenDict+${today}
    html = html.replace("&entry.1433863141=GoldenDict+", "&entry.1433863141=Simsapa+")

    synonyms: List[str] = i.inflections_list
    synonyms = add_niggahitas(synonyms)
    for synonym in synonyms:
        if synonym in sandhi_contractions:
            contractions = sandhi_contractions[synonym]["contractions"]
            for contraction in contractions:
                if "'" in contraction:
                    synonyms.append(contraction)
    if not dps_data:
        synonyms += i.inflections_sinhala_list
        synonyms += i.inflections_devanagari_list
        synonyms += i.inflections_thai_list
    synonyms += i.family_set_list
    synonyms += [str(i.id)]


    if extended_synonyms:
        # Split i.lemma_clean only if it contains a space
        if ' ' in i.lemma_clean:
            words = i.lemma_clean.split(' ')
            synonyms.extend(words)

    # size_dict["dpd_synonyms"] += len(str(synonyms))

    res = RenderResult(
        word = i.lemma_1,
        definition_html = html,
        definition_plain = "",
        synonyms = synonyms,
    )

    return res

def render_pali_root_dpd_simsapa_html(r: DpdRoots,
                                      render_data: DpdHeadwordsRenderData,
                                      open_details: List[DetailsTab] = []) -> RenderResult:
    # NOTE: Compare with
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
