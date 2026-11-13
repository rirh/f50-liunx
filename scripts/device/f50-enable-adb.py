#!/usr/bin/env python3
"""Prepare stock F50 ADB and root access; normally invoked by make adb."""
from f50_adb.cli import main


if __name__ == '__main__':
    raise SystemExit(main())
