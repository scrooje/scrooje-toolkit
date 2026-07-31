//! Representation of JSON-lines entries format.
//!
//! This format can encode a complete state of the application, and is used for
//! both data uploads and offline backups.
//!
//! Note, that there is no structural integrity validation of any kind in this
//! module, it merely exposes the types to represent the entries.
use crate::types::{
    AccountName, Comment, Description, TagName, UnitName, UnitNameError, has_consecutive_bytes,
};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct EncodedEntry {
    #[serde(default)]
    t: u8,
    d: serde_json::Value,
}

const ACCOUNT: u8 = 1;
const TRANSACTION: u8 = 2;
const PRICE: u8 = 3;
const UNIT: u8 = 4;
const PREFERENCES: u8 = 5;
const TAG: u8 = 6;
const TRACKER: u8 = 7;
const CORPORATE_ACTION: u8 = 8;

/// A single entry as it appears in JSON-lines file.
#[derive(Clone, Debug)]
pub enum Entry {
    /// Unknown entry, used for compatibility.
    Unknown(u8, serde_json::Value),
    /// Account entry.
    Account(AccountEntry),
    /// Transaction entry.
    Transaction(TransactionEntry),
    /// Price entry.
    Price(PriceEntry),
    /// Unit entry.
    Unit(UnitEntry),
    /// Preferences entry.
    Preferences(PreferencesEntry),
    /// Tag entry.
    Tag(TagEntry),
    /// Tracker entry.
    Tracker(TrackerEntry),
    /// Corporate action entry.
    CorporateAction(CorporateActionEntry),
}

impl Entry {
    /// Deserializes `Entry` from bytes containing JSON text.
    ///
    /// # Errors
    ///
    /// Returns error if bytes are not of valid JSON or the format is malformed.
    pub fn from_json(bytes: &[u8]) -> Result<Entry, serde_json::Error> {
        let entry: EncodedEntry = serde_json::from_slice(bytes)?;
        match entry.t {
            ACCOUNT => Ok(Self::Account(serde_json::from_value(entry.d)?)),
            TRANSACTION => Ok(Self::Transaction(serde_json::from_value(entry.d)?)),
            PRICE => Ok(Self::Price(serde_json::from_value(entry.d)?)),
            UNIT => Ok(Self::Unit(serde_json::from_value(entry.d)?)),
            PREFERENCES => Ok(Self::Preferences(serde_json::from_value(entry.d)?)),
            TAG => Ok(Self::Tag(serde_json::from_value(entry.d)?)),
            TRACKER => Ok(Self::Tracker(serde_json::from_value(entry.d)?)),
            CORPORATE_ACTION => Ok(Self::CorporateAction(serde_json::from_value(entry.d)?)),
            t => Ok(Self::Unknown(t, entry.d)),
        }
    }

    /// Serializes `Entry` into bytes containing JSON text.
    ///
    /// # Errors
    ///
    /// In unlikely case, when the serialization path decides to fail, which
    /// should never happen.
    pub fn json(&self) -> Result<String, serde_json::Error> {
        let encoded = match self {
            Self::Unknown(t, value) => EncodedEntry {
                t: *t,
                d: value.clone(),
            },
            Self::Account(entry) => EncodedEntry {
                t: ACCOUNT,
                d: serde_json::to_value(entry)?,
            },
            Self::Transaction(entry) => EncodedEntry {
                t: TRANSACTION,
                d: serde_json::to_value(entry)?,
            },
            Self::Price(entry) => EncodedEntry {
                t: PRICE,
                d: serde_json::to_value(entry)?,
            },
            Self::Unit(entry) => EncodedEntry {
                t: UNIT,
                d: serde_json::to_value(entry)?,
            },
            Self::Preferences(entry) => EncodedEntry {
                t: PREFERENCES,
                d: serde_json::to_value(entry)?,
            },
            Self::Tag(entry) => EncodedEntry {
                t: TAG,
                d: serde_json::to_value(entry)?,
            },
            Self::Tracker(entry) => EncodedEntry {
                t: TRACKER,
                d: serde_json::to_value(entry)?,
            },
            Self::CorporateAction(entry) => EncodedEntry {
                t: CORPORATE_ACTION,
                d: serde_json::to_value(entry)?,
            },
        };

        serde_json::to_string(&encoded)
    }
}

/// Account entity. [`Posting`] refers to these by ID.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AccountEntry {
    pub id: String,
    #[serde(rename = "na")]
    pub name: AccountName,
    #[serde(rename = "do")]
    pub date_opened: chrono::NaiveDate,
    #[serde(rename = "un")]
    pub units: Vec<UnitName>,
    #[serde(rename = "dc", skip_serializing_if = "Option::is_none")]
    pub date_closed: Option<chrono::NaiveDate>,
    #[serde(rename = "ty", skip_serializing_if = "Option::is_none")]
    pub ty: Option<AccountType>,
    #[serde(rename = "tc", skip_serializing_if = "Option::is_none")]
    pub trading_config: Option<TradingAccountConfig>,
    #[serde(rename = "ic", skip_serializing_if = "Option::is_none")]
    pub icon: Option<AccountIcon>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountType {
    Trading,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TradingAccountConfig {
    #[serde(rename = "ru")]
    pub rule: TradingAccountRule,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradingAccountRule {
    Fifo,
    Lifo,
    Acb,
}

/// An icon on the account for visual display, nothing more.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AccountIcon {
    pub id: u64,
    #[serde(rename = "co", skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

/// Transaction entity. This makes the bulk of the data.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TransactionEntry {
    pub id: String,
    #[serde(rename = "ni")]
    pub numeric_id: u32,
    #[serde(rename = "dt")]
    pub date: chrono::NaiveDate,
    #[serde(rename = "de")]
    pub description: Description,
    #[serde(rename = "po")]
    pub postings: Vec<Posting>,
    #[serde(rename = "ta", skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(rename = "co", skip_serializing_if = "Option::is_none")]
    pub comment: Option<Comment>,
}

/// The legs of the transaction capturing movements of amounts between accounts.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Posting {
    #[serde(rename = "ac")]
    pub account: String,
    #[serde(rename = "am")]
    pub amount: Amount,
    #[serde(rename = "rp", skip_serializing_if = "Option::is_none")]
    pub relprice: Option<Amount>,
    #[serde(rename = "ap", skip_serializing_if = "Option::is_none")]
    pub absprice: Option<Amount>,
    #[serde(rename = "co", skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
}

/// Amount consisting of a number and a unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Amount {
    #[serde(rename = "n")]
    pub number: rust_decimal::Decimal,
    #[serde(rename = "u")]
    pub unit: UnitName,
}

/// Cost basis definition.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Cost {
    #[serde(rename = "d")]
    pub date: chrono::NaiveDate,
    #[serde(rename = "n")]
    pub number: rust_decimal::Decimal,
    #[serde(rename = "u")]
    pub unit: UnitName,
}

/// Price entity. This is user-defined market prices entry.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PriceEntry {
    #[serde(rename = "d")]
    pub date: chrono::NaiveDate,
    #[serde(rename = "u")]
    pub unit: UnitName,
    #[serde(rename = "a")]
    pub amount: Amount,
}

/// Unit entity - a configuration of a single unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct UnitEntry {
    #[serde(rename = "na")]
    pub name: UnitName,
    #[serde(rename = "sy", skip_serializing_if = "Option::is_none")]
    pub symbol: Option<Symbol>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ty: Option<String>,
    #[serde(rename = "dp", skip_serializing_if = "Option::is_none")]
    pub decimal_places: Option<u8>,
    #[serde(rename = "df", skip_serializing_if = "Option::is_none")]
    pub disable_fetching: Option<bool>,
}

/// Symbol definition for market data.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Symbol {
    Ticker(Ticker),
    Unit(UnitName),
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ticker(ticker) => write!(f, "{ticker}"),
            Self::Unit(unit) => write!(f, "{unit}"),
        }
    }
}

impl std::str::FromStr for Symbol {
    type Err = SymbolError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("ticker:") {
            Ok(Symbol::Ticker(s.parse()?))
        } else {
            Ok(Symbol::Unit(s.parse()?))
        }
    }
}

serde_impls!(Symbol, "a valid symbol");

/// An error returned when parsing [`Symbol`] from string fails.
#[derive(Debug, thiserror::Error)]
pub enum SymbolError {
    #[error("bad symbol ticker: {0}")]
    BadTicker(#[from] TickerError),
    #[error("bad symbol unit: {0}")]
    BadUnit(#[from] UnitNameError),
}

/// Ticker definition for market data.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Ticker {
    name: String,
}

impl Ticker {
    const MAX_LENGTH: usize = 20;
    const PREFIX: &str = "ticker";
}

impl std::fmt::Display for Ticker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ticker:{}", self.name)
    }
}

impl std::str::FromStr for Ticker {
    type Err = TickerError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (prefix, ticker) = s.split_once(":").unwrap_or_default();
        if prefix != Self::PREFIX {
            return Err(TickerError::BadPrefix(s.to_owned()));
        }

        let bytes = ticker.as_bytes();

        if bytes.is_empty() {
            return Err(TickerError::Empty);
        }

        if bytes.len() > Self::MAX_LENGTH {
            return Err(TickerError::TooLong(bytes.len()));
        }

        let first = bytes[0];
        if !first.is_ascii_uppercase() && !first.is_ascii_digit() {
            return Err(TickerError::BadFirstCharacter(s.to_owned()));
        }

        let last = bytes[bytes.len() - 1];
        if !last.is_ascii_uppercase() && !last.is_ascii_digit() {
            return Err(TickerError::BadLastCharacter(s.to_owned()));
        }

        if !bytes
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'.')
        {
            return Err(TickerError::BadCharacter(s.to_owned()));
        }

        if has_consecutive_bytes(bytes, b'.') {
            return Err(TickerError::ConsecutiveDots(s.to_owned()));
        }

        Ok(Ticker {
            name: ticker.to_owned(),
        })
    }
}

/// An error returned when parsing [`Ticker`] from string fails.
#[derive(Debug, thiserror::Error)]
pub enum TickerError {
    #[error("ticker name must contain uppercase and dots: {0}")]
    BadCharacter(String),
    #[error("ticker name must start with an uppercase or digit: {0}")]
    BadFirstCharacter(String),
    #[error("ticker name must end with an uppercase or digit: {0}")]
    BadLastCharacter(String),
    #[error("bad ticker prefix: {0}")]
    BadPrefix(String),
    #[error("ticker name cannot have consecutive dots: {0}")]
    ConsecutiveDots(String),
    #[error("ticker name cannot be empty")]
    Empty,
    #[error("ticker name too long: expected {0} <= {max}", max = Ticker::MAX_LENGTH)]
    TooLong(usize),
}

/// Preferences entity describing app-wide preferences.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PreferencesEntry {
    #[serde(rename = "ve", skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    #[serde(rename = "ou")]
    pub operating_units: Vec<UnitName>,
    #[serde(rename = "au", skip_serializing_if = "Option::is_none")]
    pub auto_upload: Option<bool>,
}

/// Tag entity. [`TransactionEntry`] refers to these by ID.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TagEntry {
    pub id: String,
    #[serde(rename = "na")]
    pub name: TagName,
    #[serde(rename = "co", skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

/// A set of pre-defined colors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Color {
    Red,
    Pink,
    Violet,
    Blue,
    Cyan,
    Teal,
    Yellow,
    Orange,
}

/// Tracker entity.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "mo")]
pub enum TrackerEntry {
    Periodic(PeriodicTrackerEntry),
    Goal(GoalTrackerEntry),
}

/// Periodic tracker entry.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PeriodicTrackerEntry {
    pub id: String,
    #[serde(rename = "na")]
    pub name: String,
    #[serde(rename = "st")]
    pub start: chrono::NaiveDate,
    #[serde(rename = "pr")]
    pub period: TrackerPeriod,
    #[serde(rename = "sr")]
    pub strategy: TrackerStrategy,
    #[serde(rename = "sc")]
    pub score: TrackerScore,
    #[serde(rename = "cf")]
    pub configs: Vec<TrackerConfig>,
}

/// Goal tracker entry.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GoalTrackerEntry {
    pub id: String,
    #[serde(rename = "na")]
    pub name: String,
    #[serde(rename = "st")]
    pub start: chrono::NaiveDate,
    #[serde(rename = "ag")]
    pub aggregation: TrackerAggregation,
    #[serde(rename = "di", skip_serializing_if = "Option::is_none")]
    pub direction: Option<TrackerDirection>,
    #[serde(rename = "ex", skip_serializing_if = "Option::is_none")]
    pub expiry: Option<chrono::NaiveDate>,
    #[serde(rename = "sc")]
    pub score: TrackerScore,
    #[serde(rename = "cf")]
    pub configs: Vec<TrackerConfig>,
}

/// A tracker period.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "ty")]
pub enum TrackerPeriod {
    Calendar {
        #[serde(rename = "pd")]
        period: CalendarPeriod,
    },
    DateRange {
        #[serde(rename = "da")]
        days: u32,
    },
}

/// Calendar period interval.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CalendarPeriod {
    Month,
    Quarter,
    HalfYear,
    Year,
}

/// Tracker strategy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackerStrategy {
    Reset,
    RolloverFavorable,
    RolloverAll,
}

/// Tracker score.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackerScore {
    Positive,
    Negative,
}

/// Tracker aggregation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackerAggregation {
    Balance,
    Flow,
}

/// Tracker direction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackerDirection {
    Increase,
    Decrease,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TrackerConfig {
    #[serde(rename = "ef")]
    pub effective_from: chrono::NaiveDate,
    #[serde(rename = "sl")]
    pub selectors: Vec<AccountSelector>,
    #[serde(rename = "am")]
    pub amount: Amount,
    #[serde(rename = "fi", skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "ty")]
pub enum AccountSelector {
    Explicit {
        id: String,
    },
    Prefix {
        #[serde(rename = "pr")]
        prefix: String,
    },
}

/// Corporate action entity.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "ty")]
pub enum CorporateActionEntry {
    Split(SplitActionEntry),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SplitActionEntry {
    pub id: String,
    #[serde(rename = "dt")]
    pub date: chrono::NaiveDate,
    #[serde(rename = "un")]
    pub unit: UnitName,
    #[serde(rename = "nr")]
    pub numerator: u32,
    #[serde(rename = "dr")]
    pub denominator: u32,
}

// Warning! Tests were generated by LLM.
#[cfg(test)]
mod tests {
    use super::*;

    const DEMO_JSON_LINES: &str = include_str!("../testdata/demo.backup");

    #[test]
    fn entries_from_json() {
        for line in DEMO_JSON_LINES.split('\n') {
            if line.is_empty() {
                continue;
            }
            Entry::from_json(line.as_bytes()).expect("entry should be parsable");
        }
    }

    #[test]
    fn ticker_valid_cases() {
        // Basic tickers without exchange
        assert!("ticker:AAPL".parse::<Ticker>().is_ok());
        assert!("ticker:MSFT".parse::<Ticker>().is_ok());
        assert!("ticker:GOOGL".parse::<Ticker>().is_ok());

        // Tickers with numbers
        assert!("ticker:B1".parse::<Ticker>().is_ok());
        assert!("ticker:3M".parse::<Ticker>().is_ok());

        // Complex tickers with dots
        assert!("ticker:BRK.A".parse::<Ticker>().is_ok());
        assert!("ticker:BERKSHIRE.A".parse::<Ticker>().is_ok());
        assert!("ticker:A.B.C".parse::<Ticker>().is_ok());

        // Single character ticker
        assert!("ticker:A".parse::<Ticker>().is_ok());
        assert!("ticker:X".parse::<Ticker>().is_ok());

        // Boundary case - exactly max length (20 chars)
        let max_length_ticker = format!("ticker:{}", "A".repeat(20));
        assert!(max_length_ticker.parse::<Ticker>().is_ok());
    }

    #[test]
    fn ticker_invalid_bad_prefix() {
        let err = "stock:AAPL".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadPrefix(_)));
        assert_eq!(err.to_string(), "bad ticker prefix: stock:AAPL");

        let err = "AAPL".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadPrefix(_)));

        let err = "symbol:MSFT".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadPrefix(_)));
    }

    #[test]
    fn ticker_invalid_empty() {
        let err = "ticker:".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::Empty));
        assert_eq!(err.to_string(), "ticker name cannot be empty");
    }

    #[test]
    fn ticker_invalid_too_long() {
        let long_ticker = format!("ticker:{}", "A".repeat(21)); // 21 chars
        let err = long_ticker.parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::TooLong(21)));
        assert_eq!(err.to_string(), "ticker name too long: expected 21 <= 20");
    }

    #[test]
    fn ticker_invalid_first_character() {
        // Lowercase first
        let err = "ticker:aapl".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadFirstCharacter(_)));
        assert_eq!(
            err.to_string(),
            "ticker name must start with an uppercase or digit: ticker:aapl"
        );

        // Special character first
        let err = "ticker:.AAPL".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadFirstCharacter(_)));

        let err = "ticker:-MSFT".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadFirstCharacter(_)));
    }

    #[test]
    fn ticker_invalid_last_character() {
        let err = "ticker:AAPL.".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadLastCharacter(_)));
        assert_eq!(
            err.to_string(),
            "ticker name must end with an uppercase or digit: ticker:AAPL."
        );

        let err = "ticker:MSFT-".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadLastCharacter(_)));
    }

    #[test]
    fn ticker_invalid_characters() {
        // Lowercase in middle
        let err = "ticker:AApL".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadCharacter(_)));
        assert_eq!(
            err.to_string(),
            "ticker name must contain uppercase and dots: ticker:AApL"
        );

        // Invalid special characters
        let err = "ticker:AA-PL".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadCharacter(_)));

        let err = "ticker:AA_PL".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadCharacter(_)));

        let err = "ticker:AA@PL".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::BadCharacter(_)));

        let err = "ticker:AA PL".parse::<Ticker>().unwrap_err(); // space
        assert!(matches!(err, TickerError::BadCharacter(_)));
    }

    #[test]
    fn ticker_invalid_consecutive_dots() {
        let err = "ticker:BRK..A".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::ConsecutiveDots(_)));
        assert_eq!(
            err.to_string(),
            "ticker name cannot have consecutive dots: ticker:BRK..A"
        );

        let err = "ticker:A...B".parse::<Ticker>().unwrap_err();
        assert!(matches!(err, TickerError::ConsecutiveDots(_)));
    }

    #[test]
    fn ticker_display_format() {
        let ticker_no_exchange: Ticker = "ticker:AAPL".parse().unwrap();
        assert_eq!(ticker_no_exchange.to_string(), "ticker:AAPL");
    }
}
