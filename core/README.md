# scrooje-core

This library contains data interchange formats and types for working with
[Scrooje](https://scroo.je/).

There are two distinct formats:
-   JSON-lines `entry`, which is used in data backups and in plaintext of
    encrypted blobs uploaded to the server.
-   JSON `extract` - the format of exporting and importing the transactions via
    the web application.

The `entry` format may be useful for exporting and importing data between
bookkeeping systems.

The `extract` format may be useful for bulk data imports and overwrites, where
a script could do the necessary transformation on the extracted transaction
file, or pull the data from an API and form a JSON for a subsequent import.

See [`examples`](examples/) directory for sample usage.

## License

Licensed under Apache License, Version 2.0 ([LICENSE](LICENSE)).
