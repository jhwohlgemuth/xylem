from argparse import ArgumentParser
from pathlib import Path


def main() -> None:
    parser = ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    message = arguments.input.read_text(encoding="utf-8").strip()
    arguments.output.write_text(f"pixi: {message}\n", encoding="utf-8")
    print(arguments.output.read_text(encoding="utf-8"), end="")


if __name__ == "__main__":
    main()
