import os
import sys
from pathlib import Path

from simsapa import SIMSAPA_DIR, logger
from simsapa.app.dir_helpers import create_app_dirs

create_app_dirs()

def main():
    s = os.getenv('START_NEW_LOG')
    if s is not None and s.lower() == 'false':
        start_new = False
    else:
        start_new = True

    logger.profile("runner::main()", start_new = start_new)

    p = Path(".").absolute()
    logger.info(f"Current folder: {p}")
    logger.info(f"SIMSAPA_DIR: {SIMSAPA_DIR}")

    if len(sys.argv) == 1:
        from simsapa.gui import start
        start()

    elif len(sys.argv) == 2:
        from simsapa.cli import app, gui
        s = sys.argv[1]

        if s.startswith("ssp://"):
            gui(url=s)

        else:
            app()
    else:
        from simsapa.cli import app
        app()

if __name__ == "__main__":
    main()
