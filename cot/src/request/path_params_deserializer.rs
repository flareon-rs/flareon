use std::fmt::Display;

use serde::de::{DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::Deserializer;
use thiserror::Error;

use crate::request::PathParams;

/// An error that occurs when deserializing path parameters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum PathParamsDeserializerError {
    /// Invalid number of path parameters
    #[error("Invalid number of path parameters: expected {expected}, got {actual}")]
    InvalidParamNumber {
        /// The expected number of path parameters.
        expected: usize,
        /// The actual number of path parameters that were provided.
        actual: usize,
    },
    /// A value cannot be parsed into given type.
    #[error("Failed to parse value `{value}` as `{expected_type}`")]
    ParseError {
        /// The value that was provided.
        value: String,
        /// The expected type name.
        expected_type: &'static str,
    },
    /// Deserialization into given type is not supported.
    #[error("Deserializing `{type_name}` is not supported")]
    UnsupportedType {
        /// The type name that was provided.
        type_name: &'static str,
    },
    /// An error that doesn't fit any other variant.
    #[error("{0}")]
    Custom(String),
}

impl PathParamsDeserializerError {
    fn unsupported_type<'de, V>() -> Self
    where
        V: Visitor<'de>,
    {
        Self::UnsupportedType {
            type_name: std::any::type_name::<V::Value>(),
        }
    }
}

impl serde::de::Error for PathParamsDeserializerError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Self::Custom(msg.to_string())
    }
}

#[derive(Debug)]
pub(super) struct PathParamsDeserializer<'de> {
    path_params: &'de PathParams,
}

impl<'de> PathParamsDeserializer<'de> {
    #[must_use]
    pub(super) fn new(path_params: &'de PathParams) -> Self {
        Self { path_params }
    }

    fn get_single_value(&self) -> Result<&'de str, PathParamsDeserializerError> {
        self.check_param_num(1)?;

        let value = self
            .path_params
            .get_index(0)
            .expect("we checked for len == 1");
        Ok(value)
    }

    fn check_param_num(&self, expected: usize) -> Result<(), PathParamsDeserializerError> {
        if self.path_params.len() == expected {
            Ok(())
        } else {
            Err(PathParamsDeserializerError::InvalidParamNumber {
                expected,
                actual: self.path_params.len(),
            })
        }
    }
}

macro_rules! deserialize_value {
    ($deserialize_fn_name:ident, $visit_fn_name:ident, $type_name:ident) => {
        fn $deserialize_fn_name<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            let value = self.get_single_value()?;

            let value = value
                .parse()
                .map_err(|_| PathParamsDeserializerError::ParseError {
                    value: value.to_string(),
                    expected_type: stringify!($type_name),
                })?;

            visitor.$visit_fn_name(value)
        }
    };
}

macro_rules! deserialize_not_supported {
    ($deserialize_fn_name:ident) => {
        fn $deserialize_fn_name<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(PathParamsDeserializerError::unsupported_type::<V>())
        }
    };
}

impl<'de> Deserializer<'de> for PathParamsDeserializer<'de> {
    type Error = PathParamsDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    deserialize_value!(deserialize_bool, visit_bool, bool);
    deserialize_value!(deserialize_i8, visit_i8, i8);
    deserialize_value!(deserialize_i16, visit_i16, i16);
    deserialize_value!(deserialize_i32, visit_i32, i32);
    deserialize_value!(deserialize_i64, visit_i64, i64);
    deserialize_value!(deserialize_i128, visit_i128, i128);
    deserialize_value!(deserialize_u8, visit_u8, u8);
    deserialize_value!(deserialize_u16, visit_u16, u16);
    deserialize_value!(deserialize_u32, visit_u32, u32);
    deserialize_value!(deserialize_u64, visit_u64, u64);
    deserialize_value!(deserialize_u128, visit_u128, u128);
    deserialize_value!(deserialize_f32, visit_f32, f32);
    deserialize_value!(deserialize_f64, visit_f64, f64);
    deserialize_value!(deserialize_char, visit_char, char);
    deserialize_value!(deserialize_string, visit_string, String);

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.get_single_value()?;
        visitor.visit_borrowed_str(value)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.get_single_value()?;
        visitor.visit_bytes(value.as_bytes())
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let value = self.get_single_value()?;
        visitor.visit_byte_buf(value.as_bytes().to_owned())
    }

    deserialize_not_supported!(deserialize_option);

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(SequenceDeserializer::new(self.path_params))
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.check_param_num(len)?;

        visitor.visit_seq(SequenceDeserializer::new(self.path_params))
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.check_param_num(len)?;

        visitor.visit_seq(SequenceDeserializer::new(self.path_params))
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(MapDeserializer::new(self.path_params))
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.check_param_num(fields.len())?;

        visitor.visit_map(MapDeserializer::new(self.path_params))
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(EnumDeserializer::new(self.get_single_value()?))
    }

    deserialize_not_supported!(deserialize_identifier);

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
}

#[derive(Debug)]
struct SequenceDeserializer<'de> {
    path_params: &'de PathParams,
    index: usize,
}

impl<'de> SequenceDeserializer<'de> {
    fn new(path_params: &'de PathParams) -> Self {
        Self {
            path_params,
            index: 0,
        }
    }
}

impl<'de> SeqAccess<'de> for SequenceDeserializer<'de> {
    type Error = PathParamsDeserializerError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if let Some(value) = self.path_params.get_index(self.index) {
            let key = self
                .path_params
                .key_at_index(self.index)
                .expect("a value should always have a key");
            self.index += 1;

            let deserialized = seed.deserialize(ValueDeserializer::new(key, value))?;
            Ok(Some(deserialized))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
struct MapDeserializer<'de> {
    path_params: &'de PathParams,
    index: usize,
}

impl<'de> MapDeserializer<'de> {
    #[must_use]
    fn new(path_params: &'de PathParams) -> Self {
        Self {
            path_params,
            index: 0,
        }
    }
}

impl<'de> MapAccess<'de> for MapDeserializer<'de> {
    type Error = PathParamsDeserializerError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        if let Some(key) = self.path_params.key_at_index(self.index) {
            let deserialized = seed.deserialize(ValueDeserializer::new_value(key))?;
            Ok(Some(deserialized))
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let key = self
            .path_params
            .key_at_index(self.index)
            .expect("next_key_seed should've been called first and checked if the key exists");
        let value = self
            .path_params
            .get_index(self.index)
            .expect("next_key_seed should've been called first and checked if the value exists");
        self.index += 1;

        let deserialized = seed.deserialize(ValueDeserializer::new(key, value))?;
        Ok(deserialized)
    }
}

#[derive(Debug)]
struct ValueDeserializer<'de> {
    key: Option<&'de str>,
    value: &'de str,
}

impl<'de> ValueDeserializer<'de> {
    #[must_use]
    fn new(key: &'de str, value: &'de str) -> Self {
        Self {
            key: Some(key),
            value,
        }
    }

    #[must_use]
    fn new_value(value: &'de str) -> Self {
        Self { key: None, value }
    }

    #[allow(clippy::unnecessary_wraps)] // allows to use the same `deserialize_value!` macro
    fn get_single_value(&self) -> Result<&'de str, PathParamsDeserializerError> {
        Ok(self.value)
    }
}

impl<'de> Deserializer<'de> for ValueDeserializer<'de> {
    type Error = PathParamsDeserializerError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    deserialize_value!(deserialize_bool, visit_bool, bool);
    deserialize_value!(deserialize_i8, visit_i8, i8);
    deserialize_value!(deserialize_i16, visit_i16, i16);
    deserialize_value!(deserialize_i32, visit_i32, i32);
    deserialize_value!(deserialize_i64, visit_i64, i64);
    deserialize_value!(deserialize_i128, visit_i128, i128);
    deserialize_value!(deserialize_u8, visit_u8, u8);
    deserialize_value!(deserialize_u16, visit_u16, u16);
    deserialize_value!(deserialize_u32, visit_u32, u32);
    deserialize_value!(deserialize_u64, visit_u64, u64);
    deserialize_value!(deserialize_u128, visit_u128, u128);
    deserialize_value!(deserialize_f32, visit_f32, f32);
    deserialize_value!(deserialize_f64, visit_f64, f64);
    deserialize_value!(deserialize_char, visit_char, char);
    deserialize_value!(deserialize_string, visit_string, String);

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_str(self.value)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_bytes(self.value.as_bytes())
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_byte_buf(self.value.as_bytes().to_owned())
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    deserialize_not_supported!(deserialize_seq);

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if let Some(key) = self.key {
            if len == 2 {
                return visitor.visit_seq(ArrayDeserializer::new([key, self.value]));
            }
        }

        Err(PathParamsDeserializerError::unsupported_type::<V>())
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(PathParamsDeserializerError::unsupported_type::<V>())
    }

    deserialize_not_supported!(deserialize_map);

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(PathParamsDeserializerError::unsupported_type::<V>())
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(EnumDeserializer::new(self.value))
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_str(self.value)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

#[derive(Debug)]
struct ArrayDeserializer<'de, const LEN: usize> {
    sequence: [&'de str; LEN],
    index: usize,
}

impl<'de, const LEN: usize> ArrayDeserializer<'de, LEN> {
    #[must_use]
    fn new(sequence: [&'de str; LEN]) -> Self {
        Self { sequence, index: 0 }
    }
}

impl<'de, const LEN: usize> SeqAccess<'de> for ArrayDeserializer<'de, LEN> {
    type Error = PathParamsDeserializerError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if let Some(value) = self.sequence.get(self.index) {
            self.index += 1;

            seed.deserialize(ValueDeserializer::new_value(value))
                .map(Some)
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
struct EnumDeserializer<'de> {
    value: &'de str,
}

impl<'de> EnumDeserializer<'de> {
    #[must_use]
    fn new(value: &'de str) -> Self {
        Self { value }
    }
}

impl<'de> EnumAccess<'de> for EnumDeserializer<'de> {
    type Error = PathParamsDeserializerError;
    type Variant = UnitVariant;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        Ok((
            seed.deserialize(ValueDeserializer::new_value(self.value))?,
            UnitVariant,
        ))
    }
}

#[derive(Debug)]
struct UnitVariant;

impl<'de> VariantAccess<'de> for UnitVariant {
    type Error = PathParamsDeserializerError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        Err(PathParamsDeserializerError::UnsupportedType {
            type_name: "newtype enum variant",
        })
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(PathParamsDeserializerError::UnsupportedType {
            type_name: "tuple enum variant",
        })
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(PathParamsDeserializerError::UnsupportedType {
            type_name: "struct enum variant",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::Deserialize;

    use super::*;

    macro_rules! test_deserialize_value {
        ($test_name:ident, $ty:ty, $value:literal, $expected:literal) => {
            #[test]
            fn $test_name() {
                let path_params = create_path_params([("some_name", $value)]);
                let deserializer = PathParamsDeserializer::new(&path_params);
                let test_val = <$ty>::deserialize(deserializer).unwrap();
                assert_eq!(test_val, $expected);
            }
        };
    }

    test_deserialize_value!(deserialize_str, &str, "test", "test");
    test_deserialize_value!(deserialize_string, String, "test", "test");
    test_deserialize_value!(deserialize_bool_true, bool, "true", true);
    test_deserialize_value!(deserialize_bool_false, bool, "false", false);
    test_deserialize_value!(deserialize_i8, i8, "42", 42);
    test_deserialize_value!(deserialize_i16, i16, "2137", 2137);
    test_deserialize_value!(deserialize_i32, i32, "2137420", 2137420);
    test_deserialize_value!(deserialize_i64, i64, "2137420691337", 2137_420_691_337);
    test_deserialize_value!(
        deserialize_i128,
        i128,
        "21372137213721372137",
        21372137213721372137
    );
    test_deserialize_value!(deserialize_u8, u8, "42", 42);
    test_deserialize_value!(deserialize_u16, u16, "2137", 2137);
    test_deserialize_value!(deserialize_u32, u32, "2137420", 2137420);
    test_deserialize_value!(deserialize_u64, u64, "2137420691337", 2137420691337);
    test_deserialize_value!(
        deserialize_u128,
        u128,
        "21372137213721372137",
        21372137213721372137
    );
    test_deserialize_value!(deserialize_f32, f32, "2.137", 2.137);
    test_deserialize_value!(deserialize_f64, f64, "2.137", 2.137);
    test_deserialize_value!(deserialize_char, char, "a", 'a');

    #[test]
    fn deserialize_tuple() {
        let path_params = create_path_params([("a", "test"), ("b", "123"), ("c", "true")]);
        let actual =
            <(String, i32, bool)>::deserialize(PathParamsDeserializer::new(&path_params)).unwrap();
        assert_eq!(actual, ("test".to_string(), 123, true));
    }

    #[test]
    fn deserialize_tuple_pairs() {
        let path_params = create_path_params([("a", "test"), ("b", "123"), ("c", "true")]);
        let actual = <((String, String), (String, i32), (char, bool))>::deserialize(
            PathParamsDeserializer::new(&path_params),
        )
        .unwrap();
        assert_eq!(
            actual,
            (
                ("a".to_string(), "test".to_string()),
                ("b".to_string(), 123),
                ('c', true)
            )
        );
    }

    #[test]
    fn deserialize_vec() {
        let path_params = create_path_params([("a", "1"), ("b", "2"), ("c", "3")]);
        let actual = <Vec<i32>>::deserialize(PathParamsDeserializer::new(&path_params)).unwrap();
        assert_eq!(actual, vec![1, 2, 3]);
    }

    #[test]
    fn deserialize_struct() {
        #[derive(Debug, Deserialize, PartialEq, Eq)]
        struct Params {
            a: String,
            b: i32,
            c: bool,
        }

        let path_params = create_path_params([("a", "test"), ("b", "123"), ("c", "true")]);
        let actual = Params::deserialize(PathParamsDeserializer::new(&path_params)).unwrap();
        assert_eq!(
            actual,
            Params {
                a: "test".to_string(),
                b: 123,
                c: true,
            }
        );
    }

    #[test]
    fn deserialize_map() {
        let path_params = create_path_params([("a", "test"), ("b", "123"), ("c", "true")]);
        let actual =
            <HashMap<&str, &str>>::deserialize(PathParamsDeserializer::new(&path_params)).unwrap();
        assert_eq!(
            actual,
            HashMap::from([("a", "test"), ("b", "123"), ("c", "true")])
        );
    }

    #[test]
    fn deserialize_map_ints() {
        let path_params = create_path_params([("1", "123"), ("2", "456"), ("3", "789")]);
        let actual =
            <HashMap<i32, i32>>::deserialize(PathParamsDeserializer::new(&path_params)).unwrap();
        assert_eq!(actual, HashMap::from([(1, 123), (2, 456), (3, 789)]));
    }

    #[test]
    fn deserialize_enum() {
        #[derive(Debug, PartialEq, Eq, Deserialize)]
        enum ParamEnum {
            A,
            B,
        }

        let path_params = create_path_params([("x", "A")]);

        let actual = ParamEnum::deserialize(PathParamsDeserializer::new(&path_params)).unwrap();
        assert_eq!(actual, ParamEnum::A);
    }

    #[test]
    fn deserialize_enum_vec() {
        #[derive(Debug, PartialEq, Eq, Deserialize)]
        enum ParamEnum {
            A,
            B,
            #[serde(rename = "foo")]
            C,
        }

        let path_params = create_path_params([("x", "A"), ("y", "B"), ("z", "foo")]);

        let actual =
            <Vec<ParamEnum>>::deserialize(PathParamsDeserializer::new(&path_params)).unwrap();
        assert_eq!(actual, vec![ParamEnum::A, ParamEnum::B, ParamEnum::C]);
    }

    #[test]
    fn deserialize_wrong_param_num_tuple_error() {
        let path_params = create_path_params([("x", "a")]);

        let actual =
            <(String, String)>::deserialize(PathParamsDeserializer::new(&path_params)).unwrap_err();
        assert_eq!(
            actual,
            PathParamsDeserializerError::InvalidParamNumber {
                expected: 2,
                actual: 1,
            }
        );
    }

    #[test]
    fn deserialize_wrong_param_num_struct_error() {
        #[derive(Debug, PartialEq, Eq, Deserialize)]
        struct Params {
            a: String,
            b: String,
        }

        let path_params = create_path_params([("x", "a")]);

        let actual = Params::deserialize(PathParamsDeserializer::new(&path_params)).unwrap_err();
        assert_eq!(
            actual,
            PathParamsDeserializerError::InvalidParamNumber {
                expected: 2,
                actual: 1,
            }
        );
    }

    #[test]
    fn deserialize_parse_error() {
        let path_params = create_path_params([("x", "a")]);

        let actual = i32::deserialize(PathParamsDeserializer::new(&path_params)).unwrap_err();
        assert_eq!(
            actual,
            PathParamsDeserializerError::ParseError {
                value: "a".to_string(),
                expected_type: "i32",
            }
        );
    }

    #[test]
    fn deserialize_unsupported_type_error() {
        let path_params = create_path_params([("x", "a")]);

        let actual =
            <Option<i32>>::deserialize(PathParamsDeserializer::new(&path_params)).unwrap_err();
        assert_eq!(
            actual,
            PathParamsDeserializerError::UnsupportedType {
                type_name: "core::option::Option<i32>",
            }
        );
    }

    fn create_path_params<A, B, I>(items: I) -> PathParams
    where
        A: ToString,
        B: ToString,
        I: IntoIterator<Item = (A, B)>,
    {
        let mut path_params = PathParams::new();
        for (a, b) in items {
            path_params.insert(a.to_string(), b.to_string());
        }

        path_params
    }
}
