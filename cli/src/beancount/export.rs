use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;

use chrono::Datelike;

use scrooje_core::{entry, types};

pub(crate) struct ExportContext {
    // Account entries in the order they appear.
    accounts: Vec<entry::AccountEntry>,
    // Account names by id.
    account_name_by_id: HashMap<String, types::AccountName>,
    // Commodities derived from transactions.
    commodities: HashMap<types::UnitName, Commodity>,
    // Operating currencies.
    operating_currencies: Vec<types::UnitName>,
    // Price entries by natural id - date and unit pair.
    prices: BTreeMap<(chrono::NaiveDate, types::UnitName, types::UnitName), entry::PriceEntry>,
    // Tag entries by id.
    tags: HashMap<String, entry::TagEntry>,
    // Transaction entries by date.
    transactions: BTreeMap<(chrono::NaiveDate, u32), entry::TransactionEntry>,
}

impl ExportContext {
    pub(crate) fn new(entries: &[entry::Entry]) -> Result<ExportContext, ExportContextError> {
        let mut accounts = Vec::new();
        let mut account_name_by_id = HashMap::new();
        let mut commodities: HashMap<_, Commodity> = HashMap::new();
        let mut operating_currencies = Vec::new();
        let mut prices = BTreeMap::new();
        let mut tags = HashMap::new();
        let mut transactions = BTreeMap::new();

        for entry in entries {
            match entry {
                entry::Entry::Account(account) => {
                    if account_name_by_id
                        .insert(account.id.clone(), account.name.clone())
                        .is_some()
                    {
                        return Err(ExportContextError::DuplicateAccountId(account.id.clone()));
                    }
                    accounts.push(account.clone());
                }
                entry::Entry::Preferences(prefs) => {
                    operating_currencies.clear();
                    operating_currencies.extend(prefs.operating_units.iter().cloned());
                }
                entry::Entry::Price(price) => {
                    if prices
                        .insert(
                            (price.date, price.unit.clone(), price.amount.unit.clone()),
                            price.clone(),
                        )
                        .is_some()
                    {
                        return Err(ExportContextError::DuplicatePrice(
                            price.date,
                            price.unit.clone(),
                            price.amount.unit.clone(),
                        ));
                    }
                }
                entry::Entry::Tag(tag) => {
                    if tags.insert(tag.id.clone(), tag.clone()).is_some() {
                        return Err(ExportContextError::DuplicateTagId(tag.id.clone()));
                    }
                }
                entry::Entry::Transaction(txn) => {
                    if transactions
                        .insert((txn.date, txn.numeric_id), txn.clone())
                        .is_some()
                    {
                        return Err(ExportContextError::DuplicationTransaction(
                            txn.date,
                            txn.numeric_id,
                        ));
                    }

                    for posting in &txn.postings {
                        commodities
                            .entry(posting.amount.unit.clone())
                            .and_modify(|commodity| {
                                if commodity.date > txn.date {
                                    commodity.date = txn.date;
                                }
                            })
                            .or_insert_with(|| Commodity {
                                date: txn.date,
                                name: posting.amount.unit.clone(),
                            });
                    }
                }
                _ => {}
            }
        }

        Ok(ExportContext {
            accounts,
            account_name_by_id,
            commodities,
            operating_currencies,
            prices,
            tags,
            transactions,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExportContextError {
    #[error("duplicate account id: {0}")]
    DuplicateAccountId(String),
    #[error("duplicate price: {1}/{2} @ {0}")]
    DuplicatePrice(chrono::NaiveDate, types::UnitName, types::UnitName),
    #[error("duplicate tag id: {0}")]
    DuplicateTagId(String),
    #[error("duplicate transaction: {0}:{1}")]
    DuplicationTransaction(chrono::NaiveDate, u32),
}

pub(crate) fn export<P: AsRef<std::path::Path>>(
    ctx: &ExportContext,
    output_dir: P,
) -> Result<(), ExportError> {
    if !output_dir.as_ref().is_dir() {
        return Err(ExportError::NotADirectory(output_dir.as_ref().into()));
    }

    let mut main_file = std::io::BufWriter::new(std::fs::File::create_new(
        output_dir.as_ref().join("main.beancount"),
    )?);

    let accounts_file_path = output_dir.as_ref().join("accounts.beancount");
    let mut accounts_file =
        std::io::BufWriter::new(std::fs::File::create_new(&accounts_file_path)?);

    let prices_file_path = output_dir.as_ref().join("prices.beancount");
    let mut prices_file = std::io::BufWriter::new(std::fs::File::create_new(&prices_file_path)?);

    let transactions_dir = output_dir.as_ref().join("transactions");
    std::fs::create_dir(&transactions_dir)?;
    let transactions_file_path = output_dir.as_ref().join("transactions.beancount");
    let mut transactions_file =
        std::io::BufWriter::new(std::fs::File::create_new(&transactions_file_path)?);

    write_main_file(ctx, &mut main_file)?;
    main_file.flush()?;

    write_accounts_file(ctx, &mut accounts_file)?;
    accounts_file.flush()?;

    let txn_years: BTreeSet<_> = ctx
        .transactions
        .keys()
        .map(|(date, _)| date.year())
        .collect();
    let current_year = chrono::Utc::now().year();
    let mut partitions = HashMap::new();
    for year in txn_years {
        if year == current_year {
            continue;
        }
        writeln!(
            &mut transactions_file,
            "include \"./transactions/{year}.beancount\""
        )?;
        let path = transactions_dir.join(format!("{year}.beancount"));
        partitions.insert(
            year,
            std::io::BufWriter::new(std::fs::File::create_new(&path)?),
        );
    }
    writeln!(&mut transactions_file)?;
    partitions.insert(current_year, transactions_file);
    write_transaction_files(ctx, &mut partitions)?;
    for mut buf in partitions.into_values() {
        buf.flush()?;
    }

    write_prices_file(ctx, &mut prices_file)?;
    prices_file.flush()?;

    Ok(())
}

fn write_main_file(ctx: &ExportContext, out: &mut impl Write) -> Result<(), ExportError> {
    writeln!(out, "* Options")?;
    writeln!(out)?;
    writeln!(out, "option \"title\" \"scrooje export\"")?;
    for currency in &ctx.operating_currencies {
        writeln!(out, "option \"operating_currency\" \"{currency}\"")?;
    }
    writeln!(out)?;

    writeln!(out, "* Commodities")?;
    writeln!(out)?;
    let sorted: BTreeSet<_> = ctx.commodities.values().cloned().collect();
    for commodity in &sorted {
        writeln!(out, "{} commodity {}", commodity.date, commodity.name)?
    }
    writeln!(out)?;

    writeln!(out, "* Includes")?;
    writeln!(out)?;
    writeln!(out, "include \"./accounts.beancount\"",)?;
    writeln!(out, "include \"./transactions.beancount\"",)?;
    writeln!(out, "include \"./prices.beancount\"",)?;

    Ok(())
}

fn write_accounts_file(ctx: &ExportContext, out: &mut impl Write) -> Result<(), ExportError> {
    for account in &ctx.accounts {
        if account.units.is_empty() {
            writeln!(out, "{} open {}", account.date_opened, account.name)?;
        } else {
            let units: Vec<_> = account
                .units
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
            writeln!(
                out,
                "{} open {} {}",
                account.date_opened,
                account.name,
                units.join(",")
            )?;
        }
        if let Some(date_closed) = account.date_closed {
            writeln!(out, "{date_closed} close {}", account.name)?;
        }
        writeln!(out)?;
    }

    Ok(())
}

fn write_prices_file(ctx: &ExportContext, out: &mut impl Write) -> Result<(), ExportError> {
    for price in ctx.prices.values() {
        writeln!(
            out,
            "{} price {} {} {}",
            price.date, price.unit, price.amount.number, price.amount.unit
        )?;
    }

    Ok(())
}

fn write_transaction_files(
    ctx: &ExportContext,
    partitions: &mut HashMap<i32, impl Write>,
) -> Result<(), ExportError> {
    let mut last_year_month = (0, 0);
    for txn in ctx.transactions.values() {
        let year = txn.date.year();
        let month = txn.date.month();

        let out = partitions
            .get_mut(&year)
            .ok_or(ExportError::UnknownPartition(year))?;
        if last_year_month != (year, month) {
            last_year_month = (year, month);
            writeln!(out, "* {year}-{month:0>2}")?;
            writeln!(out)?;
        }

        let (narration, payee) = txn.description.split();

        let tags = match txn.tags.as_ref() {
            Some(tags) => {
                let tag_names: Result<Vec<_>, _> = tags
                    .iter()
                    .map(|id| {
                        ctx.tags
                            .get(id)
                            .map(|tag| format!("#{}", tag.name))
                            .ok_or_else(|| ExportError::UnmatchedTag(id.clone()))
                    })
                    .collect();
                format!(" {}", tag_names?.join(" "))
            }
            None => "".to_owned(),
        };

        if payee.is_empty() {
            writeln!(out, "{} * \"{narration}\"{tags}", txn.date)?;
        } else {
            writeln!(out, "{} * \"{payee}\" \"{narration}\"{tags}", txn.date)?;
        }

        if let Some(comment) = txn.comment.as_ref() {
            let lines: Vec<_> = comment
                .to_string()
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(std::string::ToString::to_string)
                .collect();
            writeln!(out, "  comment: \"{}\"", lines.join(" "))?;
        }

        for posting in &txn.postings {
            write_posting(ctx, posting, out)?;
        }

        writeln!(out)?;
    }

    Ok(())
}

fn write_posting(
    ctx: &ExportContext,
    posting: &entry::Posting,
    out: &mut impl Write,
) -> Result<(), ExportError> {
    let mut extra = vec![];
    if let Some(cost) = posting.cost.as_ref() {
        extra.push(format!(
            "{{ {} {}, {} }}",
            cost.number, cost.unit, cost.date
        ));
    }
    match (posting.absprice.as_ref(), posting.relprice.as_ref()) {
        (Some(absprice), _) => {
            extra.push("@@".to_owned());
            extra.push(format!("{} {}", absprice.number, absprice.unit));
        }
        (None, Some(relprice)) => {
            extra.push("@".to_owned());
            extra.push(format!("{} {}", relprice.number, relprice.unit));
        }
        (None, None) => {}
    }
    let extra = if !extra.is_empty() {
        format!(" {}", extra.join(" "))
    } else {
        "".to_owned()
    };

    let account_name = ctx
        .account_name_by_id
        .get(&posting.account)
        .ok_or_else(|| ExportError::UnmatchedAccount(posting.account.clone()))?;

    writeln!(
        out,
        "  {account_name} {} {}{}",
        posting.amount.number, posting.amount.unit, extra
    )?;

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExportError {
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("path is not a directory: {0}")]
    NotADirectory(std::path::PathBuf),
    #[error("unknown parition file: {0}")]
    UnknownPartition(i32),
    #[error("unmatched account id: {0}")]
    UnmatchedAccount(String),
    #[error("unmatched tag id: {0}")]
    UnmatchedTag(String),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Commodity {
    date: chrono::NaiveDate,
    name: types::UnitName,
}

#[cfg(test)]
mod tests {
    use nanoid::nanoid;

    use super::*;

    #[test]
    fn write_main_file_basic() {
        let ctx = test_build_export_ctx();
        let mut buf = Vec::new();

        write_main_file(&ctx, &mut buf).unwrap();
        let contents = std::str::from_utf8(&buf).unwrap();
        let expected = [
            "* Options",
            "",
            "option \"title\" \"scrooje export\"",
            "option \"operating_currency\" \"USD\"",
            "option \"operating_currency\" \"EUR\"",
            "",
            "* Commodities",
            "",
            "2023-03-01 commodity EUR",
            "2024-03-01 commodity AAPL",
            "2024-03-01 commodity USD",
            "",
            "* Includes",
            "",
            "include \"./accounts.beancount\"",
            "include \"./transactions.beancount\"",
            "include \"./prices.beancount\"",
            "",
        ]
        .join("\n");
        assert_eq!(contents, expected);
    }

    #[test]
    fn write_accounts_file_basic() {
        let ctx = test_build_export_ctx();
        let mut buf = Vec::new();

        write_accounts_file(&ctx, &mut buf).unwrap();

        let contents = std::str::from_utf8(&buf).unwrap();
        let expected = [
            "2023-01-01 open Assets:Cash:EUR EUR",
            "",
            "2023-01-01 open Assets:Cash:USD USD",
            "",
            "2024-02-07 open Assets:Bank:Checking USD",
            "2025-02-07 close Assets:Bank:Checking",
            "",
            "2024-02-07 open Assets:Stocks:AAPL AAPL",
            "",
            "2023-01-01 open Expenses:Food",
            "",
            "",
        ]
        .join("\n");
        assert_eq!(contents, expected);
    }

    #[test]
    fn write_prices_file_basic() {
        let ctx = test_build_export_ctx();
        let mut buf = Vec::new();

        write_prices_file(&ctx, &mut buf).unwrap();

        let contents = std::str::from_utf8(&buf).unwrap();
        let expected = [
            "2025-01-01 price EUR 1.423 USD",
            "2025-01-02 price EUR 1.443 USD",
            "",
        ]
        .join("\n");
        assert_eq!(contents, expected);
    }

    #[test]
    fn write_transaction_files_basic() {
        let ctx = test_build_export_ctx();
        let mut partitions: HashMap<_, _> =
            [(2023, Vec::new()), (2024, Vec::new()), (2025, Vec::new())].into();

        write_transaction_files(&ctx, &mut partitions).unwrap();

        let part_2023 = std::str::from_utf8(partitions.get(&2023).unwrap()).unwrap();
        let expected = r#"* 2023-03

2023-03-01 * "Store" "Buy food"
  comment: "Some food items Including household"
  Expenses:Food 10.123 EUR
  Assets:Cash:EUR -10.123 EUR

* 2023-04

2023-04-01 * "Store" "Buy food"
  Expenses:Food 11.13 EUR
  Assets:Cash:EUR -11.13 EUR

"#;
        assert_eq!(part_2023, expected);

        let part_2024 = std::str::from_utf8(partitions.get(&2024).unwrap()).unwrap();
        let expected = r#"* 2024-03

2024-03-01 * "Buy AAPL stock"
  Assets:Stocks:AAPL 12 AAPL { 100 USD, 2024-03-01 } @ 100 USD
  Assets:Bank:Checking -1200 USD

"#;
        assert_eq!(part_2024, expected);

        let part_2025 = std::str::from_utf8(partitions.get(&2025).unwrap()).unwrap();
        let expected = r#"* 2025-03

2025-03-01 * "Currency exchange" #trip-somewhere-2025 #somewhere
  Assets:Cash:USD 37.13 USD @@ 35 EUR
  Assets:Cash:EUR -35 EUR

"#;
        assert_eq!(part_2025, expected);
    }

    fn test_build_export_ctx() -> ExportContext {
        let entries = [
            entry::Entry::Preferences(entry::PreferencesEntry {
                version: None,
                operating_units: vec!["USD".parse().unwrap(), "EUR".parse().unwrap()],
                auto_upload: None,
            }),
            entry::Entry::Account(entry::AccountEntry {
                id: "a-cash-eur".to_owned(),
                name: "Assets:Cash:EUR".parse().unwrap(),
                date_opened: "2023-01-01".parse().unwrap(),
                units: vec!["EUR".parse().unwrap()],
                date_closed: None,
                ty: None,
                trading_config: None,
                icon: None,
            }),
            entry::Entry::Account(entry::AccountEntry {
                id: "a-cash-usd".to_owned(),
                name: "Assets:Cash:USD".parse().unwrap(),
                date_opened: "2023-01-01".parse().unwrap(),
                units: vec!["USD".parse().unwrap()],
                date_closed: None,
                ty: None,
                trading_config: None,
                icon: None,
            }),
            entry::Entry::Account(entry::AccountEntry {
                id: "a-bank".to_owned(),
                name: "Assets:Bank:Checking".parse().unwrap(),
                date_opened: "2024-02-07".parse().unwrap(),
                units: vec!["USD".parse().unwrap()],
                date_closed: Some("2025-02-07".parse().unwrap()),
                ty: None,
                trading_config: None,
                icon: None,
            }),
            entry::Entry::Account(entry::AccountEntry {
                id: "a-stock".to_owned(),
                name: "Assets:Stocks:AAPL".parse().unwrap(),
                date_opened: "2024-02-07".parse().unwrap(),
                units: vec!["AAPL".parse().unwrap()],
                date_closed: None,
                ty: None,
                trading_config: None,
                icon: None,
            }),
            entry::Entry::Account(entry::AccountEntry {
                id: "e-food".to_owned(),
                name: "Expenses:Food".parse().unwrap(),
                date_opened: "2023-01-01".parse().unwrap(),
                units: vec![],
                date_closed: None,
                ty: None,
                trading_config: None,
                icon: None,
            }),
            entry::Entry::Price(entry::PriceEntry {
                date: "2025-01-01".parse().unwrap(),
                unit: "EUR".parse().unwrap(),
                amount: entry::Amount {
                    number: "1.423".parse().unwrap(),
                    unit: "USD".parse().unwrap(),
                },
            }),
            entry::Entry::Price(entry::PriceEntry {
                date: "2025-01-02".parse().unwrap(),
                unit: "EUR".parse().unwrap(),
                amount: entry::Amount {
                    number: "1.443".parse().unwrap(),
                    unit: "USD".parse().unwrap(),
                },
            }),
            entry::Entry::Tag(entry::TagEntry {
                id: "1".to_owned(),
                name: "trip-somewhere-2025".parse().unwrap(),
                color: None,
            }),
            entry::Entry::Tag(entry::TagEntry {
                id: "2".to_owned(),
                name: "somewhere".parse().unwrap(),
                color: None,
            }),
            entry::Entry::Transaction(entry::TransactionEntry {
                id: nanoid!(),
                numeric_id: 1,
                date: "2023-03-01".parse().unwrap(),
                description: "Buy food @ Store".parse().unwrap(),
                postings: vec![
                    entry::Posting {
                        account: "e-food".to_owned(),
                        amount: entry::Amount {
                            number: "10.123".parse().unwrap(),
                            unit: "EUR".parse().unwrap(),
                        },
                        relprice: None,
                        absprice: None,
                        cost: None,
                    },
                    entry::Posting {
                        account: "a-cash-eur".to_owned(),
                        amount: entry::Amount {
                            number: "-10.123".parse().unwrap(),
                            unit: "EUR".parse().unwrap(),
                        },
                        relprice: None,
                        absprice: None,
                        cost: None,
                    },
                ],
                tags: None,
                comment: Some("Some food items\r\n\nIncluding household".parse().unwrap()),
            }),
            entry::Entry::Transaction(entry::TransactionEntry {
                id: nanoid!(),
                numeric_id: 2,
                date: "2023-04-01".parse().unwrap(),
                description: "Buy food @ Store".parse().unwrap(),
                postings: vec![
                    entry::Posting {
                        account: "e-food".to_owned(),
                        amount: entry::Amount {
                            number: "11.13".parse().unwrap(),
                            unit: "EUR".parse().unwrap(),
                        },
                        relprice: None,
                        absprice: None,
                        cost: None,
                    },
                    entry::Posting {
                        account: "a-cash-eur".to_owned(),
                        amount: entry::Amount {
                            number: "-11.13".parse().unwrap(),
                            unit: "EUR".parse().unwrap(),
                        },
                        relprice: None,
                        absprice: None,
                        cost: None,
                    },
                ],
                tags: None,
                comment: None,
            }),
            entry::Entry::Transaction(entry::TransactionEntry {
                id: nanoid!(),
                numeric_id: 3,
                date: "2024-03-01".parse().unwrap(),
                description: "Buy AAPL stock".parse().unwrap(),
                postings: vec![
                    entry::Posting {
                        account: "a-stock".to_owned(),
                        amount: entry::Amount {
                            number: "12".parse().unwrap(),
                            unit: "AAPL".parse().unwrap(),
                        },
                        relprice: Some(entry::Amount {
                            number: "100".parse().unwrap(),
                            unit: "USD".parse().unwrap(),
                        }),
                        absprice: None,
                        cost: Some(entry::Cost {
                            date: "2024-03-01".parse().unwrap(),
                            number: "100".parse().unwrap(),
                            unit: "USD".parse().unwrap(),
                        }),
                    },
                    entry::Posting {
                        account: "a-bank".to_owned(),
                        amount: entry::Amount {
                            number: "-1200".parse().unwrap(),
                            unit: "USD".parse().unwrap(),
                        },
                        relprice: None,
                        absprice: None,
                        cost: None,
                    },
                ],
                tags: None,
                comment: None,
            }),
            entry::Entry::Transaction(entry::TransactionEntry {
                id: nanoid!(),
                numeric_id: 4,
                date: "2025-03-01".parse().unwrap(),
                description: "Currency exchange".parse().unwrap(),
                postings: vec![
                    entry::Posting {
                        account: "a-cash-usd".to_owned(),
                        amount: entry::Amount {
                            number: "37.13".parse().unwrap(),
                            unit: "USD".parse().unwrap(),
                        },
                        relprice: None,
                        absprice: Some(entry::Amount {
                            number: "35".parse().unwrap(),
                            unit: "EUR".parse().unwrap(),
                        }),
                        cost: None,
                    },
                    entry::Posting {
                        account: "a-cash-eur".to_owned(),
                        amount: entry::Amount {
                            number: "-35".parse().unwrap(),
                            unit: "EUR".parse().unwrap(),
                        },
                        relprice: None,
                        absprice: None,
                        cost: None,
                    },
                ],
                tags: Some(vec!["1".to_owned(), "2".to_owned()]),
                comment: None,
            }),
        ];

        ExportContext::new(entries.as_slice()).unwrap()
    }
}
