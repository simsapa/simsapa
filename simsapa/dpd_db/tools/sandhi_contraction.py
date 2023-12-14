"""Finds all words in examples and commentary that contain an apostrophe
denoting sandhi or contraction, eg. ajj'uposatho, tañ'ca"""

from typing import Dict, List, Set, TypedDict

from simsapa.app.db import dpd_models as Dpd
from simsapa.dpd_db.tools.pali_alphabet import pali_alphabet

def main():
    pass

class SandhiContrItem(TypedDict):
    contractions: Set[str]
    ids: List[str]


SandhiContractions = Dict[str, SandhiContrItem]


def make_sandhi_contraction_dict(pali_words: List[Dpd.PaliWord]) -> SandhiContractions:
    """Return a list of all sandhi words in db that are split with '."""

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
        string = string.replace(":", " ")
        string = string.replace("\n", " ")
        list = string.split(" ")
        return list

    for i in pali_words:
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

    for id, words in word_dict.items():
        for word in words:
            word_clean = word.replace("'", "")

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

    error_list = []
    for key in sandhi_contraction:
        for char in key:
            if char not in pali_alphabet:
                error_list += char

    # print(sandhi_contraction)
    return sandhi_contraction


if __name__ == "__main__":
    main()
