//! Small serde helpers used across the wardnet-common API types.

use serde::{Deserialize, Deserializer};

/// Deserializer for a "nullable field" — distinguishes three states that
/// standard `Option<Option<T>>` cannot express without this helper:
///
/// | JSON           | Result           | Meaning          |
/// |----------------|------------------|------------------|
/// | field absent   | `None`           | don't touch      |
/// | `null`         | `Some(None)`     | clear / set NULL |
/// | `"text"`       | `Some(Some(s))`  | replace value    |
///
/// Used with `#[serde(default, deserialize_with = "crate::serde_util::nullable_field")]`.
/// The `#[serde(default)]` handles the "absent" → `None` case; this function
/// handles the two cases where the field IS present in the JSON.
pub fn nullable_field<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    // Field is present in the JSON — wrap the inner deserialize so that
    // `null` → `Some(None)` and `"text"` → `Some(Some("text"))`.
    Ok(Some(Option::<T>::deserialize(de)?))
}
