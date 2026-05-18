# cargo-typegraph

`cargo-typegraph` generates a Graphviz DOT graph of Rust type dependencies from
rustdoc JSON. It can run as a Cargo subcommand (`cargo typegraph`) or as a
standalone binary.

The graph includes local structs, enums, traits, type aliases. Edges
point from a type to the types referenced by its definition, fields,
aliases, generics, bounds, and implementations. Nodes are colored by
visibility so public, crate-visible, super-visible, restricted, and
private types are easy to distinguish.

## Requirements

- A nightly Rust toolchain when generating rustdoc JSON directly, because
  rustdoc JSON currently uses unstable rustdoc options.
- Graphviz if you want to render the generated DOT output as SVG, PNG, or PDF.

## Installation

From crates.io:

```sh
cargo install cargo-typegraph
```

After installation, Cargo can invoke the binary as a subcommand:

```sh
cargo typegraph --help
```

## Usage

Generate a DOT graph for the current package:

```sh
cargo typegraph -o typegraph.dot
```

Render the graph with Graphviz:

```sh
dot -Tsvg typegraph.dot -o typegraph.svg
```

Generate a graph for a specific package or binary:

```sh
cargo typegraph -p my_crate -o typegraph.dot
cargo typegraph --bin my_binary -o typegraph.dot
```

Read an existing rustdoc JSON file instead of invoking `cargo doc`:

```sh
cargo typegraph --json target/doc/my_crate.json -o typegraph.dot
```

Include external referenced types as graph nodes:

```sh
cargo typegraph --include-external -o typegraph.dot
```

Detect dependency cycles and fail when any are found:

```sh
cargo typegraph --detect-cycles -o typegraph.dot
```

Pass additional arguments through to `cargo doc` after `--`:

```sh
cargo typegraph -o typegraph.dot -- --workspace
```

## Options

```text
--json <PATH>                 Read an existing rustdoc JSON file
-o, --output <PATH>           Write DOT to a file instead of stdout
--include-external            Include referenced external types as graph nodes
--detect-cycles               Print the first dependency cycle to stderr and exit with an error
--no-private                  Do not pass --document-private-items to cargo doc
--manifest-path <PATH>        Cargo manifest path for cargo doc
-p, --package <PACKAGE>       Package to document
--bin <NAME>                  Generate docs for a binary target
--target <TRIPLE>             Cargo target triple
--features <FEATURES>         Space or comma separated Cargo features
--all-features                Activate all Cargo features
--no-default-features         Disable default Cargo features
--toolchain <NAME>            Use cargo +<NAME> doc (default: nightly)
-h, --help                    Print help
-V, --version                 Print version
```

## Output

DOT is written to standard output by default. Use `-o` or `--output` to write it
to a file.

By default, `cargo-typegraph` asks rustdoc to include private items so the graph
can describe internal type structure. Use `--no-private` when you only want the
types exposed by normal generated documentation.

## License

This project is dedicated to the public domain under CC0 1.0. See
[`LICENSE`](LICENSE) for details.
