
# yangfmt

YANG auto-formatter, inspired by the consistent style of IETF YANG models

## Install

Clone the repo, then:

```
$ cargo install --path .
```

Pre-compiled binaries will be provided when the project has stabilized.

## Usage

You can specify a YANG file as the first positional argument:

```
$ yangfmt my-model.yang
```

Or pipe a YANG source file as STDIN:

```
$ cat my-model.yang | yangfmt
```

## Features

- Consistent indentation
- Trims excessive whitespace, allows max 1 empty line between statements
- Removes empty lines at the start and end of blocks
- Aligns concatenated strings with the original keyword

## Status

Experimental! Use with caution.

TODO:

- [ ] Move statement values to the next line, based on the configured max length
- [ ] Always move multi-line strings to a new line
- [ ] Auto-indent the contents of multi-line strings to align with the first line
- [ ] Handle comments in between string concatenations. Currently this causes a parse error. For example:

  ```yang
  pattern "abcdef"  // Comments here
        + "ghijkl"; // currently causes a parse error
  ```
