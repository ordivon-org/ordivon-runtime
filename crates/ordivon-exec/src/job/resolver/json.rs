use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use super::{policy_error, policy_io_error};
use crate::job::JobContractError;

pub(super) fn reject_duplicate_json_keys(bytes: &[u8]) -> Result<(), JobContractError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    DuplicateCheckedValue::deserialize(&mut deserializer).map_err(|error| {
        policy_error(
            format!("invalid capability policy JSON structure: {error}"),
            "policy",
        )
    })?;
    deserializer.end().map_err(|error| {
        policy_error(
            format!("invalid trailing capability policy JSON: {error}"),
            "policy",
        )
    })
}

struct DuplicateCheckedValue;

impl<'de> Deserialize<'de> for DuplicateCheckedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(DuplicateCheckedVisitor)
    }
}

struct DuplicateCheckedVisitor;

impl<'de> Visitor<'de> for DuplicateCheckedVisitor {
    type Value = DuplicateCheckedValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(DuplicateCheckedValue)
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(DuplicateCheckedValue)
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(DuplicateCheckedValue)
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(DuplicateCheckedValue)
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(DuplicateCheckedValue)
    }

    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
        Ok(DuplicateCheckedValue)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(DuplicateCheckedValue)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(DuplicateCheckedValue)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        DuplicateCheckedValue::deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<DuplicateCheckedValue>()?.is_some() {}
        Ok(DuplicateCheckedValue)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(de::Error::custom(format!("duplicate JSON key {key}")));
            }
            map.next_value::<DuplicateCheckedValue>()?;
        }
        Ok(DuplicateCheckedValue)
    }
}

pub(super) fn canonical_digest(value: &impl Serialize) -> Result<String, JobContractError> {
    let value = serde_json::to_value(value).map_err(|error| {
        policy_error(
            format!("cannot serialize canonical value: {error}"),
            "digest",
        )
    })?;
    let mut bytes = Vec::new();
    write_canonical_json(&value, &mut bytes)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

fn write_canonical_json(value: &Value, output: &mut impl Write) -> Result<(), JobContractError> {
    match value {
        Value::Null => canonical_write(output, b"null"),
        Value::Bool(value) => canonical_write(output, if *value { b"true" } else { b"false" }),
        Value::Number(value) => canonical_write(output, value.to_string().as_bytes()),
        Value::String(value) => {
            let encoded = serde_json::to_string(value).map_err(|error| {
                policy_error(format!("cannot encode canonical string: {error}"), "digest")
            })?;
            canonical_write(output, encoded.as_bytes())
        }
        Value::Array(values) => {
            canonical_write(output, b"[")?;
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    canonical_write(output, b",")?;
                }
                write_canonical_json(value, output)?;
            }
            canonical_write(output, b"]")
        }
        Value::Object(values) => {
            canonical_write(output, b"{")?;
            let ordered: BTreeMap<_, _> = values.iter().collect();
            for (index, (key, value)) in ordered.into_iter().enumerate() {
                if index > 0 {
                    canonical_write(output, b",")?;
                }
                let encoded = serde_json::to_string(key).map_err(|error| {
                    policy_error(format!("cannot encode canonical key: {error}"), "digest")
                })?;
                canonical_write(output, encoded.as_bytes())?;
                canonical_write(output, b":")?;
                write_canonical_json(value, output)?;
            }
            canonical_write(output, b"}")
        }
    }
}

fn canonical_write(output: &mut impl Write, bytes: &[u8]) -> Result<(), JobContractError> {
    output
        .write_all(bytes)
        .map_err(|error| policy_io_error("digest", "cannot build canonical JSON", error))
}
