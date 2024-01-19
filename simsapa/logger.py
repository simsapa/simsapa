import os
import sys
from datetime import datetime
from typing import Any, Optional, Union

from simsapa import IS_LINUX, IS_MAC, SIMSAPA_LOG_PATH, INIT_START_TIME
from simsapa.time_log import TimeLog

class TerminalStub():
    # A stub class.
    # Using blessed on Windows errors out.

    bold_underline = ""
    normal = ""

    def __init__(self):
        pass

    def bold(self, text: str) -> str:
        return text

    def bold_yellow(self, text: str) -> str:
        return text

    def bold_red(self, text: str) -> str:
        return text

    def bold_blue(self, text: str) -> str:
        return text

if IS_LINUX:
    from blessed import Terminal
    term: Union[Terminal, TerminalStub] = Terminal()
else:
    term = TerminalStub()

s = os.getenv('ENABLE_TIME_LOG')
if s is not None and s.lower() == 'true':
    ENABLE_TIME_LOG = True
else:
    ENABLE_TIME_LOG = False

time_log: Optional[TimeLog] = None

if ENABLE_TIME_LOG:
    time_log = TimeLog()
    time_log.start(t0=INIT_START_TIME, start_new=True)

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
    _write_log(msg, term.bold("INFO"), start_new)

def warn(msg: Any, start_new = False):
    _write_log(msg, term.bold_yellow("WARN"), start_new)

def error(msg: Any, start_new = False):
    _write_log(msg, term.bold_red("ERROR"), start_new)

def profile(msg: Any, start_new = False):
    if time_log is not None:
        time_log.log(msg)
    _write_log(f"{msg}: {datetime.now() - INIT_START_TIME}", term.bold_blue("PROFILE"), start_new)

def _write_log(msg: Any, level: str = "INFO", start_new: bool = False):
    if DISABLE_LOG:
        return

    msg = str(msg).strip()
    t = datetime.now()
    logline = f"{term.bold_underline}[{t}]{term.normal} {level}: {msg}\n"

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
