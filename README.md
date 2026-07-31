# scrooje-toolkit

This is a complementary toolkit for [Scrooje](https://scroo.je). It is composed
of a command line utility, a library for manipulating Scrooje data formats,
cryptographic primitives and tools for converting data from various systems to
Scrooje.

The repository has the following structure:
  -  `cli/` - scrooje-cli binary.
  -  `core/` - core library with data types.
  -  `crypto/` - crypto primitives.
  -  `scripts/` - other scripts for converting data between formats.

## Installing scrooje-cli

1.  Install `cargo` by following
    [these steps](https://doc.rust-lang.org/cargo/getting-started/installation.html).
2.  Install `scrooje-cli`:
    ```sh
    cargo install scrooje-cli --features passkey
    ```
3.  Run `cargo scrooje --help` to see the list of available commands.

## Converting between formats

If you would like to convert from Scrooje to Beancount, use `scrooje-cli`.

If you would like to convert from Beancount to Scrooje, see
[`beancount-to-scrooje`](scripts/beancount-to-scrooje/) script.

## License

All code is licensed under Apache License, Version 2.0, except when a license
file is present in a subdirectory, which takes precedence over the top-level
license file.

See [LICENSE](LICENSE).
