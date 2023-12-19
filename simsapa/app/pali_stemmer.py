from typing import List, TypedDict

class EndingRepl(TypedDict):
    endings: List[str]
    repl: str

# // bhūmīnaṁ → bhūmi is OK, but kumārīnaṁ → kumāri should be kumārī.

def pali_stem(word_orig: str) -> str:
    # Based on the Snowball Pāli stemmer.

    # Remove declension endings for masc., neut., fem. nouns.
    # Start with the longer endings to avoid ambiguity.

    declensions: List[EndingRepl] = [
        # masc.-a 5 chars
        {'endings': ['asmiṁ'], 'repl': 'a'},
        # masc.-i 5 chars
        {'endings': ['ismiṁ'], 'repl': 'i'},
        # masc.-u 5 chars
        {'endings': ['usmiṁ'], 'repl': 'u'},
        # masc.-a 4 chars
        {'endings': ['ānaṁ', 'amhā', 'amhi', 'asmā', 'assa'], 'repl': 'a'},
        # fem.-ā 4 chars
        {'endings': ['āyaṁ', 'āyo'], 'repl': 'ā'},
        # masc.-i 4 chars
        {'endings': ['īnaṁ', 'imhā', 'imhi', 'ismā', 'issa'], 'repl': 'i'},
        # fem.-i 4 chars
        {'endings': ['iyaṁ'], 'repl': 'i'},
        # masc.-ī 4 chars
        {'endings': ['inaṁ'], 'repl': 'ī'},
        # masc.-u 4 chars
        {'endings': ['umhā', 'umhi', 'usmā', 'ussa', 'ūnaṁ'], 'repl': 'u'},
        # fem.-u 4 chars
        {'endings': ['uyaṁ'], 'repl': 'u'},
        # masc.-a 3 chars
        {'endings': ['āya', 'ehi', 'ena', 'esu'], 'repl': 'a'},
        # fem.-ā 3 chars
        {'endings': ['āhi', 'āsu'], 'repl': 'ā'},
        # neut.-a 3 chars
        {'endings': ['āni'], 'repl': 'a'},
        # masc.-i 3 chars
        {'endings': ['ayo', 'īhi', 'īsu', 'inā', 'ino'], 'repl': 'i'},
        # neut.-i 3 chars
        {'endings': ['īni', 'ini', 'isu'], 'repl': 'i'},
        # fem.-i 3 chars
        {'endings': ['iyā', 'iyo'], 'repl': 'i'},
        # masc.-u 3 chars
        {'endings': ['ave', 'avo', 'unā', 'uno', 'ūhi', 'ūsu'], 'repl': 'u'},
        # neut.-u 3 chars
        {'endings': ['ūni'], 'repl': 'u'},
        # fem.-u 3 chars
        {'endings': ['usu', 'uyā', 'uyo'], 'repl': 'u'},
        # masc.-a 2 chars
        {'endings': ['aṁ'], 'repl': 'a'},
        # masc.-i 2 chars
        {'endings': ['iṁ'], 'repl': 'i'},
        # masc.-u 2 chars
        {'endings': ['uṁ'], 'repl': 'u'},
        # masc.-a 1 chars
        {'endings': ['o', 'ā', 'e'], 'repl': 'a'},
        # masc.-i 1 chars
        {'endings': ['ī'], 'repl': 'i'},
        # masc.-u 1 chars
        {'endings': ['ū'], 'repl': 'u'},
    ]

    stem = word_orig

    # Not using the replacement wovel.
    for i in declensions:
        endings = i['endings']
        for e in endings:
            if stem.endswith(e):
                stem = stem.replace(e, '')

    return stem
