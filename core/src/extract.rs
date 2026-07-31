//! Deserialization of extracted transactions JSON.
//!
//! Use [`Format::from_json`] to get the corresponding representation of the
//! versioned data. For version 1 data types, see [`extract_v1`] module.
use crate::extract_v1;

/// A version of extracted transactions JSON file.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Version {
    V1_0,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::V1_0 => f.write_str("1.0"),
        }
    }
}

impl std::str::FromStr for Version {
    type Err = ParseVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1.0" => Ok(Version::V1_0),
            _ => Err(ParseVersionError(s.to_owned())),
        }
    }
}

serde_impls!(Version, "a valid version");

/// An error returned if the version is unrecognized.
#[derive(Debug, thiserror::Error)]
pub struct ParseVersionError(String);

impl std::fmt::Display for ParseVersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown version: {}", self.0)
    }
}

/// Versioned extracted transactions format.
#[derive(Clone, Debug)]
pub enum Format {
    /// Transactions of version 1 format.
    V1(extract_v1::Transactions),
}

impl Format {
    /// Deserializes an extracted transaction format from bytes containing JSON
    /// text.
    ///
    /// # Errors
    ///
    /// Returns [`ParseFormatError::MissingVersion`] if JSON does not contain
    /// a version. Otherwise, returns [`ParseFormatError::JsonError`] if the
    /// version or the format itself is malformed.
    pub fn from_json(bytes: &[u8]) -> Result<Format, ParseFormatError> {
        let json: serde_json::Value = serde_json::from_slice(bytes)?;

        let version: Version = serde_json::from_value(
            json.get("version")
                .ok_or(ParseFormatError::MissingVersion)?
                .to_owned(),
        )?;

        match version {
            Version::V1_0 => Ok(Format::V1(serde_json::from_value(json)?)),
        }
    }
}

/// An error returned when parsing of extracted transaction format fails.
#[derive(Debug, thiserror::Error)]
pub enum ParseFormatError {
    #[error("failed to parse json: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("format version is missing")]
    MissingVersion,
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1_0_JSON: &[u8] = include_bytes!("../testdata/v1_0.json");

    #[test]
    fn format_from_json_v1_0() {
        Format::from_json(V1_0_JSON).expect("json should be valid");
    }

    #[test]
    fn format_from_json_bad_version() {
        let json = b"{}";
        let err = Format::from_json(json).expect_err("version should not parse");
        assert_eq!(err.to_string(), "format version is missing");

        let json = b"{\"version\": \"unknown\"}";
        Format::from_json(json).expect_err("version should not parse");
    }
}
