#!/usr/bin/env python3

from multiprocessing import freeze_support
from simsapa import runner, IS_WINDOWS

if __name__ == "__main__":
    if IS_WINDOWS:
        freeze_support()
    runner.main()
