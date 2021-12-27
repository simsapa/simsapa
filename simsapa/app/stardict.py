"""Stardict format related funcions

Stardict format documentation:
https://github.com/huzheng001/stardict-3/blob/master/dict/doc/StarDictFileFormat

Notes about StarDict dictionary format
http://dhyannataraj.github.io/blog/2010/10/04/Notes-about-stardict-dictionry-format/

Support functions:
https://github.com/codito/stargaze/blob/master/stargaze.py
"""

import logging as _logging
from pathlib import Path
import datetime
from typing import List, TypedDict, Optional
import shutil
from zipfile import ZipFile
import struct
import idzip

from simsapa import SIMSAPA_DIR

logger = _logging.getLogger(__name__)

class DictError(Exception):
    """Error in the dictionary."""
    pass

class StarDictIfo(TypedDict):
    """
    Available options:

    ```
    bookname=      // required
    wordcount=     // required
    synwordcount=  // required if ".syn" file exists.
    idxfilesize=   // required
    idxoffsetbits= // New in 3.0.0
    author=
    email=
    website=
    description= // You can use <br> for new line.
    date=
    sametypesequence= // very important.
    dicttype=
    ```

    sametypesequence=m The data should be a utf-8 string ending with '\\0'.
    sametypesequence=h Html


    Example contents of an .ifo file:

    ```
    StarDict's dict ifo file
    version=3.0.0
    bookname=Digital Pāli Dictionary
    wordcount=36893
    synwordcount=1727042
    idxfilesize=747969
    idxoffsetbits=32
    author=Digital Pāli Tools <digitalpalitools@gmail.com>
    website=https://github.com/digitalpalitools
    description=The next generation comprehensive Digital Pāli Dictionary.
    date=2021-10-31T08:56:25Z
    sametypesequence=h
    ```
    """

    version: str
    bookname: str
    wordcount: str
    synwordcount: str
    idxfilesize: str
    idxoffsetbits: str
    author: str
    email: str
    website: str
    description: str
    date: str
    sametypesequence: str
    dicttype: str

def ifo_from_opts(opts: dict[str, str]) -> StarDictIfo:
    ifo = StarDictIfo(
        version = '',
        bookname = '',
        wordcount = '',
        synwordcount = '',
        idxfilesize = '',
        idxoffsetbits = '',
        author = '',
        email = '',
        website = '',
        description = '',
        date = '',
        sametypesequence = '',
        dicttype = '',
    )
    for k in opts.keys():
        if k in ifo.keys():
            ifo[k] = opts[k]

    return ifo

class IdxEntry(TypedDict):
    # a utf-8 string, `\0` stripped
    word: str
    # word data's offset in .dict file
    offset_begin: int
    # word data's total size in .dict file
    data_size: int

class DictEntry(TypedDict):
    # a utf-8 string, `\0` stripped
    word: str
    definition_plain: str
    definition_html: str
    synonyms: List[str]

SynEntries = dict[
    # a utf-8 string, `\0` stripped
    str,
    # indices from the .idx
    List[int],
]

class StarDictPaths(TypedDict):
    zip_path: Path
    unzipped_dir: Path
    icon_path: Optional[Path]
    ifo_path: Optional[Path]
    idx_path: Optional[Path]
    dic_path: Optional[Path]
    syn_path: Optional[Path]

def new_stardict_paths(zip_path: Path):
    unzipped_dir: Path = SIMSAPA_DIR.joinpath("unzipped_stardict")
    return StarDictPaths(
        zip_path = zip_path,
        unzipped_dir = unzipped_dir,
        icon_path = None,
        ifo_path = None,
        idx_path = None,
        dic_path = None,
        syn_path = None,
    )

def parse_stardict_zip(zip_path: Path) -> StarDictPaths:

    stardict_paths = new_stardict_paths(zip_path)
    unzipped_dir = stardict_paths['unzipped_dir']

    # Find the .ifo, .idx, .dic, .syn
    hits = {'*.ifo': [], '*.idx': [], '*.dic*': [], '*.syn': []}
    try:
        # delete and re-create to make sure it's an empty directory
        if unzipped_dir.exists():
            shutil.rmtree(unzipped_dir)
        unzipped_dir.mkdir()

        with ZipFile(zip_path, 'r') as z:
            z.extractall(unzipped_dir)

        # NOTE: The zip file may or may not have a top-level folder. A
        # dictionary may be compressed as '*.dict.dz'.

        for ext in hits.keys():
            a = list(unzipped_dir.glob(f"**/{ext}"))
            if len(a) == 0:
                # .syn is optional
                if ext == '*.syn':
                    hits[ext] = [None]
                else:
                    msg = f"ERROR: Can't find this type of file in the .zip: {ext}"
                    logger.error(msg)
                    raise DictError(msg)
            else:
                hits[ext] = [a[0]]

    except Exception as e:
        logger.error(e)
        raise e

    stardict_paths['ifo_path'] = hits['*.ifo'][0]
    stardict_paths['idx_path'] = hits['*.idx'][0]
    stardict_paths['dic_path'] = hits['*.dic*'][0]
    stardict_paths['syn_path'] = hits['*.syn'][0]

    return stardict_paths

def parse_ifo(paths: StarDictPaths) -> StarDictIfo:
    if paths['ifo_path'] is None:
        msg = f"ifo file is None"
        logger.error(msg)
        raise DictError(msg)

    ifo_path = paths['ifo_path']

    if not ifo_path.exists():
        msg = f"ifo file not found: {ifo_path}"
        logger.error(msg)
        raise DictError(msg)

    magic_header = "StarDict's dict ifo file"
    with open(ifo_path, 'r', encoding='utf-8') as f:
        magic_string = f.readline().rstrip()
        if (magic_string != magic_header):
            msg = "IFO: Incorrect header: {}".format(magic_string)
            logger.error(msg)
            raise DictError(msg)

        a = (l.split("=") for l in map(str.rstrip, f) if l != "")
        opts = {k.strip(): v.strip() for k, v in a}

        return ifo_from_opts(opts)

def stardict_to_dict_entries(paths: StarDictPaths) -> List[DictEntry]:

    idx = parse_idx(paths)
    ifo = parse_ifo(paths)
    syn = parse_syn(paths)
    words = parse_dict(paths, ifo, idx, syn)

    return words

def parse_idx(paths: StarDictPaths) -> List[IdxEntry]:
    """Parse an .idx file."""

    if paths['idx_path'] is None:
        msg = f"idx file is None"
        logger.error(msg)
        raise DictError(msg)

    idx_path = paths['idx_path']

    if not idx_path.exists():
        msg = f"idx file not found: {idx_path}"
        logger.error(msg)
        raise DictError(msg)

    words_index = []

    # idxoffsetbits can be 32bit or 64bit.
    # offset refers to two integers.
    # struct packing uses ">II", two unsigned ints, 2*4 bytes = 32 bits
    offset_size = 8
    with open(idx_path, "rb") as f:
        while True:
            word_str = _read_word(f).rstrip('\0')

            word_pointer = f.read(offset_size)
            if not word_pointer:
                break

            offset_begin, data_size = struct.unpack(">II", word_pointer)
            words_index.append(IdxEntry(
                word = word_str,
                offset_begin = offset_begin,
                data_size = data_size,
            ))

    return words_index

def parse_dict(paths: StarDictPaths,
               ifo: StarDictIfo,
               idx: List[IdxEntry],
               syn: Optional[SynEntries]) -> List[DictEntry]:
    """Parse a .dict file."""

    dict_path = paths['dic_path']

    if dict_path is None:
        msg = f"dict file is None"
        logger.error(msg)
        raise DictError(msg)

    if not dict_path.exists():
        msg = f"dict file not found: {dict_path}"
        logger.error(msg)
        raise DictError(msg)

    words: List[DictEntry] = []

    open_dict = open
    if f"{dict_path}".endswith(".dz"):
        open_dict = idzip.open

    if len(ifo["sametypesequence"]) > 0:
        types = ifo["sametypesequence"]
    else:
        types = "m"

    with open_dict(dict_path, "rb") as f:
        for i in idx:

            dict_word = i['word']
            f.seek(i["offset_begin"])
            data = f.read(i["data_size"])
            data_str: str = data.decode("utf-8").rstrip("\0")

            definition_plain = ""
            definition_html = ""
            # Only accept sametypesequence = m, h
            if types == "m" and data_str[0] == "m":
                definition_plain = data_str[1:]

            if types == "h":
                definition_html = data_str

            synonyms = []
            if syn is not None and dict_word in syn.keys():
                synonyms = list(map(lambda x: idx[x]['word'], syn[dict_word]))
                # FIXME synonyms seem to just repeat the dict word
                # if len(synonyms) > 0:
                #     print(syn[dict_word])
                #     print(synonyms)

            words.append(DictEntry(
                word = dict_word,
                definition_plain = definition_plain,
                definition_html = definition_html,
                synonyms = synonyms,
            ))

    return words

def parse_syn(paths: StarDictPaths) -> Optional[SynEntries]:
    """Parse a .syn file with synonyms.

    The .syn format:

    Each item contains one string and a number.

    synonym_word;  // a utf-8 string terminated by NUL '\\0'.
    original_word_index; // original word's index in .idx file.

    Then other items without separation.

    When you input synonym_word, StarDict will search original_word; The length
    of "synonym_word" should be less than 256. In other words, (strlen(word) <
    256).

    original_word_index is a 32-bits unsigned number in network byte order. Two
    or more items may have the same "synonym_word" with different
    original_word_index.
    """

    if paths['syn_path'] is None:
        # Syn file is optional
        return None

    syn_path = paths['syn_path']

    if not syn_path.exists():
        msg = f"syn file not found: {syn_path}"
        logger.error(msg)
        raise DictError(msg)

    syn_entries: SynEntries = {}

    with open(syn_path, 'rb') as f:
        while True:
            word_str = _read_word(f).rstrip('\0')

            word_pointer = f.read(4)
            if not word_pointer:
                break

            if word_str not in syn_entries.keys():
                syn_entries[word_str] = []

            syn_entries[word_str].extend(struct.unpack(">I", word_pointer))

    return syn_entries

def _read_word(f) -> str:
    r"""Read a unicode NUL `\\0` terminated string from a file like object."""

    word = bytearray()
    c = f.read(1)
    while c and c != b'\0':
        word.extend(c)
        c = f.read(1)
    return word.decode('utf-8')

def write_ifo(ifo: StarDictIfo, paths: StarDictPaths):
    """Writes .ifo"""

    if paths['ifo_path'] is None:
        logger.error("ifo_path is required")
        return

    lines: List[str] = ["StarDict's dict ifo file"]

    required = ['bookname', 'wordcount', 'synwordcount', 'idxfilesize', 'sametypesequence']
    missing = []
    for k in required:
        if k not in ifo.keys() and len(ifo[k]) > 0 and ifo[k] != 'None':
            missing.append(k)

    if len(missing) > 0:
        logger.error(f"Missing required keys: {missing}")
        return

    for k in ifo.keys():
        v = ifo[k]
        if len(v) > 0 and v != 'None':
            lines.append(f"{k}={v}")

    with open(paths['ifo_path'], 'w') as f:
        f.write("\n".join(lines))

class WriteResult(TypedDict):
    idx_size: Optional[int]
    syn_count: Optional[int]

def write_words(words: List[DictEntry], paths: StarDictPaths) -> WriteResult:
    """Writes .idx, .dict.dz, .syn"""

    res = WriteResult(
        idx_size = None,
        syn_count = None
    )

    if paths['idx_path'] is None or paths['dic_path'] is None:
        logger.error("idx_path and dic_path are required")
        return res

    idx: List[IdxEntry] = []

    with idzip.IdzipFile(f"{paths['dic_path']}", "wb") as f:
        offset_begin = 0
        data_size = 0
        for w in words:
            d = bytes(w['definition_html'], 'utf-8')
            f.write(d)

            data_size = len(d)

            idx.append(IdxEntry(
                word = w['word'],
                offset_begin = offset_begin,
                data_size = data_size,
            ))

            offset_begin += data_size

    with open(paths['idx_path'], 'wb') as f:
        for i in idx:
            d = bytes(f"{i['word']}\0", "utf-8")
            f.write(d)
            d = struct.pack(">II", i['offset_begin'], i['data_size'])
            f.write(d)

    res['idx_size'] = paths['idx_path'].stat().st_size

    if paths['syn_path'] is not None:
        res['syn_count'] = 0

        with open(paths['syn_path'], 'wb') as f:
            for n, w in enumerate(words):

                if res['syn_count'] is not None:
                    res['syn_count'] += len(w['synonyms'])

                for s in w['synonyms']:
                    d = bytes(f"{s}\0", "utf-8")
                    f.write(d)
                    d = struct.pack(">I", n)
                    f.write(d)

    return res

def write_stardict_zip(paths: StarDictPaths):
    with ZipFile(paths['zip_path'], 'w') as z:
        a = [paths['ifo_path'],
             paths['idx_path'],
             paths['dic_path'],
             paths['syn_path'],
             paths['icon_path']]
        for p in a:
            if p is not None:
                # NOTE .parent to create a top level folder in .zip
                z.write(p, p.relative_to(paths['unzipped_dir'].parent))

def export_words_as_stardict_zip(words: List[DictEntry],
                                 ifo: StarDictIfo,
                                 zip_path: Path,
                                 icon_path: Optional[Path] = None):

    name = zip_path.name.replace('.zip', '')
    # No spaces in the filename and dict files.
    name = name.replace(' ', '-')

    # NOTE: A toplevel folder is created in the zip file, with the file name of the .zip file.
    # E.g. ncped.zip will contain ncped/ncped.ifo
    unzipped_dir: Path = SIMSAPA_DIR.joinpath("new_stardict").joinpath(name)
    if unzipped_dir.exists():
        shutil.rmtree(unzipped_dir)
    unzipped_dir.mkdir(parents=True)

    zip_icon_path = None

    if icon_path is not None and icon_path.exists():
        ext = icon_path.suffix
        zip_icon_path = unzipped_dir.joinpath(f"{name}{ext}")
        shutil.copy(icon_path, zip_icon_path)

    paths = StarDictPaths(
        zip_path = zip_path,
        unzipped_dir = unzipped_dir,
        icon_path = zip_icon_path,
        ifo_path = unzipped_dir.joinpath(f"{name}.ifo"),
        idx_path = unzipped_dir.joinpath(f"{name}.idx"),
        dic_path = unzipped_dir.joinpath(f"{name}.dict.dz"),
        syn_path = unzipped_dir.joinpath(f"{name}.syn"),
    )

    ifo['version'] = '3.0.0'
    ifo['wordcount'] = f"{len(words)}"
    ifo['sametypesequence'] = 'h'
    ifo['date'] = datetime.datetime.utcnow().replace(microsecond=0).isoformat()

    res = write_words(words, paths)

    ifo['idxoffsetbits'] = "32"
    ifo['idxfilesize'] = f"{res['idx_size']}"
    ifo['synwordcount'] = f"{res['syn_count']}"

    write_ifo(ifo, paths)

    write_stardict_zip(paths)

    if unzipped_dir.exists():
        shutil.rmtree(unzipped_dir)
