from datetime import datetime
from typing import Any

from simsapa import SIMSAPA_LOG_PATH
from simsapa.app.helpers import create_app_dirs

create_app_dirs()

def info(msg: Any, start_new = False):
    _write_log(msg, "INFO", start_new)

def warn(msg: Any, start_new = False):
    _write_log(msg, "WARN", start_new)

def error(msg: Any, start_new = False):
    _write_log(msg, "ERROR", start_new)

def _write_log(msg: Any, level: str = "INFO", start_new: bool = False):
    msg = str(msg).strip()
    t = datetime.now()
    logline = f"[{t}] {level}: {msg}\n"
    print(logline.strip())

    if start_new:
        mode = 'w'
    else:
        mode = 'a'

    with open(SIMSAPA_LOG_PATH, mode, encoding='utf-8') as f:
        f.write(logline)
