"""Test Search: Combined
"""

from simsapa.app.db_session import get_db_engine_connection_session
from simsapa.app.search.helpers import combined_search
from simsapa.app.search.tantivy_index import TantivySearchIndexes
from simsapa.app.types import SearchMode, SearchParams
from simsapa.layouts.gui_queries import GuiSearchQueries

# The headwords are sorted for consistent test results.
QUERY_TEXT_TEST_CASES = {
    "20400/dpd": {"words": ["kammika 1.dpd.20400/dpd",]},
    "20400": {"words": ["kammika 1.dpd.20400/dpd",]},
    "assa": {"words": ["assa 1.1.dpd.9912/dpd", "assa 1.2.dpd.9913/dpd", "assa 2.1.dpd.9914/dpd", "assa 3.1.dpd.9915/dpd", "assa 4.1.dpd.9916/dpd", "assa 4.2.dpd.9917/dpd", "assa 5.1.dpd.9918/dpd", "assa.appdata.assa/cpd", "assa.appdata.assa/dhammika", "assa.appdata.assa/ncped", "assa.appdata.assa/pts", "assati 2.1.dpd.9940/dpd", "assapota.dpd.9975/dpd", "assaratha.dpd.10001/dpd", "assā 1.1.dpd.10032/dpd", "assā 1.2.dpd.10033/dpd", "assā 2.1.dpd.10034/dpd", "assā.appdata.assā/cpd", "ima 1.1.dpd.13779/dpd", "bhāgī assa.dpd.49651/dpd", "sīghassa.dpd.73960/dpd", "Assa Sutta.appdata.assa-sutta/dppn",]},
    "Idha": {"words": ["idha 1.dpd.13686/dpd", "idha 2.dpd.13687/dpd", "idha no.dpd.13683/dpd", "idha pana.dpd.13684/dpd", "idha 3.dpd.13688/dpd", "idha vā huraṁ vā.dpd.13685/dpd", "idha.appdata.idha/cpd", "idha.appdata.idha/ncped", "idha.appdata.idha/pts", "idhajjhagamā.dpd.13689/dpd", "idhaloka.dpd.13693/dpd", "idhalokika.dpd.71961/dpd", "idhassa.dpd.13698/dpd", "idhāgata.dpd.13700/dpd", "idhāgamana.dpd.13701/dpd", "īdha.appdata.īdha/cpd", "nayidha.dpd.35834/dpd", "yamidha.dpd.53794/dpd", "yedha.dpd.54160/dpd", "samaṇīdha.dpd.59254/dpd",]},
    "kammapatta": {"words": ["upārambha.appdata.upārambha/cpd", "kammayutta.appdata.kammayutta/cpd", "kammāraha.appdata.kammāraha/cpd", "kammāraha.dpd.20388/dpd",]},
    "kammapattā": {"words": ["apatta 1.1.dpd.6157/dpd", "apatta 2.1.dpd.73079/dpd", "upārambha.appdata.upārambha/cpd", "kamma 1.dpd.20233/dpd", "kamma 2.dpd.20234/dpd", "kamma 3.dpd.20235/dpd", "kamma 4.dpd.20236/dpd", "kamma 5.dpd.20237/dpd", "kamma 6.dpd.20238/dpd", "kamma 7.dpd.20239/dpd", "kamma 8.dpd.20240/dpd", "kammayutta.appdata.kammayutta/cpd", "kammāraha.appdata.kammāraha/cpd", "kammāraha.dpd.20388/dpd", "kammī.dpd.20403/dpd", "patta 1.1.dpd.41845/dpd", "patta 2.1.dpd.41846/dpd", "patta 2.2.dpd.41847/dpd", "patta 2.3.dpd.41848/dpd", "patta 2.4.dpd.41849/dpd", "patta 2.5.dpd.41850/dpd", "patta 3.1.dpd.41851/dpd", "patta 3.2.dpd.41852/dpd", "patta 4.1.dpd.77737/dpd",]},
    "kamma": {"words": ["ahosi-kamma.appdata.ahosi-kamma/nyanat", "ācinnaka-kamma.appdata.ācinnaka-kamma/nyanat", "upaghātaka-kamma.appdata.upaghātaka-kamma/nyanat", "upacchedaka-kamma.appdata.upacchedaka-kamma/nyanat", "upatthambhaka-kamma.appdata.upatthambhaka-kamma/nyanat", "upapīlaka-kamma.appdata.upapīlaka-kamma/nyanat", "katattā-kamma.appdata.katattā-kamma/nyanat", "kamma 1.dpd.20233/dpd", "kamma 2.dpd.20234/dpd", "kamma 3.dpd.20235/dpd", "kamma 4.dpd.20236/dpd", "kamma 5.dpd.20237/dpd", "kamma 6.dpd.20238/dpd", "kamma 7.dpd.20239/dpd", "kamma 8.dpd.20240/dpd", "kamma-paccaya.appdata.kamma-paccaya/nyanat", "kamma-bhava.appdata.kamma-bhava/nyanat", "kamma-vatta.appdata.kamma-vatta/nyanat", "kamma.appdata.kamma/cpd", "kamma.appdata.kamma/ncped", "kamma.appdata.kamma/nyanat", "kāya-kamma.appdata.kāya-kamma/nyanat", "garuka-kamma.appdata.garuka-kamma/nyanat", "janaka-kamma.appdata.janaka-kamma/nyanat", "bahula-kamma.appdata.bahula-kamma/nyanat", "mano-kamma.appdata.mano-kamma/nyanat", "maranāsanna-kamma.appdata.maranāsanna-kamma/nyanat", "vacī-kamma.appdata.vacī-kamma/nyanat"]},
    "kammikassa": {"words": ["asubha.appdata.asubha/cpd", "ādi.appdata.ādi/pts", "ādikammika.dpd.11542/dpd", "ādikara.appdata.ādikara/cpd", "āya.appdata.āya/pts", "kammaṭṭhānakammikā.appdata.kammaṭṭhānakammikā/cpd", "kammantika.appdata.kammantika/cpd", "kammika 1.dpd.20400/dpd", "kammika 2.dpd.20401/dpd", "kammika.appdata.kammika/cpd", "kammika.appdata.kammika/ncped", "kammika.appdata.kammika/pts", "kammiya.appdata.kammiya/cpd", "kammiya.dpd.20402/dpd", "kammī.dpd.20403/dpd", "kāra 2.dpd.21289/dpd", "kārī 2.dpd.21372/dpd", "dārukammika.dpd.32337/dpd", "nava.appdata.nava/pts", "vanakammika.dpd.66182/dpd",]},
    "kammī": {"words": ["apapakammi.appdata.apapakammi/cpd", "apapakammin.appdata.apapakammin/cpd", "asādhu.appdata.asādhu/cpd", "ādikammi.appdata.ādikammi/cpd", "ādikammin.appdata.ādikammin/cpd", "āvāsa.appdata.āvāsa/cpd", "kammika 2.dpd.20401/dpd", "kammiya.dpd.20402/dpd", "kammī.dpd.20403/dpd", "kāra 2.dpd.21289/dpd", "kāraka 1.dpd.21293/dpd", "kārī 2.dpd.21372/dpd", "pāpakammī.dpd.45589/dpd",]},
    "ka": {"words": ["√kā.appdata.√kā/whitney", "√kā.dpd.√kā/dpd", "ka 1.1.dpd.19158/dpd", "ka 1.2.dpd.19159/dpd", "ka cana.appdata.ka-cana/ncped", "ka cana.dpd.19156/dpd", "ka ci.appdata.ka-ci/ncped", "ka ci.dpd.19157/dpd", "ka 2.1.dpd.19160/dpd", "ka 3.1.dpd.19161/dpd", "ka 4.1.dpd.19162/dpd", "ka.appdata.ka/cpd", "ka.appdata.ka/ncped", "ka.appdata.ka/pts", "ka.appdata.ka/mw", "kā 1.1.dpd.20877/dpd", "kā 2.1.dpd.20878/dpd", "kā.appdata.kā/cpd", "kā.appdata.kā/pts", "kā.appdata.kā/mw", "Ka.appdata.ka/wn",]},
    "Kīdisī": {"words": ["edisa.appdata.edisa/cpd", "kiṁparama.dpd.21995/dpd", "kīdisa.dpd.22032/dpd",]},
    "natavedisaṁ": {"words": ["edisa.dpd.17958/dpd", "tava 1.dpd.30200/dpd", "tava 2.dpd.30201/dpd", "tvaṁ 1.dpd.31383/dpd", "na 1.dpd.35345/dpd", "na 2.dpd.35346/dpd", "na 3.dpd.35347/dpd", "na 4.dpd.35348/dpd",]},
    "passasāmī'ti": {"words": ["iti.dpd.13466/dpd", "passasati.dpd.44770/dpd",]},
    "passasāmī’ti": {"words": ["iti.dpd.13466/dpd", "passasati.dpd.44770/dpd",]},
    "samadhi": {"words": ["appanā-samādhi.appdata.appanā-samādhi/nyanat", "upacāra-samādhi.appdata.upacāra-samādhi/nyanat", "thiti-bhāgiya-sīla, -samādhi, -paññā.appdata.thiti-bhāgiya-sīla-samādhi-paññā/nyanat", "nibbedha-bhāgiya-sīla (-samādhi, -paññā).appdata.nibbedha-bhāgiya-sīla-samādhi-paññā-/nyanat", "parikamma-samādhi.appdata.parikamma-samādhi/nyanat", "samadhī.appdata.samadhī/mw", "samādhi 1.dpd.59623/dpd", "samādhi 2.dpd.59624/dpd", "samādhi-parikkhāra.appdata.samādhi-parikkhāra/nyanat", "samādhi-vipphārā iddhi.appdata.samādhi-vipphārā-iddhi/nyanat", "samādhi-samāpatti-kusalatā, -thiti-kusalatā, -utthānakusalatā.appdata.samādhi-samāpatti-kusalatā-thiti-kusalatā-utthānakusalatā/nyanat", "samādhi-sambojjhanga.appdata.samādhi-sambojjhanga/nyanat", "samādhi.appdata.samādhi/nyanat", "samādhi.appdata.samādhi/pts", "samādhi.appdata.samādhi/mw", "samādhippamukha.dpd.59647/dpd", "sīla-samādhi-paññā.appdata.sīla-samādhi-paññā/nyanat", "Samādhi Samyutta.appdata.samādhi-samyutta/dppn", "Samādhi Sutta.appdata.samādhi-sutta/dppn", "Samādhi Vagga.appdata.samādhi-vagga/dppn",]},
    "samādhi": {"words": ["appanā-samādhi.appdata.appanā-samādhi/nyanat", "upacāra-samādhi.appdata.upacāra-samādhi/nyanat", "thiti-bhāgiya-sīla, -samādhi, -paññā.appdata.thiti-bhāgiya-sīla-samādhi-paññā/nyanat", "nibbedha-bhāgiya-sīla (-samādhi, -paññā).appdata.nibbedha-bhāgiya-sīla-samādhi-paññā-/nyanat", "parikamma-samādhi.appdata.parikamma-samādhi/nyanat", "samadhī.appdata.samadhī/mw", "samādhi 1.dpd.59623/dpd", "samādhi 2.dpd.59624/dpd", "samādhi-parikkhāra.appdata.samādhi-parikkhāra/nyanat", "samādhi-vipphārā iddhi.appdata.samādhi-vipphārā-iddhi/nyanat", "samādhi-samāpatti-kusalatā, -thiti-kusalatā, -utthānakusalatā.appdata.samādhi-samāpatti-kusalatā-thiti-kusalatā-utthānakusalatā/nyanat", "samādhi-sambojjhanga.appdata.samādhi-sambojjhanga/nyanat", "samādhi.appdata.samādhi/nyanat", "samādhi.appdata.samādhi/pts", "samādhi.appdata.samādhi/mw", "samādhippamukha.dpd.59647/dpd", "sīla-samādhi-paññā.appdata.sīla-samādhi-paññā/nyanat", "Samādhi Samyutta.appdata.samādhi-samyutta/dppn", "Samādhi Sutta.appdata.samādhi-sutta/dppn", "Samādhi Vagga.appdata.samādhi-vagga/dppn",]},
    "ṭhitomhī'ti": {"words": ["atthi 1.1.dpd.2736/dpd", "amha 2.1.dpd.8691/dpd", "amhā.dpd.8693/dpd", "amhi.dpd.8702/dpd", "āsi 2.1.dpd.12879/dpd", "iti.dpd.13466/dpd", "ima 1.1.dpd.13779/dpd", "ṭhita 1.dpd.29087/dpd", "ṭhita 2.dpd.29088/dpd", "ṭhita 3.dpd.29089/dpd", "ṭhita 4.dpd.29090/dpd", "ṭhita 6.dpd.29092/dpd", "ṭhita 7.dpd.29093/dpd",]},
    "ṭhitomhī’ti": {"words": ["atthi 1.1.dpd.2736/dpd", "amha 2.1.dpd.8691/dpd", "amhā.dpd.8693/dpd", "amhi.dpd.8702/dpd", "āsi 2.1.dpd.12879/dpd", "iti.dpd.13466/dpd", "ima 1.1.dpd.13779/dpd", "ṭhita 1.dpd.29087/dpd", "ṭhita 2.dpd.29088/dpd", "ṭhita 3.dpd.29089/dpd", "ṭhita 4.dpd.29090/dpd", "ṭhita 6.dpd.29092/dpd", "ṭhita 7.dpd.29093/dpd",]},
    "upacārasamādhi": {"words": ["asamādhisaṁvattanika.dpd.9529/dpd", "upacārasamādhi.appdata.upacārasamādhi/cpd", "upacārasamādhi.dpd.15454/dpd", "okāsādhigama.appdata.okāsādhigama/cpd", "otarati.appdata.otarati/cpd", "samādhisaṁvattanika.dpd.59672/dpd",]},
    "upacara samadhi": {"words": ["appanā-samādhi.appdata.appanā-samādhi/nyanat", "upacara.appdata.upacara/mw", "upacāra 1.dpd.15448/dpd", "upacāra 2.dpd.15449/dpd", "upacāra 3.dpd.15450/dpd", "upacāra 4.dpd.15451/dpd", "upacāra 5.dpd.15452/dpd", "upacāra 6.dpd.73744/dpd", "upacāra-samādhi.appdata.upacāra-samādhi/nyanat", "upacāra.appdata.upacāra/ncped", "upacāra.appdata.upacāra/nyanat", "upacāra.appdata.upacāra/pts", "upacārasamādhi.dpd.15454/dpd", "upācāra.appdata.upācāra/mw", "gāma’ūpacāra.appdata.gāma’ūpacāra/ncped", "neighbourhood-concentration.appdata.neighbourhood-concentration/nyanat", "parikamma-samādhi.appdata.parikamma-samādhi/nyanat", "samadhī.appdata.samadhī/mw", "samādhi.appdata.samādhi/pts", "Upacara.appdata.upacara/cpd",]},
    "upacāra samādhi": {"words": ["appanā-samādhi.appdata.appanā-samādhi/nyanat", "upacara.appdata.upacara/mw", "upacāra 1.dpd.15448/dpd", "upacāra 2.dpd.15449/dpd", "upacāra 3.dpd.15450/dpd", "upacāra 4.dpd.15451/dpd", "upacāra 5.dpd.15452/dpd", "upacāra 6.dpd.73744/dpd", "upacāra-samādhi.appdata.upacāra-samādhi/nyanat", "upacāra.appdata.upacāra/ncped", "upacāra.appdata.upacāra/nyanat", "upacāra.appdata.upacāra/pts", "upacārasamādhi.dpd.15454/dpd", "upācāra.appdata.upācāra/mw", "gāma’ūpacāra.appdata.gāma’ūpacāra/ncped", "neighbourhood-concentration.appdata.neighbourhood-concentration/nyanat", "parikamma-samādhi.appdata.parikamma-samādhi/nyanat", "samadhī.appdata.samadhī/mw", "samādhi.appdata.samādhi/pts", "Upacara.appdata.upacara/cpd",]},
    "upacara": {"words": ["ārāmūpacāra.dpd.12496/dpd", "upacara.appdata.upacara/mw", "upacarati 1.dpd.15444/dpd", "upacarati 2.dpd.15445/dpd", "upacari.dpd.15446/dpd", "upacāra 1.dpd.15448/dpd", "upacāra 2.dpd.15449/dpd", "upacāra 3.dpd.15450/dpd", "upacāra 4.dpd.15451/dpd", "upacāra 5.dpd.15452/dpd", "upacāra 6.dpd.73744/dpd", "upacāra-samādhi.appdata.upacāra-samādhi/nyanat", "upacāra.appdata.upacāra/cpd", "upacāra.appdata.upacāra/ncped", "upacāra.appdata.upacāra/nyanat", "upacāra.appdata.upacāra/pts", "upacāra.appdata.upacāra/mw", "upacāraṭṭhāna.dpd.77854/dpd", "upācāra.appdata.upācāra/mw", "gāma’ūpacāra.appdata.gāma’ūpacāra/ncped", "mātugāmūpacāra.dpd.52222/dpd", "samīpūpacāra.dpd.75008/dpd", "Upacara.appdata.upacara/cpd",]},
    "upacāra": {"words": ["ārāmūpacāra.dpd.12496/dpd", "upacara.appdata.upacara/mw", "upacāra 1.dpd.15448/dpd", "upacāra 2.dpd.15449/dpd", "upacāra 3.dpd.15450/dpd", "upacāra 4.dpd.15451/dpd", "upacāra 5.dpd.15452/dpd", "upacāra 6.dpd.73744/dpd", "upacāra-samādhi.appdata.upacāra-samādhi/nyanat", "upacāra.appdata.upacāra/cpd", "upacāra.appdata.upacāra/ncped", "upacāra.appdata.upacāra/nyanat", "upacāra.appdata.upacāra/pts", "upacāra.appdata.upacāra/mw", "upacāraṭṭhāna.dpd.77854/dpd", "upācāra.appdata.upācāra/mw", "gāma’ūpacāra.appdata.gāma’ūpacāra/ncped", "mātugāmūpacāra.dpd.52222/dpd", "samīpūpacāra.dpd.75008/dpd", "Upacara.appdata.upacara/cpd",]},
    "vacchagotta": {"words": ["vacchagotta 1.dpd.65666/dpd", "vacchagotta 2.dpd.76285/dpd", "vacchaputta.dpd.65681/dpd", "AggiVacchagottasutta.appdata.aggivacchagottasutta/cpd", "Aññanā Sutta.appdata.aññanā-sutta/dppn", "Anabhisamaya Sutta.appdata.anabhisamaya-sutta/dppn", "Apaccakkhakamma Suttā.appdata.apaccakkhakamma-suttā/dppn", "Appativedhā Sutta.appdata.appativedhā-sutta/dppn", "Asamapekkhanā Sutta.appdata.asamapekkhanā-sutta/dppn", "Asallakkhanā Sutta.appdata.asallakkhanā-sutta/dppn", "Ekapundarīka.appdata.ekapundarīka/dppn", "Kutūhalasālā Sutta.appdata.kutūhalasālā-sutta/dppn", "Mahāvacchagotta Sutta.appdata.mahāvacchagotta-sutta/dppn", "Sudhaja.appdata.sudhaja/dppn", "Tevijja-Vacchagotta Sutta.appdata.tevijja-vacchagotta-sutta/dppn", "Vaccha or Bandha Sutta.appdata.vaccha-or-bandha-sutta/dppn", "Vacchagotta Sutta.appdata.vacchagotta-sutta/dppn", "Vacchagotta.appdata.vacchagotta/dppn", "Vīthisammajjaka Thera.appdata.vīthisammajjaka-thera/dppn", "Venāga Sutta.appdata.venāga-sutta/dppn",]},
}

def test_combined_search():
    _, _, db_session = get_db_engine_connection_session()

    search_indexes = TantivySearchIndexes(db_session)

    api_url = 'http://localhost:4848'

    queries = GuiSearchQueries(db_session,
                               search_indexes,
                               None,
                               api_url)

    for query_text, v in QUERY_TEXT_TEST_CASES.items():
        params = SearchParams(
            mode = SearchMode.Combined,
            page_len = 20,
            lang = 'en',
            lang_include = True,
            source = None,
            source_include = True,
            enable_regex = False,
            fuzzy_distance = 0,
        )

        api_results = combined_search(
            queries = queries,
            query_text = query_text,
            params = params,
            page_num = 0,
            do_pali_sort = True,
        )

        headwords = [f"{i['title']}.{i['schema_name']}.{i['uid']}" for i in api_results['results']]

        assert "\n".join(headwords) == "\n".join(v["words"])
