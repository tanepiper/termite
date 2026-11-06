//! JSON that keeps its key order, so changing one entry in a person's
//! settings file shows as one small change and not as a sorted file.

use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::{Number, Value};

/// A JSON value whose objects keep the order they were written in.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Json {
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub(super) fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|error| error.to_string())
    }

    /// A value built with `json!`. Keys come out sorted, which is fine for
    /// entries termite writes itself.
    pub(super) fn from_value(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(flag) => Self::Bool(flag),
            Value::Number(number) => Self::Number(number),
            Value::String(text) => Self::String(text),
            Value::Array(items) => {
                Self::Array(items.into_iter().map(Self::from_value).collect())
            },
            Value::Object(map) => Self::Object(
                map.into_iter()
                    .map(|(key, item)| (key, Self::from_value(item)))
                    .collect(),
            ),
        }
    }

    /// The text of a settings file: two spaces, and a final new line.
    pub(super) fn pretty(&self) -> String {
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        format!("{text}\n")
    }

    pub(super) fn get(&self, key: &str) -> Option<&Json> {
        let Self::Object(entries) = self else {
            return None;
        };
        entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, item)| item)
    }

    pub(super) fn get_mut(&mut self, key: &str) -> Option<&mut Json> {
        let Self::Object(entries) = self else {
            return None;
        };
        entries
            .iter_mut()
            .find(|(name, _)| name == key)
            .map(|(_, item)| item)
    }

    pub(super) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// The value at `key`, added as `empty` when it isn't there. `None` when
    /// this isn't an object.
    pub(super) fn entry(
        &mut self,
        key: &str,
        empty: Json,
    ) -> Option<&mut Json> {
        let Self::Object(entries) = self else {
            return None;
        };
        let index = match entries.iter().position(|(name, _)| name == key) {
            Some(index) => index,
            None => {
                entries.push((key.to_owned(), empty));
                entries.len() - 1
            },
        };
        entries.get_mut(index).map(|(_, item)| item)
    }

    pub(super) fn remove(&mut self, key: &str) {
        if let Self::Object(entries) = self {
            entries.retain(|(name, _)| name != key);
        }
    }
}

impl Serialize for Json {
    fn serialize<S: Serializer>(&self, out: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => out.serialize_unit(),
            Self::Bool(flag) => out.serialize_bool(*flag),
            Self::Number(number) => number.serialize(out),
            Self::String(text) => out.serialize_str(text),
            Self::Array(items) => items.serialize(out),
            Self::Object(entries) => {
                let mut map = out.serialize_map(Some(entries.len()))?;
                for (key, item) in entries {
                    map.serialize_entry(key, item)?;
                }
                map.end()
            },
        }
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("JSON")
    }

    fn visit_unit<E>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }

    fn visit_bool<E>(self, flag: bool) -> Result<Json, E> {
        Ok(Json::Bool(flag))
    }

    fn visit_i64<E>(self, number: i64) -> Result<Json, E> {
        Ok(Json::Number(number.into()))
    }

    fn visit_u64<E>(self, number: u64) -> Result<Json, E> {
        Ok(Json::Number(number.into()))
    }

    fn visit_f64<E: serde::de::Error>(self, number: f64) -> Result<Json, E> {
        Number::from_f64(number)
            .map(Json::Number)
            .ok_or_else(|| E::custom("a number JSON cannot hold"))
    }

    fn visit_str<E>(self, text: &str) -> Result<Json, E> {
        Ok(Json::String(text.to_owned()))
    }

    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> Result<Json, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Json::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(
        self,
        mut map: A,
    ) -> Result<Json, A::Error> {
        let mut entries = Vec::new();
        while let Some(entry) = map.next_entry()? {
            entries.push(entry);
        }
        Ok(Json::Object(entries))
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(input: D) -> Result<Self, D::Error> {
        input.deserialize_any(JsonVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::Json;

    #[test]
    fn given_a_settings_file_then_it_comes_back_unchanged() {
        let text = "{\n  \"zebra\": 1,\n  \"apple\": [\n    true,\n    null,\n    1.5\n  ],\n  \"mango\": {\n    \"b\": \"x\",\n    \"a\": \"y\"\n  }\n}\n";

        assert_eq!(Json::parse(text).expect("parses").pretty(), text);
    }

    #[test]
    fn given_text_that_is_not_json_then_it_is_an_error() {
        assert!(Json::parse("{ // a comment\n}").is_err());
    }
}
