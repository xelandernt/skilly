# Contributing

## Quick start

```shell
# Install dev dependencies + editable extension
just install

# Run linters (prek, ruff)
just lint

# Run tests (Rust + Python + TypeScript)
just test

# Run type checkers (Pyrefly + TypeScript)
just typecheck
```

## Documentation

```shell
just docs-serve    # Preview site at http://localhost:8000
just docs-build    # Strict production build
```

The documentation site is built with [Zensical](https://zensical.org/) from
Markdown files in `docs/`. See [documentation conventions](documentation.md)
for authoring guidelines.

## Quality gates

Before opening a pull request, ensure all gates pass:

```shell
just lint
just test
just typecheck
```

Pull requests also run the Zensical documentation build to catch broken links
and validation errors.


## License

This project is licensed under the MIT License. See [LICENSE](https://github.com/xelandernt/skilly/blob/main/LICENSE) for details.
