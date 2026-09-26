import argparse
import os

import polars as pl

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--message", required=True)
    parser.add_argument("--modules", required=True)
    args = parser.parse_args()

    print(pl.DataFrame({"a": [1, 2, 3], "b": ["a", "b", "c"]}))
    print(f"Message: {args.message}")
    print(f"Modules: {args.modules}")
    print(f"Namespaced environment: {os.environ['_XYLEM_MESSAGE']}")
