# SPDX-License-Identifier: GPL-2.0-only
import datetime
import json
import os
import sys
from collections import OrderedDict
from dataclasses import asdict, dataclass
from typing import Any
from unittest import mock

import nanoid
import typer
from beancount import loader
from beancount.core import account, data, interpolate, position
from beancount.parser import grammar

ROUNDING_ACCOUNT_NAME = "Equity:Rounding"


@dataclass
class AccountEntry:
    id: str
    na: str
    do: str
    un: list[str]
    dc: str | None = None


@dataclass
class Amount:
    n: str
    u: str


@dataclass
class Cost:
    n: str
    u: str
    d: str


@dataclass
class Posting:
    ac: str
    am: Amount
    rp: Amount | None = None
    ap: Amount | None = None
    co: Cost | None = None


@dataclass
class PreferencesEntry:
    ou: list[str]


@dataclass
class TransactionEntry:
    id: str
    ni: int
    dt: str
    de: str
    po: list[Posting]
    ta: list[str] | None
    co: str | None


@dataclass
class PriceEntry:
    d: str
    u: str
    a: Amount


@dataclass
class TagEntry:
    id: str
    na: str


def eprint(*args):
    print(*args, file=sys.stderr)


def format_date(date: datetime.date) -> str:
    return date.strftime("%Y-%m-%d")


def get_earliest_account_date(entries) -> datetime.date:
    date = datetime.datetime.now(tz=datetime.timzene.utc).date()
    for entry in entries:
        if not isinstance(entry, data.Open):
            continue
        date = min(date, entry.date)
    return date


def get_accounts_with_children(accounts: list[str]) -> set[str]:
    parents: set[str] = set()
    for name in accounts:
        for parent in account.parents(name):
            if parent != name:
                parents.add(parent)
    with_children: set[str] = set()
    for name in accounts:
        if name in parents:
            with_children.add(name)
    return with_children


@dataclass
class Context:
    accounts: OrderedDict[str, AccountEntry]
    accounts_with_children: set[str]
    tags: OrderedDict[str, TagEntry]


def convert_accounts(
    ctx: Context,
    entries,
    overlap_suffix: str | None,
) -> tuple[list[AccountEntry], list[str]]:
    names: list[str] = [
        entry.account for entry in entries if isinstance(entry, data.Open)
    ]
    accounts_with_children = get_accounts_with_children(names)

    if overlap_suffix is None and len(accounts_with_children) != 0:
        eprint("Some accounts have children and thus cannot be opened.")
        eprint(
            "Map to another account name if you want to keep it or it has postings on it."
        )
        eprint("Start the name with ':' to add a suffix to the existing account name.")
        eprint("Omitting the mapped name will exclude the account completely.")
        eprint()

    account_map: dict[str, str] = {}
    for name in sorted(accounts_with_children):
        if overlap_suffix is None:
            sys.stderr.write(f"{name}: ")
            mapped = input().strip()
            if len(mapped) == 0:
                continue
            if mapped.startswith(":"):
                mapped = f"{name}{mapped}"
        else:
            mapped = f"{name}:{overlap_suffix}"
        account_map[name] = mapped
        accounts_with_children.remove(name)

    accounts = OrderedDict()
    seen = set()
    errors = []
    for entry in entries:
        if not isinstance(entry, data.Open):
            continue
        if entry.account in accounts_with_children:
            continue

        name = account_map.get(entry.account, entry.account)
        if name in seen:
            errors.append(f"duplicate account name: {name}")

        seen.add(name)
        accounts[entry.account] = AccountEntry(
            id=nanoid.generate(size=10),
            na=name,
            do=format_date(entry.date),
            un=entry.currencies if entry.currencies is not None else [],
        )

    if ROUNDING_ACCOUNT_NAME not in accounts:
        earliest_date = get_earliest_account_date(entries)
        accounts[ROUNDING_ACCOUNT_NAME] = AccountEntry(
            id=nanoid.generate(size=10),
            na=ROUNDING_ACCOUNT_NAME,
            do=format_date(earliest_date),
            un=[],
        )

    for entry in entries:
        if not isinstance(entry, data.Close) or entry.account not in accounts:
            continue
        accounts[entry.account].dc = format_date(entry.date)

    ctx.accounts = accounts
    ctx.accounts_with_children = accounts_with_children
    return list(accounts.values()), errors


def convert_postings(
    ctx: Context,
    postings: list[data.Posting],
) -> tuple[list[Posting], list[str]]:
    result = []
    errors = []
    if len(postings) > 20:
        errors.append(
            "transaction cannot have more than 20 postings, "
            "consider splitting it into multiple transactions"
        )
        return [], errors

    for posting in postings:
        if posting.account in ctx.accounts_with_children:
            errors.append(f"posting to account {posting.account} that has children")
            continue

        amount = posting.units
        assert amount is not None, "amount should be set on all postings"
        item = Posting(
            ac=ctx.accounts[posting.account].id,
            am=Amount(n=str(amount.number), u=amount.currency),
        )

        price = posting.price
        if price is not None:
            absprice = (
                posting.meta.get("absprice") if posting.meta is not None else None
            )
            if absprice is None:
                item.rp = Amount(n=str(price.number), u=price.currency)
            else:
                item.ap = Amount(n=str(absprice.number), u=absprice.currency)

        cost = posting.cost
        if cost is not None:
            assert isinstance(cost, position.Cost), "cost should be of a cost type"
            if cost.label is not None:
                errors.append("posting with a cost label that is not supported")
                continue

            item.co = Cost(
                n=str(cost.number),
                u=cost.currency,
                d=format_date(cost.date),
            )
        result.append(item)
    return result, errors


def map_tags(ctx: Context, tag_names: list[str]) -> list[str]:
    ids = []
    for tag_name in tag_names:
        lower = tag_name.lower()
        if lower not in ctx.tags:
            ctx.tags[lower] = TagEntry(
                id=nanoid.generate(size=10),
                na=lower,
            )
        ids.append(ctx.tags[lower].id)
    return ids


def convert_transactions(
    ctx: Context,
    entries,
) -> tuple[list[TransactionEntry], list[str]]:
    transactions = []
    errors = []
    idx = 1
    for entry in entries:
        if not isinstance(entry, data.Transaction):
            continue

        # A hack around absprice - convert the posting to be at units and
        # compute the residual off that, because the relative price can result
        # in small fractions of residual, and since the target system treats
        # absprice exactly, these small fractions should not be added to the
        # rounding account.
        postings = list(entry.postings)
        for i, posting in enumerate(postings):
            if posting.meta is None:
                continue

            absprice = posting.meta.get("absprice")
            if absprice is None:
                continue

            if posting.units is None or posting.units.number is None:
                continue

            if posting.units.number < 0:
                postings[i] = posting._replace(units=-absprice, price=None)
            else:
                postings[i] = posting._replace(units=absprice, price=None)

        # Residual is necessary, because the target system currently does not
        # have tolerance configuration, and the transaction has to match
        # exactly. In other words, the residual have to go to the rounding
        # account, else transaction won't balance.
        residual = interpolate.compute_residual(postings)
        if not residual.is_empty():
            new_postings = list(entry.postings)
            new_postings.extend(
                interpolate.get_residual_postings(residual, ROUNDING_ACCOUNT_NAME)
            )
            entry = entry._replace(postings=new_postings)

        postings, posting_errors = convert_postings(
            ctx,
            entry.postings,
        )
        if posting_errors:
            error = (
                f"transaction at {entry.meta['filename']}:{entry.meta['lineno']} "
                "has postings that cannot be converted:"
            )
            errors.append(error + "\n  " + "\n  ".join(posting_errors))
            continue

        description = entry.narration
        if entry.narration and entry.payee:
            description = f"{entry.narration} @ {entry.payee}"
        elif entry.narration and not entry.payee:
            description = entry.narration
        elif not entry.narration and entry.payee:
            description = entry.payee
        else:
            description = "[imported from beancount]"

        tags = None
        if entry.tags:
            tags = map_tags(ctx, sorted(entry.tags))

        comments = [entry.meta["comment"]] if "comment" in entry.meta else []
        comments += [
            posting.meta["comment"]
            for posting in entry.postings
            if posting.meta is not None and "comment" in posting.meta
        ]

        transactions.append(
            TransactionEntry(
                id=nanoid.generate(),
                ni=idx,
                dt=format_date(entry.date),
                de=description,
                po=postings,
                ta=tags,
                co="\n".join(comments) if len(comments) != 0 else None,
            )
        )
        idx += 1

    return transactions, errors


def convert_prices(
    entries,
) -> tuple[list[PriceEntry], list[str]]:
    prices: OrderedDict[str, PriceEntry] = OrderedDict()
    errors = []
    for entry in entries:
        if not isinstance(entry, data.Price):
            continue

        key = f"{format_date(entry.date)}:{entry.currency}/{entry.amount.currency}"
        if key in prices:
            errors.append(
                f"duplicate price: {key} "
                f"at {entry.meta['filename']}:{entry.meta['lineno']}"
            )
            continue

        prices[key] = PriceEntry(
            d=format_date(entry.date),
            u=entry.currency,
            a=Amount(n=str(entry.amount.number), u=entry.amount.currency),
        )

    return list(prices.values()), errors


class GrammarBuilder(grammar.Builder):
    def posting(self, filename, lineno, account, units, cost, price, istotal, flag):
        posting = super().posting(
            filename, lineno, account, units, cost, price, istotal, flag
        )
        if istotal:
            if posting.meta is None:
                posting = posting._replace(meta={"absprice": price})
            else:
                posting.meta["absprice"] = price
        return posting


def load_beancount_file(
    filename: str,
) -> tuple[list[data.Directive], loader.OptionsMap]:
    if not os.path.isfile(filename):
        eprint(f"File {filename} is not a regular file")
        raise typer.Exit(code=1)

    with mock.patch.object(grammar, "Builder", GrammarBuilder):
        entries, errors, opts = loader._uncached_load_file(
            os.path.abspath(filename), None, None, None
        )

    if errors:
        eprint(f"Loading file {filename} failed, errors:")
        for error in errors:
            eprint("  ", error)
        raise typer.Exit(code=1)

    return entries, opts


def drop_none(adict: dict[str, Any]):
    for key, value in list(adict.items()):
        if value is None:
            del adict[key]
        if isinstance(value, dict):
            drop_none(value)
        if isinstance(value, list):
            for item in value:
                if isinstance(item, dict):
                    drop_none(item)
    return adict


def to_json(
    entry: AccountEntry | PreferencesEntry | PriceEntry | TagEntry | TransactionEntry,
) -> str:
    if isinstance(entry, AccountEntry):
        ty = 1
    elif isinstance(entry, TransactionEntry):
        ty = 2
    elif isinstance(entry, PriceEntry):
        ty = 3
    elif isinstance(entry, PreferencesEntry):
        ty = 5
    elif isinstance(entry, TagEntry):
        ty = 6
    return json.dumps({"t": ty, "d": drop_none(asdict(entry))}, separators=(",", ":"))


def main(filename: str, overlap_suffix: str | None = None):
    entries, opts = load_beancount_file(filename)

    ctx = Context(
        accounts=OrderedDict(), accounts_with_children=set(), tags=OrderedDict()
    )

    accounts, errors = convert_accounts(ctx, entries, overlap_suffix)
    if errors:
        eprint("Failed to convert accounts:")
        for error in errors:
            eprint("* ", error)
        raise typer.Exit(code=1)

    transactions, errors = convert_transactions(ctx, entries)
    if errors:
        eprint("Failed to convert transactions:")
        for error in errors:
            eprint("* ", error)
        raise typer.Exit(code=1)

    prices, errors = convert_prices(entries)
    if errors:
        eprint("Failed to convert prices:")
        for error in errors:
            eprint("* ", error)
        raise typer.Exit(code=1)

    operating_units = opts.get("operating_currency", [])
    if len(operating_units) > 3:
        eprint("Too many operating currencies, must be at most 3")
        raise typer.Exit(code=1)

    print(to_json(PreferencesEntry(ou=operating_units)))
    for acc in accounts:
        print(to_json(acc))
    for txn in transactions:
        print(to_json(txn))
    for pri in prices:
        print(to_json(pri))
    for tag in ctx.tags.values():
        print(to_json(tag))

    raise typer.Exit()


if __name__ == "__main__":
    typer.run(main)
