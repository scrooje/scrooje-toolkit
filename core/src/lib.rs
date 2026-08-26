//! Data interchange formats and types for working with
//! [Scrooje](https://scroo.je/).
macro_rules! serde_impls {
    ($ty:ty, $expecting:expr) => {
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct Visitor;
                impl serde::de::Visitor<'_> for Visitor {
                    type Value = $ty;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter<'_>,
                    ) -> std::fmt::Result {
                        formatter.write_str($expecting)
                    }

                    fn visit_str<E>(self, s: &str) -> Result<$ty, E>
                    where
                        E: serde::de::Error,
                    {
                        s.parse().map_err(serde::de::Error::custom)
                    }
                }
                deserializer.deserialize_str(Visitor)
            }
        }

        impl serde::Serialize for $ty {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }
    };
}

pub mod entry;
pub mod extract;
pub mod extract_v1;
pub mod types;
