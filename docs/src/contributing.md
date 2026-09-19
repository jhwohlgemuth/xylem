# Contributing

Clone the repository, install the development tools listed in the `Makefile`,
and use the standard project checks:

```shell
make test
make lint
make book
```

Use `make book-write` to serve the book locally. The generated output is stored
under `docs/book` and is not committed.

The root
[`CONTRIBUTING.md`](https://code.ornl.gov/research-enablement/xylem/-/blob/main/CONTRIBUTING.md)
documents the development environment, cross-platform testing, logging
conventions, release tasks, and Homebrew tap maintenance.

Changes to public configuration or CLI behavior should update the relevant
book page in the same merge request. CI builds the book for documentation
changes, while successful main-branch or version-tag pipelines can publish it
to the Xylem documentation site.
