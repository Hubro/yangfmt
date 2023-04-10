
# yangfmt

YANG code formatter

## Install

If you have the Rust toolchain installed:

```
$ cargo install yangfmt
```

Pre-compiled binaries will be provided soon.

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

Pretty well tested, should be safe to use!

But please keep your code in version control just in case.

TODO:

- [ ] Better error messages, currently syntax errors are reported raw with a character index

  Instead, error messages should show the file name, line number and column number, plus details about the error.

- [ ] Automatically indent the contents of multi-line strings to align with the first line

- [ ] Handle comments in between string concatenations. Currently this causes a parse error. For example:

  ```yang
  pattern "abcdef"  // Comments here
        + "ghijkl"; // currently causes a parse error
  ```

  Fortunately I've never seen anybody do this, but it's legal YANG so it should be supported.
