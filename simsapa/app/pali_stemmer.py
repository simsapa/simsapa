def pali_stem(word_orig: str) -> str:
    # FIXME Improve Pāli stemming.

    # dukkhasmiṃ -> dukkh
    stem = word_orig[:-5]

    return stem
