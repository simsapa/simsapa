#!/usr/bin/env python3

from multiprocessing import freeze_support
from simsapa import runner

if __name__ == "__main__":
    freeze_support()
    runner.main()
