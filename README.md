
# yangfmt

YANG code formatter

## Install

Download the correct pre-compiled binary for your system from [Releases](https://github.com/Hubro/yangfmt/releases).

Alternatively, if you have the Rust toolchain installed:

```
$ cargo install yangfmt
```

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

- [ ] Fix all corner case crashes

  The [YangModels] repository contains over 80 000 YANG models (totalling over 2.2 GB). When running the formatter on
  every YANG file in this repository, there are still some corner cases that causes the formatter to crash.

- [ ] Handle comments in between string concatenations. Currently this causes a parse error. For example:

  ```yang
  pattern "abcdef"  // Comments here
        + "ghijkl"; // currently causes a parse error
  ```

  Fortunately I've never seen anybody do this, but it's legal YANG so it should be supported.

[YangModels]: https://github.com/YangModels/yang

## Ideas

- Automatically reflow the contents of `description` and `tailf:info` statements to comply with the configured line
  length. This would be a big developer experience win, but would make it impossible to add pre-formatted contents to
  these statements (such as ASCII diagrams, lists etc). If this is added, it will definitely be off by default.
