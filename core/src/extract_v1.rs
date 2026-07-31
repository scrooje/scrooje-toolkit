//! Data type definitions for version 1 of extracted transactions format.
//!
//! The types are not meant to be mutable, instead for every transformation, a
//! new instance of [`Transactions`] should be created.
use nanoid::nanoid;

use crate::{
    extract::Version,
    types::{AccountName, Comment, Description, MAX_POSTINGS, MAX_TAGS, TagName, UnitName},
};

/// The main object as it appears in the extracted transactions JSON file.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Transactions {
    version: Version,
    processed_at: chrono::DateTime<chrono::Utc>,
    transactions: Vec<Transaction>,
}

impl Transactions {
    /// Creates new `Transactions` from a vector of `Transaction`.
    pub fn new(transactions: Vec<Transaction>) -> Transactions {
        Transactions {
            version: Version::V1_0,
            processed_at: chrono::Utc::now(),
            transactions,
        }
    }

    /// Returns the version of extracted transactions format.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Returns the timestamp at which the extracted transactions were
    /// processed.
    pub fn processed_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.processed_at
    }

    /// Returns the transactions as they were extracted.
    pub fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }
}

/// A single transaction of extracted format.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Transaction {
    id: String,
    numeric_id: u32,
    date: chrono::NaiveDate,
    description: Description,
    tags: Vec<TagName>,
    postings: Vec<Posting>,
    comment: Option<Comment>,
}

impl Transaction {
    /// Creates new `Transaction` by passing the mandatory fields.
    ///
    /// The value of `id` must match the existing transaction if it is to be
    /// updated when loading back. Otherwise, the ID can be any non-overlapping
    /// number with the existing transaction IDs.
    pub fn new(numeric_id: u32, date: chrono::NaiveDate, description: Description) -> Transaction {
        Transaction {
            id: nanoid!(),
            numeric_id,
            date,
            description,
            tags: vec![],
            postings: vec![],
            comment: None,
        }
    }

    /// Returns date of this transaction.
    pub fn date(&self) -> chrono::NaiveDate {
        self.date
    }

    /// Returns description of this transaction.
    pub fn description(&self) -> &Description {
        &self.description
    }

    /// Sets the comment field of transaction.
    pub fn with_comment(&mut self, comment: Comment) -> &mut Transaction {
        self.comment = Some(comment);
        self
    }

    /// Drops the comment field of transaction.
    pub fn without_comment(&mut self) -> &mut Transaction {
        self.comment = None;
        self
    }

    /// Appends a tag name to a list of tags of this transaction.
    ///
    /// # Errors
    ///
    /// Returns error if the tag repeats or if the number of tags exceeds
    /// [`MAX_TAGS`].
    pub fn add_tag(&mut self, tag: TagName) -> Result<&mut Transaction, TransactionError> {
        if self.tags.contains(&tag) {
            return Err(TransactionError::RepeatingTags(tag));
        }

        if self.tags.len() == MAX_TAGS {
            return Err(TransactionError::TooManyTags(
                self.tags.len().saturating_add(1),
            ));
        }

        self.tags.push(tag);
        Ok(self)
    }

    /// Sets tags of this transaction to a pre-allocated vector of tag names.
    ///
    /// # Errors
    ///
    /// Returns error if the tag repeats or if the number of tags exceeds
    /// [`MAX_TAGS`].
    pub fn set_tags(&mut self, tags: Vec<TagName>) -> Result<&mut Transaction, TransactionError> {
        let mut set = std::collections::HashSet::new();
        for tag in &tags {
            if !set.insert(tag.clone()) {
                return Err(TransactionError::RepeatingTags(tag.clone()));
            }
        }

        if tags.len() > MAX_TAGS {
            return Err(TransactionError::TooManyTags(tags.len()));
        }

        self.tags = tags;
        Ok(self)
    }

    /// Returns a list of tag names.
    pub fn tags(&self) -> &[TagName] {
        &self.tags
    }

    /// Appends a positng to this transaction.
    ///
    /// # Errors
    ///
    /// Returns error if the number of postings is above [`MAX_POSTINGS`]
    pub fn add_posting(&mut self, posting: Posting) -> Result<&mut Transaction, TransactionError> {
        if self.postings.len() == MAX_POSTINGS {
            return Err(TransactionError::TooManyPostings(
                self.postings.len().saturating_add(1),
            ));
        }

        self.postings.push(posting);
        Ok(self)
    }

    /// Sets postings to a pre-allocated vector.
    ///
    /// # Errors
    ///
    /// Returns error if the number of postings is above [`MAX_POSTINGS`]
    pub fn set_postings(
        &mut self,
        postings: Vec<Posting>,
    ) -> Result<&mut Transaction, TransactionError> {
        if postings.len() > MAX_POSTINGS {
            return Err(TransactionError::TooManyPostings(postings.len()));
        }

        self.postings = postings;
        Ok(self)
    }

    /// Returns a list of postings.
    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }
}

/// An error returned when building [`Transaction`] fails.
#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("transaction cannot have repeating tags: {0}")]
    RepeatingTags(TagName),
    #[error("transaction has too many postings: expected {0} <= {max}", max = MAX_POSTINGS)]
    TooManyPostings(usize),
    #[error("transaction has too many tags: expected {0} <= {max}", max = MAX_TAGS)]
    TooManyTags(usize),
}

#[derive(serde::Deserialize)]
struct TransactionData {
    id: String,
    numeric_id: u32,
    date: chrono::NaiveDate,
    description: Description,
    tags: Vec<TagName>,
    postings: Vec<Posting>,
    comment: Option<Comment>,
}

impl<'de> serde::Deserialize<'de> for Transaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data = TransactionData::deserialize(deserializer)?;

        let mut transaction = Transaction {
            id: data.id,
            numeric_id: data.numeric_id,
            date: data.date,
            description: data.description,
            tags: vec![],
            postings: vec![],
            comment: data.comment,
        };

        transaction
            .set_tags(data.tags)
            .map_err(serde::de::Error::custom)?;
        transaction
            .set_postings(data.postings)
            .map_err(serde::de::Error::custom)?;

        Ok(transaction)
    }
}

/// A single leg of [`Transaction`].
#[derive(Clone, Debug, serde::Serialize)]
pub struct Posting {
    account: AccountName,
    amount: Amount,
    cost: Option<Cost>,
    exchange_amount: Option<Amount>,
    foreign_amount: Option<Amount>,
}

impl Posting {
    /// Creates new posting of specified amount on a given account.
    pub fn new(account: AccountName, amount: Amount) -> Posting {
        Posting {
            account,
            amount,
            cost: None,
            exchange_amount: None,
            foreign_amount: None,
        }
    }

    /// Update account of this posting.
    pub fn with_account(&mut self, account: AccountName) -> &mut Posting {
        self.account = account;
        self
    }

    /// Update amount of this posting.
    pub fn with_amount(&mut self, amount: Amount) -> &mut Posting {
        self.amount = amount;
        self
    }

    /// Adds cost basis to this posting.
    pub fn with_cost(&mut self, cost: Cost) -> &mut Posting {
        self.cost = Some(cost);
        self
    }

    /// Removes cost basis on this posting.
    pub fn without_cost(&mut self) -> &mut Posting {
        self.cost = None;
        self
    }

    /// Adds unit conversion amount to this posting.
    pub fn with_conversion_amount(&mut self, conversion_amount: ConversionAmount) -> &mut Posting {
        match conversion_amount {
            ConversionAmount::Exchange(amount) => {
                self.exchange_amount = Some(amount);
                self.foreign_amount = None;
            }
            ConversionAmount::Foreign(amount) => {
                self.exchange_amount = None;
                self.foreign_amount = Some(amount);
            }
        }
        self
    }

    /// Removes unit conversion from this posting.
    pub fn without_conversion_amount(&mut self) -> &mut Posting {
        self.exchange_amount = None;
        self.foreign_amount = None;
        self
    }

    /// Returns account of this posting.
    pub fn account(&self) -> &AccountName {
        &self.account
    }

    /// Returns amount of this posting.
    pub fn amount(&self) -> &Amount {
        &self.amount
    }

    /// Returns cost basis of this posting.
    pub fn cost(&self) -> Option<&Cost> {
        self.cost.as_ref()
    }

    /// Returns unit conversion amount of this posting.
    pub fn conversion_amount(&self) -> Option<ConversionAmount> {
        match (self.exchange_amount.as_ref(), self.foreign_amount.as_ref()) {
            (Some(exchange), None) => Some(ConversionAmount::Exchange(exchange.clone())),
            (None, Some(foreign)) => Some(ConversionAmount::Foreign(foreign.clone())),
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("both exchange and foreign amount cannot be set"),
        }
    }
}

#[derive(serde::Deserialize)]
struct PostingData {
    account: AccountName,
    amount: Amount,
    cost: Option<Cost>,
    exchange_amount: Option<Amount>,
    foreign_amount: Option<Amount>,
}

impl<'de> serde::Deserialize<'de> for Posting {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data = PostingData::deserialize(deserializer)?;

        if data.exchange_amount.is_some() && data.foreign_amount.is_some() {
            return Err(serde::de::Error::custom(
                "posting cannot have both an exchange amount and a foreign amount",
            ));
        }

        Ok(Posting {
            account: data.account,
            amount: data.amount,
            cost: data.cost,
            exchange_amount: data.exchange_amount,
            foreign_amount: data.foreign_amount,
        })
    }
}

/// Amount used for conversion between units.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ConversionAmount {
    /// The amount per single unit, e.g. with USD amount, this could be 0.9 EUR.
    Exchange(Amount),
    /// The total amount, e.g. with the amount of 100 USD, that could be 90 EUR.
    Foreign(Amount),
}

/// Amount - a number and a unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Amount {
    pub number: rust_decimal::Decimal,
    pub unit: UnitName,
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.number, self.unit)
    }
}

/// Cost basis definition.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Cost {
    pub date: chrono::NaiveDate,
    pub number: rust_decimal::Decimal,
    pub unit: UnitName,
}

impl std::fmt::Display for Cost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} @ {}", self.number, self.unit, self.date)
    }
}
