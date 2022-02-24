#!/usr/bin/env python3

import os
import shutil
import sys
from pathlib import Path
from typing import List
from dotenv import load_dotenv
import sqlite3
import multiprocessing

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
# A batch is not guaranteed to complete without error, sometimes the webdriver
# crashes unexpectedly. Due to the large number of words, this script may need
# to run several times.
#
# If the key file (aMSa.txt) for a word already exists, we assume it is from a
# previous export run and skip.

load_dotenv()

SEARCH_URL = "http://localhost:8888/web/webtc2/index.php"

CHROMEDRIVER_PATH = "/snap/bin/chromium.chromedriver"

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


def export_word(word_key: str):
    key_path = WORDS_OUT_DIR.joinpath(f"{word_key}.txt")
    if key_path.exists():
        logger.info(f"SKIPPING {word_key}")
        return

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

    wait = ui.WebDriverWait(driver, 10)

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

    logger.info(f"{word_key}: {title}")

    with open(word_path, 'w') as f:
        f.write(html)

    # Write key_path after the word, as a sign of successful HTML export.
    with open(key_path, 'w') as f:
        f.write('')

    driver.close()

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

    total = len(word_keys)

    logger.info(f"Total words: {total}", start_new = True)

    pool = multiprocessing.Pool(processes=8)
    outputs_async = pool.map_async(export_word, word_keys)
    outputs_async.get()

    logger.info("Done")


if __name__ == "__main__":
    main()
