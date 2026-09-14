//! Shared type definitions.
/// Maximum number of postings per transaction.
pub const MAX_POSTINGS: usize = 20;
/// Maximum number of tags per transaction.
pub const MAX_TAGS: usize = 8;

/// Account name, consisting of two or more labels separated by ':'.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AccountName(Vec<AccountLabel>);

impl AccountName {
    const LABEL_SEPARATOR: &str = ":";
    const MAX_LABELS: usize = 8;
    const MAX_LENGTH: usize = 80;

    /// Checks whether this account name is a parent of another.
    ///
    /// # Examples
    ///
    /// ```
    /// use scrooje_core::types::AccountName;
    ///
    /// let a: AccountName = "Assets:Cash".parse().unwrap();
    /// let b: AccountName = "Assets:Cash:USD".parse().unwrap();
    /// let c: AccountName = "Assets:Bank".parse().unwrap();
    ///
    /// assert!(a.parent_of(&b));
    /// assert!(!b.parent_of(&a));
    /// assert!(!c.parent_of(&b));
    /// ```
    pub fn parent_of(&self, other: &AccountName) -> bool {
        self.0.len() < other.0.len()
            && self
                .0
                .iter()
                .zip(other.0.iter())
                .all(|(left, right)| left == right)
    }

    /// Returns true if this is an Assets account.
    pub fn is_assets(&self) -> bool {
        self.0
            .first()
            .is_some_and(|label| label.0 == AccountLabel::ASSETS)
    }

    /// Returns true if this is an Equity account.
    pub fn is_equity(&self) -> bool {
        self.0
            .first()
            .is_some_and(|label| label.0 == AccountLabel::EQUITY)
    }

    /// Returns true if this is an Expenses account.
    pub fn is_expenses(&self) -> bool {
        self.0
            .first()
            .is_some_and(|label| label.0 == AccountLabel::EXPENSES)
    }

    /// Returns true if this is an Income account.
    pub fn is_income(&self) -> bool {
        self.0
            .first()
            .is_some_and(|label| label.0 == AccountLabel::INCOME)
    }

    /// Returns true if this is a Liabilities account.
    pub fn is_liabilities(&self) -> bool {
        self.0
            .first()
            .is_some_and(|label| label.0 == AccountLabel::LIABILITIES)
    }
}

impl std::fmt::Display for AccountName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let labels: Vec<_> = self
            .0
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        f.write_str(&labels.join(Self::LABEL_SEPARATOR))
    }
}

impl std::str::FromStr for AccountName {
    type Err = AccountNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(AccountNameError::Empty);
        }

        if s.len() > Self::MAX_LENGTH {
            return Err(AccountNameError::TooLong(s.len()));
        }

        let mut labels = Vec::new();
        for label in s.split(Self::LABEL_SEPARATOR) {
            let label: AccountLabel = label.parse()?;
            labels.push(label);
        }

        if labels.len() <= 1 {
            return Err(AccountNameError::TooFewLabels);
        }

        if !labels[0].is_root() {
            return Err(AccountNameError::BadRoot);
        }

        if labels.len() > Self::MAX_LABELS {
            return Err(AccountNameError::TooManyLabels(labels.len()));
        }

        Ok(AccountName(labels))
    }
}

serde_impls!(AccountName, "a valid account name");

/// An error returned when parsing [`AccountName`] from string fails.
#[derive(Debug, thiserror::Error)]
pub enum AccountNameError {
    #[error("bad account label: {0}")]
    BadLabel(#[from] AccountLabelError),
    #[error(
        "account name must start with {}, {}, {}, {}, {}",
        AccountLabel::ASSETS,
        AccountLabel::EQUITY,
        AccountLabel::EXPENSES,
        AccountLabel::INCOME,
        AccountLabel::LIABILITIES
    )]
    BadRoot,
    #[error("account name cannot be empty")]
    Empty,
    #[error("account name must have more than 1 label")]
    TooFewLabels,
    #[error("account name too long: expected {0} <= {max}", max = AccountName::MAX_LENGTH)]
    TooLong(usize),
    #[error("too many account name labels, expected {0} <= {max}", max = AccountName::MAX_LABELS)]
    TooManyLabels(usize),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AccountLabel(String);

impl AccountLabel {
    pub const ASSETS: &str = "Assets";
    pub const EQUITY: &str = "Equity";
    pub const EXPENSES: &str = "Expenses";
    pub const INCOME: &str = "Income";
    pub const LIABILITIES: &str = "Liabilities";

    pub fn is_root(&self) -> bool {
        self.0 == Self::ASSETS
            || self.0 == Self::EQUITY
            || self.0 == Self::EXPENSES
            || self.0 == Self::INCOME
            || self.0 == Self::LIABILITIES
    }
}

impl std::fmt::Display for AccountLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for AccountLabel {
    type Err = AccountLabelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();

        if bytes.is_empty() {
            return Err(AccountLabelError::Empty);
        }

        let first = bytes[0];
        if !first.is_ascii_uppercase() && !first.is_ascii_digit() {
            return Err(AccountLabelError::BadFirstCharacter(s.to_owned()));
        }

        let last = bytes[bytes.len() - 1];
        if !last.is_ascii_alphanumeric() {
            return Err(AccountLabelError::BadLastCharacter(s.to_owned()));
        }

        if !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            return Err(AccountLabelError::BadCharacter(s.to_owned()));
        }

        if has_consecutive_bytes(bytes, b'-') {
            return Err(AccountLabelError::ConsecutiveHyphens(s.to_owned()));
        }

        Ok(AccountLabel(s.to_owned()))
    }
}

serde_impls!(AccountLabel, "a valid account name label");

/// An error return when parsing an account name label from string fails.
#[derive(Debug, thiserror::Error)]
pub enum AccountLabelError {
    #[error("account label must contain alphanumeric and hyphens: {0}")]
    BadCharacter(String),
    #[error("account label must start with uppercase or digit: {0}")]
    BadFirstCharacter(String),
    #[error("account label must end with alphanumeric: {0}")]
    BadLastCharacter(String),
    #[error("account label cannot have consecutive hyphens: {0}")]
    ConsecutiveHyphens(String),
    #[error("account label cannot be empty")]
    Empty,
}

/// A comment of a transaction. Can be an arbitrary non-empty string.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Comment(String);

impl Comment {
    const MAX_LENGTH: usize = 600;
}

impl std::fmt::Display for Comment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for Comment {
    type Err = CommentError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(CommentError::Empty);
        }

        if s.len() > Self::MAX_LENGTH {
            return Err(CommentError::TooLong(s.len()));
        }

        Ok(Comment(s.to_owned()))
    }
}

serde_impls!(Comment, "a valid comment");

/// An error returned when parsing [`Comment`] from string fails.
#[derive(Debug, thiserror::Error)]
pub enum CommentError {
    #[error("comment cannot be empty")]
    Empty,
    #[error("comment too long: expected {0} <= {max}", max = Comment::MAX_LENGTH)]
    TooLong(usize),
}

/// A free-form description of a transaction. Cannot contain new lines.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Description(String);

impl Description {
    const MAX_LENGTH: usize = 120;
    const PAYEE_SEPARATOR: &str = " @ ";

    pub fn split(&self) -> (&str, &str) {
        self.0
            .split_once(Self::PAYEE_SEPARATOR)
            .unwrap_or((&self.0, ""))
    }
}

impl std::fmt::Display for Description {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for Description {
    type Err = DescriptionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(DescriptionError::Empty);
        }

        if s.len() > Self::MAX_LENGTH {
            return Err(DescriptionError::TooLong(s.len()));
        }

        if s.contains('\r') || s.contains('\n') {
            return Err(DescriptionError::MultipleLines);
        }

        Ok(Description(s.to_owned()))
    }
}

serde_impls!(Description, "a valid description");

/// An error returned when parsing [`Description`] from string fails.
#[derive(Debug, thiserror::Error)]
pub enum DescriptionError {
    #[error("description cannot be empty")]
    Empty,
    #[error("description cannot contain multiple lines")]
    MultipleLines,
    #[error("description too long: expected {0} <= {max}", max = Description::MAX_LENGTH)]
    TooLong(usize),
}

/// Tag name. Can contain only lowercase ASCII separated by '-'.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TagName(String);

impl TagName {
    const MAX_LENGTH: usize = 60;
}

impl std::fmt::Display for TagName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for TagName {
    type Err = TagNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();

        if bytes.is_empty() {
            return Err(TagNameError::Empty);
        }

        if bytes.len() > Self::MAX_LENGTH {
            return Err(TagNameError::TooLong(bytes.len()));
        }

        let first = bytes[0];
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return Err(TagNameError::BadFirstCharacter(s.to_owned()));
        }

        let last = bytes[bytes.len() - 1];
        if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
            return Err(TagNameError::BadLastCharacter(s.to_owned()));
        }

        if !bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        {
            return Err(TagNameError::BadCharacter(s.to_owned()));
        }

        if has_consecutive_bytes(bytes, b'-') {
            return Err(TagNameError::ConsecutiveHyphens(s.to_owned()));
        }

        Ok(TagName(s.to_string()))
    }
}

serde_impls!(TagName, "a valid tag name");

/// An error returned when parsing [`TagName`] from string fails.
#[derive(Debug, thiserror::Error)]
pub enum TagNameError {
    #[error("tag name must contain lowercase and hyphens: {0}")]
    BadCharacter(String),
    #[error("tag name must start with lowercase or digit: {0}")]
    BadFirstCharacter(String),
    #[error("tag name must end with lowercase or digit: {0}")]
    BadLastCharacter(String),
    #[error("tag name cannot have consecutive hyphens: {0}")]
    ConsecutiveHyphens(String),
    #[error("tag name cannot be empty")]
    Empty,
    #[error("tag name too long: expected {0} <= {max}", max = TagName::MAX_LENGTH)]
    TooLong(usize),
}

/// Unit name. Can contain only uppercase ASCII and dots.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UnitName(String);

impl UnitName {
    const MAX_LENGTH: usize = 16;
    const MIN_LENGTH: usize = 2;
}

impl std::fmt::Display for UnitName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

serde_impls!(UnitName, "a valid unit name");

impl std::str::FromStr for UnitName {
    type Err = UnitNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();

        if bytes.is_empty() {
            return Err(UnitNameError::Empty);
        }

        if bytes.len() < Self::MIN_LENGTH {
            return Err(UnitNameError::TooShort(bytes.len()));
        }

        if bytes.len() > Self::MAX_LENGTH {
            return Err(UnitNameError::TooLong(bytes.len()));
        }

        let first = bytes[0];
        if !first.is_ascii_uppercase() {
            return Err(UnitNameError::BadFirstCharacter(s.to_owned()));
        }

        let last = bytes[bytes.len() - 1];
        if last == b'.' {
            return Err(UnitNameError::BadLastCharacter(s.to_owned()));
        }

        if !bytes
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'.')
        {
            return Err(UnitNameError::BadCharacter(s.to_owned()));
        }

        if has_consecutive_bytes(bytes, b'.') {
            return Err(UnitNameError::ConsecutiveDots(s.to_owned()));
        }

        Ok(UnitName(s.to_owned()))
    }
}

/// An error returned when parsing [`UnitName`] from string fails.
#[derive(Debug, thiserror::Error)]
pub enum UnitNameError {
    #[error("unit name must contain uppercase and dots: {0}")]
    BadCharacter(String),
    #[error("unit name must start with an uppercase: {0}")]
    BadFirstCharacter(String),
    #[error("unit name cannot end with a dot: {0}")]
    BadLastCharacter(String),
    #[error("unit name cannot have consecutive dots: {0}")]
    ConsecutiveDots(String),
    #[error("unit name cannot be empty")]
    Empty,
    #[error("unit name too long: expected {0} <= {max}", max = UnitName::MAX_LENGTH)]
    TooLong(usize),
    #[error("unit name too short: expected {0} >= {min}", min = UnitName::MIN_LENGTH)]
    TooShort(usize),
}

pub(crate) fn has_consecutive_bytes(bytes: &[u8], matched: u8) -> bool {
    bytes
        .windows(2)
        .any(|byte| byte[0] == matched && byte[0] == byte[1])
}

// Warning! Tests were generated by LLM.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_name_valid_cases() {
        // Basic two-level accounts
        assert!("Assets:Cash".parse::<AccountName>().is_ok());
        assert!("Liabilities:CreditCard".parse::<AccountName>().is_ok());
        assert!("Expenses:Groceries".parse::<AccountName>().is_ok());
        assert!("Income:Salary".parse::<AccountName>().is_ok());
        assert!("Equity:Opening-Balances".parse::<AccountName>().is_ok());

        // Multi-level accounts
        assert!("Assets:Bank:Checking".parse::<AccountName>().is_ok());
        assert!(
            "Assets:Investment:401K:Vanguard"
                .parse::<AccountName>()
                .is_ok()
        );
        assert!(
            "Expenses:Transportation:Car:Insurance"
                .parse::<AccountName>()
                .is_ok()
        );

        // With numbers
        assert!("Assets:Account123".parse::<AccountName>().is_ok());
        assert!(
            "Assets:Fund401K:Portfolio2023"
                .parse::<AccountName>()
                .is_ok()
        );

        // Maximum levels (8 labels)
        assert!("Assets:A:B:C:D:E:F:G".parse::<AccountName>().is_ok());

        // Close to maximum length (80 chars total)
        let long_name =
            "Assets:VeryLongAccountNameThatIsStillValidButApproachesTheMaximumAllowedLength1";
        assert_eq!(long_name.len(), 79); // Just under limit
        assert!(long_name.parse::<AccountName>().is_ok());
    }

    #[test]
    fn account_name_invalid_empty() {
        let err = "".parse::<AccountName>().unwrap_err();
        assert!(matches!(err, AccountNameError::Empty));
        assert_eq!(err.to_string(), "account name cannot be empty");
    }

    #[test]
    fn account_name_invalid_too_long() {
        let long_name = "A".repeat(81); // 81 chars
        let err = long_name.parse::<AccountName>().unwrap_err();
        assert!(matches!(err, AccountNameError::TooLong(81)));
        assert_eq!(err.to_string(), "account name too long: expected 81 <= 80");
    }

    #[test]
    fn account_name_invalid_too_few_labels() {
        // Single label
        let err = "Assets".parse::<AccountName>().unwrap_err();
        assert!(matches!(err, AccountNameError::TooFewLabels));
        assert_eq!(err.to_string(), "account name must have more than 1 label");

        // Empty after split (just separator)
        let err = ":".parse::<AccountName>().unwrap_err();
        assert!(matches!(err, AccountNameError::BadLabel(_))); // Empty label error comes first
    }

    #[test]
    fn account_name_invalid_too_many_labels() {
        // 9 labels (exceeds MAX_LABELS = 8)
        let many_labels = "Assets:A:B:C:D:E:F:G:H";
        let err = many_labels.parse::<AccountName>().unwrap_err();
        assert!(matches!(err, AccountNameError::TooManyLabels(9)));
        assert_eq!(
            err.to_string(),
            "too many account name labels, expected 9 <= 8"
        );
    }

    #[test]
    fn account_name_invalid_bad_root() {
        let err = "Cash:Assets".parse::<AccountName>().unwrap_err();
        assert!(matches!(err, AccountNameError::BadRoot));
        assert!(err.to_string().contains("account name must start with"));

        let err = "InvalidRoot:Cash".parse::<AccountName>().unwrap_err();
        assert!(matches!(err, AccountNameError::BadRoot));
    }

    #[test]
    fn account_name_invalid_bad_label() {
        // Invalid label gets propagated
        let err = "Assets:Invalid-label-".parse::<AccountName>().unwrap_err();
        assert!(matches!(err, AccountNameError::BadLabel(_)));

        // The specific AccountLabelError should be wrapped
        if let AccountNameError::BadLabel(label_err) = err {
            assert!(matches!(label_err, AccountLabelError::BadLastCharacter(_)));
        }
    }

    #[test]
    fn account_name_display_format() {
        let account_name: AccountName = "Assets:Bank:Checking".parse().unwrap();
        assert_eq!(account_name.to_string(), "Assets:Bank:Checking");

        let complex_name: AccountName = "Expenses:Transportation:Car:Insurance".parse().unwrap();
        assert_eq!(
            complex_name.to_string(),
            "Expenses:Transportation:Car:Insurance"
        );
    }

    #[test]
    fn account_label_valid_cases() {
        // Root account labels
        assert!("Assets".parse::<AccountLabel>().is_ok());
        assert!("Liabilities".parse::<AccountLabel>().is_ok());
        assert!("Equity".parse::<AccountLabel>().is_ok());
        assert!("Income".parse::<AccountLabel>().is_ok());
        assert!("Expenses".parse::<AccountLabel>().is_ok());

        // Basic uppercase labels
        assert!("Cash".parse::<AccountLabel>().is_ok());
        assert!("Bank".parse::<AccountLabel>().is_ok());
        assert!("Checking".parse::<AccountLabel>().is_ok());

        // CamelCase labels
        assert!("CashAccount".parse::<AccountLabel>().is_ok());
        assert!("ExpensesBar".parse::<AccountLabel>().is_ok());

        // With numbers
        assert!("Account123".parse::<AccountLabel>().is_ok());
        assert!("Fund401K".parse::<AccountLabel>().is_ok());
        assert!("Portfolio2023".parse::<AccountLabel>().is_ok());

        // Starting with numbers
        assert!("401K".parse::<AccountLabel>().is_ok());
        assert!("123Main".parse::<AccountLabel>().is_ok());
        assert!("2023Budget".parse::<AccountLabel>().is_ok());

        // With hyphens
        assert!("Long-Term".parse::<AccountLabel>().is_ok());
        assert!("High-Yield".parse::<AccountLabel>().is_ok());
        assert!("Tax-Deferred".parse::<AccountLabel>().is_ok());

        // Mixed patterns
        assert!("IRA-401K".parse::<AccountLabel>().is_ok());
        assert!("Account-123".parse::<AccountLabel>().is_ok());
        assert!("Fund-2023-Q1".parse::<AccountLabel>().is_ok());

        // Single character
        assert!("A".parse::<AccountLabel>().is_ok());
        assert!("1".parse::<AccountLabel>().is_ok());
    }

    #[test]
    fn account_label_is_root() {
        assert!("Assets".parse::<AccountLabel>().unwrap().is_root());
        assert!("Liabilities".parse::<AccountLabel>().unwrap().is_root());
        assert!("Equity".parse::<AccountLabel>().unwrap().is_root());
        assert!("Income".parse::<AccountLabel>().unwrap().is_root());
        assert!("Expenses".parse::<AccountLabel>().unwrap().is_root());

        // Non-root labels
        assert!(!"Cash".parse::<AccountLabel>().unwrap().is_root());
        assert!(!"Bank".parse::<AccountLabel>().unwrap().is_root());
        assert!(!"Checking".parse::<AccountLabel>().unwrap().is_root());
    }

    #[test]
    fn account_label_invalid_empty() {
        let err = "".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::Empty));
        assert_eq!(err.to_string(), "account label cannot be empty");
    }

    #[test]
    fn account_label_invalid_first_character() {
        // Lowercase first
        let err = "assets".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadFirstCharacter(_)));
        assert_eq!(
            err.to_string(),
            "account label must start with uppercase or digit: assets"
        );

        // Special character first
        let err = "-Cash".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadFirstCharacter(_)));

        let err = "_Assets".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadFirstCharacter(_)));

        let err = ".Bank".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadFirstCharacter(_)));
    }

    #[test]
    fn account_label_invalid_last_character() {
        let err = "Cash-".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadLastCharacter(_)));
        assert_eq!(
            err.to_string(),
            "account label must end with alphanumeric: Cash-"
        );

        let err = "Assets_".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadLastCharacter(_)));

        let err = "Bank.".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadLastCharacter(_)));
    }

    #[test]
    fn account_label_invalid_characters() {
        // Special characters
        let err = "Cash_Account".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadCharacter(_)));

        let err = "Cash.Account".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadCharacter(_)));

        let err = "Cash@Bank".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::BadCharacter(_)));

        let err = "Cash Account".parse::<AccountLabel>().unwrap_err(); // space
        assert!(matches!(err, AccountLabelError::BadCharacter(_)));
    }

    #[test]
    fn account_label_invalid_consecutive_hyphens() {
        let err = "Long--Term".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::ConsecutiveHyphens(_)));
        assert_eq!(
            err.to_string(),
            "account label cannot have consecutive hyphens: Long--Term"
        );

        let err = "High---Yield".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::ConsecutiveHyphens(_)));

        let err = "Account--123".parse::<AccountLabel>().unwrap_err();
        assert!(matches!(err, AccountLabelError::ConsecutiveHyphens(_)));
    }

    #[test]
    fn comment_valid_cases() {
        // Basic descriptions
        assert!("Grocery shopping".parse::<Comment>().is_ok());
        assert!("Coffee at Starbucks".parse::<Comment>().is_ok());
        assert!("Monthly rent payment".parse::<Comment>().is_ok());

        // With numbers and symbols
        assert!("Invoice #12345".parse::<Comment>().is_ok());
        assert!("Transfer to account *1234".parse::<Comment>().is_ok());
        assert!("Payment ref: ABC-123".parse::<Comment>().is_ok());

        // With special characters (but no newlines)
        assert!("Dinner @ restaurant".parse::<Comment>().is_ok());
        assert!("Buy & sell transaction".parse::<Comment>().is_ok());
        assert!("50% discount purchase".parse::<Comment>().is_ok());
        assert!("Email: user@example.com".parse::<Comment>().is_ok());

        // With newlines
        assert!("Line1\r\nLine 2".parse::<Comment>().is_ok());
        assert!("Line1\n\nLine 3".parse::<Comment>().is_ok());

        // Unicode characters
        assert!("Café payment".parse::<Comment>().is_ok());
        assert!("€100 transfer".parse::<Comment>().is_ok());
        assert!("Naïve purchase".parse::<Comment>().is_ok());

        // Single character
        assert!("A".parse::<Comment>().is_ok());

        // Boundary case - exactly max length (600 chars)
        let max_length_comm = "A".repeat(600);
        assert!(max_length_comm.parse::<Comment>().is_ok());
    }

    #[test]
    fn comment_invalid_empty() {
        let err = "".parse::<Comment>().unwrap_err();
        assert!(matches!(err, CommentError::Empty));
        assert_eq!(err.to_string(), "comment cannot be empty");
    }

    #[test]
    fn comment_invalid_too_long() {
        let long_comm = "A".repeat(601); // 601 chars
        let err = long_comm.parse::<Comment>().unwrap_err();
        assert!(matches!(err, CommentError::TooLong(601)));
        assert_eq!(err.to_string(), "comment too long: expected 601 <= 600");
    }

    #[test]
    fn description_valid_cases() {
        // Basic descriptions
        assert!("Grocery shopping".parse::<Description>().is_ok());
        assert!("Coffee at Starbucks".parse::<Description>().is_ok());
        assert!("Monthly rent payment".parse::<Description>().is_ok());

        // With numbers and symbols
        assert!("Invoice #12345".parse::<Description>().is_ok());
        assert!("Transfer to account *1234".parse::<Description>().is_ok());
        assert!("Payment ref: ABC-123".parse::<Description>().is_ok());

        // With special characters (but no newlines)
        assert!("Dinner @ restaurant".parse::<Description>().is_ok());
        assert!("Buy & sell transaction".parse::<Description>().is_ok());
        assert!("50% discount purchase".parse::<Description>().is_ok());
        assert!("Email: user@example.com".parse::<Description>().is_ok());

        // Unicode characters
        assert!("Café payment".parse::<Description>().is_ok());
        assert!("€100 transfer".parse::<Description>().is_ok());
        assert!("Naïve purchase".parse::<Description>().is_ok());

        // Single character
        assert!("A".parse::<Description>().is_ok());

        // Boundary case - exactly max length (120 chars)
        let max_length_desc = "A".repeat(120);
        assert!(max_length_desc.parse::<Description>().is_ok());
    }

    #[test]
    fn description_invalid_empty() {
        let err = "".parse::<Description>().unwrap_err();
        assert!(matches!(err, DescriptionError::Empty));
        assert_eq!(err.to_string(), "description cannot be empty");
    }

    #[test]
    fn description_invalid_too_long() {
        let long_desc = "A".repeat(121); // 121 chars
        let err = long_desc.parse::<Description>().unwrap_err();
        assert!(matches!(err, DescriptionError::TooLong(121)));
        assert_eq!(err.to_string(), "description too long: expected 121 <= 120");
    }

    #[test]
    fn description_invalid_multiple_lines() {
        // Newline character
        let err = "First line\nSecond line"
            .parse::<Description>()
            .unwrap_err();
        assert!(matches!(err, DescriptionError::MultipleLines));
        assert_eq!(err.to_string(), "description cannot contain multiple lines");

        // Carriage return
        let err = "First line\rSecond line"
            .parse::<Description>()
            .unwrap_err();
        assert!(matches!(err, DescriptionError::MultipleLines));

        // Both CR and LF
        let err = "First line\r\nSecond line"
            .parse::<Description>()
            .unwrap_err();
        assert!(matches!(err, DescriptionError::MultipleLines));

        // Multiple newlines
        let err = "Line 1\n\nLine 3".parse::<Description>().unwrap_err();
        assert!(matches!(err, DescriptionError::MultipleLines));

        // Newline at end
        let err = "Single line\n".parse::<Description>().unwrap_err();
        assert!(matches!(err, DescriptionError::MultipleLines));

        // Newline at start
        let err = "\nSingle line".parse::<Description>().unwrap_err();
        assert!(matches!(err, DescriptionError::MultipleLines));
    }

    #[test]
    fn tag_name_valid_cases() {
        // Basic lowercase
        assert!("personal".parse::<TagName>().is_ok());
        assert!("work".parse::<TagName>().is_ok());
        assert!("home".parse::<TagName>().is_ok());

        // With numbers
        assert!("project1".parse::<TagName>().is_ok());
        assert!("2023".parse::<TagName>().is_ok());
        assert!("quarter2".parse::<TagName>().is_ok());

        // With hyphens
        assert!("work-related".parse::<TagName>().is_ok());
        assert!("home-office".parse::<TagName>().is_ok());
        assert!("long-term-investment".parse::<TagName>().is_ok());

        // Mixed patterns
        assert!("project-2023".parse::<TagName>().is_ok());
        assert!("q1-report".parse::<TagName>().is_ok());
        assert!("tax-year-2023".parse::<TagName>().is_ok());

        // Starting/ending with numbers
        assert!("2023-taxes".parse::<TagName>().is_ok());
        assert!("project-2".parse::<TagName>().is_ok());
        assert!("1st-quarter".parse::<TagName>().is_ok());

        // Boundary case - exactly max length (60 chars)
        let max_length_tag = "a".repeat(60);
        assert!(max_length_tag.parse::<TagName>().is_ok());
    }

    #[test]
    fn tag_name_invalid_empty() {
        let err = "".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::Empty));
        assert_eq!(err.to_string(), "tag name cannot be empty");
    }

    #[test]
    fn tag_name_invalid_too_long() {
        let long_tag = "a".repeat(61); // 61 chars
        let err = long_tag.parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::TooLong(61)));
        assert_eq!(err.to_string(), "tag name too long: expected 61 <= 60");
    }

    #[test]
    fn tag_name_invalid_first_character() {
        // Uppercase first
        let err = "Personal".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadFirstCharacter(_)));
        assert_eq!(
            err.to_string(),
            "tag name must start with lowercase or digit: Personal"
        );

        // Special character first
        let err = "-work".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadFirstCharacter(_)));

        let err = "_personal".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadFirstCharacter(_)));

        // Digit first should be OK
        assert!("1personal".parse::<TagName>().is_ok());
    }

    #[test]
    fn tag_name_invalid_last_character() {
        let err = "work-".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadLastCharacter(_)));
        assert_eq!(
            err.to_string(),
            "tag name must end with lowercase or digit: work-"
        );

        let err = "personal_".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadLastCharacter(_)));
    }

    #[test]
    fn tag_name_invalid_characters() {
        // Uppercase in middle
        let err = "workRelated".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadCharacter(_)));
        assert_eq!(
            err.to_string(),
            "tag name must contain lowercase and hyphens: workRelated"
        );

        // Special characters
        let err = "work_related".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadCharacter(_)));

        let err = "work.related".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadCharacter(_)));

        let err = "work@home".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::BadCharacter(_)));

        let err = "work related".parse::<TagName>().unwrap_err(); // space
        assert!(matches!(err, TagNameError::BadCharacter(_)));
    }

    #[test]
    fn tag_name_invalid_consecutive_hyphens() {
        let err = "work--related".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::ConsecutiveHyphens(_)));
        assert_eq!(
            err.to_string(),
            "tag name cannot have consecutive hyphens: work--related"
        );

        let err = "long---term".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::ConsecutiveHyphens(_)));

        let err = "project--2023".parse::<TagName>().unwrap_err();
        assert!(matches!(err, TagNameError::ConsecutiveHyphens(_)));
    }

    #[test]
    fn unit_name_valid_cases() {
        // Basic currency codes
        assert!("USD".parse::<UnitName>().is_ok());
        assert!("EUR".parse::<UnitName>().is_ok());
        assert!("GBP".parse::<UnitName>().is_ok());

        // With numbers
        assert!("BTC1".parse::<UnitName>().is_ok());
        assert!("ETH2".parse::<UnitName>().is_ok());

        // With dots
        assert!("USD.T".parse::<UnitName>().is_ok());
        assert!("SPY.US".parse::<UnitName>().is_ok());
        assert!("AAPL.NASDAQ".parse::<UnitName>().is_ok());

        // Mixed patterns
        assert!("A1.B2.C3".parse::<UnitName>().is_ok());
        assert!("FUND123.US".parse::<UnitName>().is_ok());

        // Boundary cases - exactly min/max length
        assert!("AB".parse::<UnitName>().is_ok()); // MIN_LENGTH = 2
        assert!("ABCDEFGHIJKLMNOP".parse::<UnitName>().is_ok()); // MAX_LENGTH = 16
    }

    #[test]
    fn unit_name_invalid_empty() {
        let err = "".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::Empty));
        assert_eq!(err.to_string(), "unit name cannot be empty");
    }

    #[test]
    fn unit_name_invalid_too_short() {
        let err = "A".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::TooShort(1)));
        assert_eq!(err.to_string(), "unit name too short: expected 1 >= 2");
    }

    #[test]
    fn unit_name_invalid_too_long() {
        let long_name = "ABCDEFGHIJKLMNOPQ"; // 17 chars
        let err = long_name.parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::TooLong(17)));
        assert_eq!(err.to_string(), "unit name too long: expected 17 <= 16");
    }

    #[test]
    fn unit_name_invalid_first_character() {
        // Lowercase first
        let err = "usd".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::BadFirstCharacter(_)));
        assert_eq!(
            err.to_string(),
            "unit name must start with an uppercase: usd"
        );

        // Special character first
        let err = ".USD".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::BadFirstCharacter(_)));

        // Number first
        let err = "1USD".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::BadFirstCharacter(_)));
    }

    #[test]
    fn unit_name_invalid_last_character() {
        let err = "USD.".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::BadLastCharacter(_)));
        assert_eq!(err.to_string(), "unit name cannot end with a dot: USD.");
    }

    #[test]
    fn unit_name_invalid_characters() {
        // Lowercase in middle
        let err = "UsDT".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::BadCharacter(_)));
        assert_eq!(
            err.to_string(),
            "unit name must contain uppercase and dots: UsDT"
        );

        // Special characters
        let err = "US-D".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::BadCharacter(_)));

        let err = "US_D".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::BadCharacter(_)));

        let err = "US@D".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::BadCharacter(_)));
    }

    #[test]
    fn unit_name_invalid_consecutive_dots() {
        let err = "USD..T".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::ConsecutiveDots(_)));
        assert_eq!(
            err.to_string(),
            "unit name cannot have consecutive dots: USD..T"
        );

        let err = "US...D".parse::<UnitName>().unwrap_err();
        assert!(matches!(err, UnitNameError::ConsecutiveDots(_)));
    }
}
