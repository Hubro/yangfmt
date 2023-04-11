
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

- [ ] Automatically sort statements to comply with the canonical order (maybe behind a feature flag?)

- [ ] Better error messages, currently syntax errors are reported raw with a character index

  Instead, error messages should show the file name, line number and column number, plus details about the error.

- [ ] Automatically indent the contents of multi-line strings to align with the first line

  I want to avoid modifying string contents. Leading and trailing whitespace should be fair game though. For the rest of
  the string, only leading indentation should be touched.

- [ ] Fix all corner case crashes

  The [YangModels] repository contains over 80 000 YANG models (totalling over 2.2 GB). When running the formatter on
  every YANG file in this repository, there are still some corner cases that causes the formatter to crash.

- [ ] Handle comments in between string concatenations. Currently this causes a parse error. For example:

  ```yang
  pattern "abcdef"  // Comments here
        + "ghijkl"; // currently causes a parse error
  ```

  Fortunately I've never seen anybody do this, but it's legal YANG so it should be supported.

[YandModels]: https://github.com/YangModels/yang

## Ideas

- Automatically reflow the contents of `description` and `tailf:info` statements to comply with the configured line
  length. This would be a big developer experience win, but would make it impossible to add pre-formatted contents to
  these statements (such as ASCII diagrams, lists etc). If this is added, it will definitely be off by default.
