# Beancount to Scrooje

## Overview

This is a little script to convert data from Beancount format to
[Scrooje](https://scroo.je/). Since Scrooje does not cover all Beancount
features, there is some information loss. For instance, balance assertions and
metadata (except transaction comments) are not converted.

There are some differences in data model between two formats. Scrooje does not
allow creating overlapping account hierarchies. In other words, it is not
possible to open `Assets:Bank` and `Assets:Bank:Card`. Instead, it is required
to append a suffix to the former account.

Transactions cannot have more than 20 postings, meaning these transactions have
to be split into multiple transactions.

All residual is put into `Equity:Rounding` account.

## Converting to Scrooje

1.  Install dependencies:
    ```sh
    pip3 install -r requirements.txt
    ```
2.  Convert the data by pointing the script to main Beancount file
    ```sh
    python3 main.py <path-to-beancount-main> > entries.json
    ```
    You will be prompted to rename overlapping account. Alternatively, you can
    pass `--overlap-suffix` argument to append that suffix to all overlapping
    accounts.
3.  Sign into [Scrooje](https://app.scroo.je/auth/login), create a book, go to
    book settings and restore from backup, selecting resulting `entries.json`
    file.

## License

Licensed under the GNU General Public License v2.0, only (GPL-2.0-only).
