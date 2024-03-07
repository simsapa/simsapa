"""Finds all words in examples and commentary that contain an apostrophe
denoting sandhi or contraction, eg. ajj'uposatho, tañ'ca"""

from typing import Dict, List, Set, TypedDict

# from rich import print
# from tools.tic_toc import tic, toc

from sqlalchemy.orm.session import Session

# from db.get_db_session import get_db_session
# from db.models import DpdHeadwords
from simsapa.dpd_db.tools.pali_alphabet import pali_alphabet
# from simsapa.dpd_db.tools.paths import ProjectPaths
# from simsapa.dpd_db.tools.configger import config_test

from sqlalchemy.orm import joinedload

from simsapa.app.db import dpd_models as Dpd

exceptions = [
    "maññeti",
    "āyataggaṃ",
    "nayanti",
    "āṇāti",
    "gacchanti",
    "jīvanti",
    "sayissanti",
    "gāmeti"
]

def main():
    pass

class SandhiContrItem(TypedDict):
    contractions: Set[str]
    ids: List[str]


SandhiContractions = Dict[str, SandhiContrItem]


def make_sandhi_contraction_dict(db_session: Session) -> SandhiContractions:
    """Return a list of all sandhi words in db that are split with '."""

    db = db_session.query(Dpd.DpdHeadwords).options(joinedload(Dpd.DpdHeadwords.sbs)).all()
    sandhi_contraction: SandhiContractions = dict()
    word_dict: Dict[int, Set[str]] = dict()

    def replace_split(string: str) -> List[str]:
        string = string.replace("<b>", "")
        string = string.replace("</b>", "")
        string = string.replace("<i>", "")
        string = string.replace("</i>", "")

        string = string.replace(".", " ")
        string = string.replace(",", " ")
        string = string.replace(";", " ")
        string = string.replace("!", " ")
        string = string.replace("?", " ")
        string = string.replace("/", " ")
        string = string.replace("-", " ")
        string = string.replace("{", " ")
        string = string.replace("}", " ")
        string = string.replace("(", " ")
        string = string.replace(")", " ")
        string = string.replace("[", " ")
        string = string.replace("]", " ")
        string = string.replace(":", " ")
        string = string.replace("\n", " ")
        string = string.replace("\r", " ")
        list = string.split(" ")
        return list

    for i in db:
        word_dict[i.id] = set()

        if i.example_1 is not None and "'" in i.example_1:
            word_list = replace_split(i.example_1)
            for word in word_list:
                if "'" in word:
                    word_dict[i.id].update([word])

        if i.example_2 is not None and "'" in i.example_2:
            word_list = replace_split(i.example_2)
            for word in word_list:
                if "'" in word:
                    word_dict[i.id].update([word])

        if i.commentary is not None and "'" in i.commentary:
            word_list = replace_split(i.commentary)
            for word in word_list:
                if "'" in word:
                    word_dict[i.id].update([word])

        # if config_test("gui", "include_sbs_examples", "yes"):
        if True:

            if i.sbs and i.sbs.sbs_example_1 is not None and "'" in i.sbs.sbs_example_1:
                word_list = replace_split(i.sbs.sbs_example_1)
                for word in word_list:
                    if "'" in word:
                        word_dict[i.id].update([word])

            if i.sbs and i.sbs.sbs_example_2 is not None and "'" in i.sbs.sbs_example_2:
                word_list = replace_split(i.sbs.sbs_example_2)
                for word in word_list:
                    if "'" in word:
                        word_dict[i.id].update([word])

            if i.sbs and i.sbs.sbs_example_3 is not None and "'" in i.sbs.sbs_example_3:
                word_list = replace_split(i.sbs.sbs_example_3)
                for word in word_list:
                    if "'" in word:
                        word_dict[i.id].update([word])

            if i.sbs and i.sbs.sbs_example_4 is not None and "'" in i.sbs.sbs_example_4:
                word_list = replace_split(i.sbs.sbs_example_4)
                for word in word_list:
                    if "'" in word:
                        word_dict[i.id].update([word])

    for id, words in word_dict.items():
        for word in words:
            word_clean = word.replace("'", "")

            if word not in exceptions:

                if word_clean not in sandhi_contraction:
                    sandhi_contraction[word_clean] = SandhiContrItem(
                        contractions = {word},
                        ids = [str(id)],
                    )

                else:
                    if word not in sandhi_contraction[word_clean]["contractions"]:
                        sandhi_contraction[word_clean]["contractions"].add(word)
                        sandhi_contraction[word_clean]["ids"] += [str(id)]
                    else:
                        sandhi_contraction[word_clean]["ids"] += [str(id)]
    
    # go back thru the db and find words without ' but in sandhi_contraction            
    for i in db:
        word_list = replace_split(i.example_1)
        word_list += replace_split(i.example_2)
        word_list += replace_split(i.commentary)
        for word in word_list:
            if word in sandhi_contraction:
                sandhi_contraction[word]["contractions"].add(word)
                sandhi_contraction[word]["ids"] += [str(i.id)]

    # print out an wrong characters
    error_list = []
    for key in sandhi_contraction:
        for char in key:
            if char not in pali_alphabet:
                error_list += char
                print(key)
    if error_list != []:
        print("[red]SANDHI ERRORS IN EG1,2,COMM:", end=" ")
        print([x for x in error_list], end=" ")

    # print(sandhi_contraction)
    return sandhi_contraction


if __name__ == "__main__":
    # tic()
    main()
    # toc()
