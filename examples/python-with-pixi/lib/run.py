"""Print values supplied by Xylem to a Pixi-managed Python process."""

import argparse
import os


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--message", required=True)
    parser.add_argument("--modules", required=True)
    args = parser.parse_args()

    print(args.message)
    print(f"Workflow modules: {args.modules}")
    print(f"Namespaced environment: {os.environ['_XYLEM_MESSAGE']}")


if __name__ == "__main__":
    main()
