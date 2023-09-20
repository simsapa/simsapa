import os
import sys
from datetime import datetime
from typing import Any

from simsapa import IS_MAC, SIMSAPA_LOG_PATH, INIT_START_TIME

s = os.getenv('DISABLE_LOG')
if s is not None and s.lower() == 'true':
    DISABLE_LOG = True
else:
    DISABLE_LOG = False

s = os.getenv('ENABLE_PRINT_LOG')
if s is not None and s.lower() == 'true':
    ENABLE_PRINT_LOG = True
else:
    ENABLE_PRINT_LOG = False

def info(msg: Any, start_new = False):
    _write_log(msg, "INFO", start_new)

def warn(msg: Any, start_new = False):
    _write_log(msg, "WARN", start_new)

def error(msg: Any, start_new = False):
    _write_log(msg, "ERROR", start_new)

def profile(msg: Any, start_new = False):
    _write_log(f"{msg}: {datetime.now() - INIT_START_TIME}", "PROFILE", start_new)

def _write_log(msg: Any, level: str = "INFO", start_new: bool = False):
    if DISABLE_LOG:
        return

    msg = str(msg).strip()
    t = datetime.now()
    logline = f"[{t}] {level}: {msg}\n"

    if ENABLE_PRINT_LOG and not getattr(sys, 'frozen', False):
        if IS_MAC:
            # Avoid MacOS encoding error
            # UnicodeEncodeError: 'ascii' codec can't encode character '\u0101' in position 169: ordinal not in range(128)
            print(logline.encode('utf-8').strip())
        else:
            # ensure utf-8 unicode for print
            s = u"%s" % logline.strip()
            print(s)

    if start_new:
        mode = 'w'
    else:
        mode = 'a'

    with open(SIMSAPA_LOG_PATH, mode, encoding='utf-8') as f:
        f.write(logline)
