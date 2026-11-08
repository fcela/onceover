"""`python -m onceover` runs the console entry point."""

import sys

from onceover._cli import main

if __name__ == "__main__":
    sys.exit(main())
