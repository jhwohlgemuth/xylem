from argparse import ArgumentParser
from pathlib import Path


def main() -> None:
    parser = ArgumentParser()
    parser.add_argument("--input", required=True)
    parser.add_argument("--message", required=True)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(f"mamba: {arguments.input}: {arguments.message}\n", encoding="utf-8")
    print(arguments.output.read_text(encoding="utf-8"), end="")


if __name__ == "__main__":
    main()
