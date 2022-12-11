#!/usr/bin/env python3

import os
import sys
from pathlib import Path
from typing import List, Tuple
from dotenv import load_dotenv
import sqlite3
import multiprocessing
import psutil

from selenium import webdriver
from selenium.webdriver.chrome.service import Service as ChromeService
from selenium.webdriver.common.by import By
from selenium.webdriver.support.select import Select
import selenium.webdriver.support.ui as ui

from simsapa import logger

# If first and second arguments are given, they are taken as the batch range.
#
# The word list IS SORTED alphabetically by the sqlite db key (i.e. 'aMSa').
# Parallel processing nonetheless scrambles the order of words to some extent.
#
# If the key file (aMSa.txt) for a word already exists, we assume it is from a
# previous export run and skip.

load_dotenv()

SEARCH_URL = "http://localhost:8888/web/webtc2/index.php"

CHROMEDRIVER_PATH = "/snap/bin/chromium.chromedriver"

# These word keys return empty data.
SKIP_WORDS = ['kIdfkza', 'kIdfSa', 'nF~HpraRetra']

s = os.getenv('BOOTSTRAP_ASSETS_DIR')
if s is None or s == "":
    logger.error("Missing env variable: BOOTSTRAP_ASSETS_DIR")
    sys.exit(1)

bootstrap_assets_dir = Path(s)
DB_PATH = bootstrap_assets_dir.joinpath("sanskrit/web/sqlite/mw.sqlite")

WORDS_OUT_DIR = bootstrap_assets_dir.joinpath("sanskrit/words-html/")

if not WORDS_OUT_DIR.exists():
    WORDS_OUT_DIR.mkdir()


def get_word_keys() -> List[str]:
    db_conn = sqlite3.connect(DB_PATH)
    cursor = db_conn.execute("SELECT key FROM mw")

    a = set(map(lambda x: x[0], cursor))

    word_keys = list(a)
    word_keys.sort()

    db_conn.close()

    return word_keys


def export_word(word_key: str) -> Tuple[str, str]:
    key_path = WORDS_OUT_DIR.joinpath(f"{word_key}.txt")
    if key_path.exists() or word_key in SKIP_WORDS:
        logger.info(f"SKIPPING {word_key}")
        return (word_key, "")

    options = webdriver.ChromeOptions()
    options.headless = True
    options.add_experimental_option("excludeSwitches", ["enable-automation"])
    options.add_experimental_option("useAutomationExtension", False)
    service = ChromeService(executable_path=CHROMEDRIVER_PATH)

    driver = webdriver.Chrome(service=service, options=options) # type: ignore

    driver.get(SEARCH_URL)

    driver.find_element(By.ID, 'sword').send_keys(word_key)

    sel = Select(driver.find_element(By.ID, 'transLit'))
    sel.select_by_value('slp1')

    sel = Select(driver.find_element(By.ID, 'filter'))
    sel.select_by_value('roman')

    driver.find_element(By.ID, 'searchbtn').click()

    wait = ui.WebDriverWait(driver, 10) # type: ignore

    wait.until(lambda driver: driver.find_element(By.ID, 'CologneBasic'))

    span = driver.find_element(By.CSS_SELECTOR, '#CologneBasic h1 span.sdata')
    title = span.get_attribute('innerHTML')

    # Remove the h1 title, dictionary definition doesn't need it
    driver.execute_script("""
    var element = document.querySelector("#CologneBasic h1");
    if (element) {
        element.parentNode.removeChild(element);
    }
    """)

    cologne_basic = driver.find_element(By.ID, 'CologneBasic')

    html: str = cologne_basic.get_attribute('innerHTML')

    # fix escaped ampersand
    # servepdf.php?dict=MW&amp;page=76
    html = html.replace('servepdf.php?dict=MW&amp;page=', 'servepdf.php?dict=MW&page=')

    # fix relative urls
    # href="../webtc/servepdf.php?dict=MW&amp;page=18"
    # to
    # href="https://www.sanskrit-lexicon.uni-koeln.de/scans/csl-apidev/servepdf.php?dict=MW&page=470"
    html = html.replace('href="../webtc/servepdf.php', 'href="https://www.sanskrit-lexicon.uni-koeln.de/scans/csl-apidev/servepdf.php')
    # fix missing schema
    # <a href="//www.sanskrit-lexicon.uni-koeln.de/scans/csl-whitroot/disp/index.php?page=45"
    html = html.replace('href="//', 'href="https://')

    word_path = WORDS_OUT_DIR.joinpath(f"{title}.html")

    with open(word_path, 'w') as f:
        f.write(html)

    # Write key_path after the word, as a sign of successful HTML export.
    with open(key_path, 'w') as f:
        f.write('')

    driver.close()

    return (word_key, title)


def process_word(word_key: str) -> Tuple[str, str]:
    for attempt in range(5):
        try:
            r = export_word(word_key)
            return r
        except Exception as e:
            logger.error(f"{word_key}: {e.__class__.__name__}, after {attempt+1} attempt(s). Retrying.")
            # logger.error(f"{e}")
    else:
        logger.error(f"All attempts failed for {word_key}. Exiting.")
        sys.exit(1)


def main():
    if len(sys.argv) == 1:
        word_keys = get_word_keys()
    elif len(sys.argv) == 3:
        a = int(sys.argv[1])
        b = int(sys.argv[2])
        word_keys = get_word_keys()[a:b]
    else:
        print("Either 0 or 2 arguments are allowed.")
        sys.exit(1)

    global total
    total = len(word_keys)

    logger.info(f"Total words: {total}", start_new = True)

    global done_count
    done_count = 0

    def word_done(item: Tuple[str, str]):
        global total
        global done_count
        done_count += 1
        if item[1] != "":
            logger.info(f"{done_count:05d} / {total}: {item[0]} - {item[1]}")

    n = psutil.cpu_count()-4
    if n > 0:
        processes = n
    else:
        processes = 1

    pool = multiprocessing.Pool(processes = processes)

    results = []
    for w in word_keys:
        r = pool.apply_async(process_word, (w,), callback = word_done)
        results.append(r)
    for r in results:
        r.wait()

    logger.info("Done")


if __name__ == "__main__":
    main()
