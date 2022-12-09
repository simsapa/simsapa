#!/usr/bin/env python3

import os
from pathlib import Path
from dotenv import load_dotenv
import appdirs

load_dotenv()

s = os.getenv('SIMSAPA_DIR')
if s is not None and s != '':
    SIMSAPA_DIR = Path(s)
else:
    SIMSAPA_DIR = Path(appdirs.user_data_dir('simsapa'))

SIMSAPA_LOG_PATH = SIMSAPA_DIR.joinpath('log.txt')

if SIMSAPA_LOG_PATH.exists():
    print(f"Removing {SIMSAPA_LOG_PATH}")
    SIMSAPA_LOG_PATH.unlink()
