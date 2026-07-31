extern crate scrooje_core;

use std::io::Write;

use scrooje_core::{extract, extract_v1, types};

const DATA: &[u8] = include_bytes!("./data/cash-expenses.json");

fn main() {
    let format = extract::Format::from_json(DATA).unwrap();
    let data = match format {
        extract::Format::V1(data) => data,
    };

    let base_account: types::AccountName = "Assets:Cash:Other".parse().unwrap();
    let mut transactions = Vec::new();
    for txn in data.transactions() {
        let mut postings = Vec::new();
        for posting in txn.postings() {
            let mut posting_clone = posting.clone();
            if posting.account() != &base_account {
                postings.push(posting_clone);
                continue;
            }

            let unit = &posting.amount().unit;
            let account: types::AccountName = format!("Assets:Cash:{unit}").parse().unwrap();
            posting_clone.with_account(account);
            postings.push(posting_clone);
        }

        let mut txn_clone = txn.clone();
        txn_clone.set_postings(postings).unwrap();
        transactions.push(txn_clone);
    }

    let transformed = extract_v1::Transactions::new(transactions);

    std::io::stdout()
        .lock()
        .write_all(&serde_json::to_vec_pretty(&transformed).unwrap())
        .unwrap();
}
