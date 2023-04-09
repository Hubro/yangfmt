
# yangfmt

YANG code formatter

## Install

Clone the repo, then:

```
$ cargo install --path .
```

Pre-compiled binaries will be provided when the project has stabilized.

## Usage

Pipe YANG source code to STDIN:

```
$ cat my-model.yang | yangfmt
```

Or specify a YANG file as the first positional argument:

```
$ yangfmt my-model.yang
```

Add `-i` to format the given YANG file in place:

```
$ yangfmt -i my-model.yang
```

## Status

Experimental! Use with caution.

TODO:

- [ ] Automatically indent the contents of multi-line strings to align with the first line
- [ ] Handle comments in between string concatenations. Currently this causes a parse error. For example:

  ```yang
  pattern "abcdef"  // Comments here
        + "ghijkl"; // currently causes a parse error
  ```
