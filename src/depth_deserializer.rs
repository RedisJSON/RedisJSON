use serde::de;
use std::fmt;

pub struct Stats {
    pub max_depth: usize,
    pub curr_depth: usize,
}

/// Deserializer adapter that avoids stack overflows by dynamically growing the
/// stack.
///
/// At each level of nested deserialization, the adapter will check whether it
/// is within `red_zone` bytes of the end of the stack. If so, it will allocate
/// a new stack of size `stack_size` on which to continue deserialization.
pub struct Deserializer<'stats, D> {
    pub de: D,
    pub stats: &'stats mut Stats,
}

impl<'stats, D> Deserializer<'stats, D> {
    /// Build a deserializer adapter with reasonable default `red_zone` (64 KB)
    /// and `stack_size` (2 MB).
    pub fn new(deserializer: D, stats: &'stats mut Stats) -> Self {
        Deserializer {
            de: deserializer,
            stats: stats,
        }
    }
}

impl<'de, 'stats, D> de::Deserializer<'de> for Deserializer<'stats, D>
where
    D: de::Deserializer<'de>,
{
    type Error = D::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_any(Visitor::new(visitor, self.stats))
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_bool(Visitor::new(visitor, self.stats))
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_u8(Visitor::new(visitor, self.stats))
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_u16(Visitor::new(visitor, self.stats))
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_u32(Visitor::new(visitor, self.stats))
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_u64(Visitor::new(visitor, self.stats))
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_u128(Visitor::new(visitor, self.stats))
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_i8(Visitor::new(visitor, self.stats))
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_i16(Visitor::new(visitor, self.stats))
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_i32(Visitor::new(visitor, self.stats))
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_i64(Visitor::new(visitor, self.stats))
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_i128(Visitor::new(visitor, self.stats))
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_f32(Visitor::new(visitor, self.stats))
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_f64(Visitor::new(visitor, self.stats))
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_char(Visitor::new(visitor, self.stats))
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_str(Visitor::new(visitor, self.stats))
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_string(Visitor::new(visitor, self.stats))
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_bytes(Visitor::new(visitor, self.stats))
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_byte_buf(Visitor::new(visitor, self.stats))
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_option(Visitor::new(visitor, self.stats))
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_unit(Visitor::new(visitor, self.stats))
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de
            .deserialize_unit_struct(name, Visitor::new(visitor, self.stats))
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de
            .deserialize_newtype_struct(name, Visitor::new(visitor, self.stats))
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_seq(Visitor::new(visitor, self.stats))
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_tuple(len, Visitor::new(visitor, self.stats))
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de
            .deserialize_tuple_struct(name, len, Visitor::new(visitor, self.stats))
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_map(Visitor::new(visitor, self.stats))
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de
            .deserialize_struct(name, fields, Visitor::new(visitor, self.stats))
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de
            .deserialize_enum(name, variants, Visitor::new(visitor, self.stats))
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de
            .deserialize_ignored_any(Visitor::new(visitor, self.stats))
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.de.deserialize_identifier(Visitor::new(visitor, self.stats))
    }

    fn is_human_readable(&self) -> bool {
        self.de.is_human_readable()
    }
}

struct Visitor<'stats, V> {
    delegate: V,
    stats: &'stats mut Stats,
}

impl<'stats, V> Visitor<'stats, V> {
    fn new(delegate: V, stats: &'stats mut Stats) -> Self {
        Visitor { delegate, stats }
    }
}

impl<'de, 'stats, V> de::Visitor<'de> for Visitor<'stats, V>
where
    V: de::Visitor<'de>,
{
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        self.delegate.expecting(formatter)
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_bool(v)
    }

    fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_i8(v)
    }

    fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_i16(v)
    }

    fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_i32(v)
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_i64(v)
    }

    fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_i128(v)
    }

    fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_u8(v)
    }

    fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_u16(v)
    }

    fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_u32(v)
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_u64(v)
    }

    fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_u128(v)
    }

    fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_f32(v)
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_f64(v)
    }

    fn visit_char<E>(self, v: char) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_char(v)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_str(v)
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_borrowed_str(v)
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_string(v)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_unit()
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_none()
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        self.delegate.visit_some(Deserializer {
            de: deserializer,
            stats: self.stats,
        })
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        self.delegate.visit_newtype_struct(Deserializer {
            de: deserializer,
            stats: self.stats,
        })
    }

    fn visit_seq<A>(self, visitor: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        self.stats.curr_depth += 1;
        if self.stats.curr_depth > self.stats.max_depth {
            self.stats.max_depth = self.stats.curr_depth;
        }
        let res = self.delegate.visit_seq(SeqAccess::new(visitor, self.stats));
        self.stats.curr_depth -= 1;
        res
    }

    fn visit_map<A>(self, visitor: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        self.stats.curr_depth += 1;
        if self.stats.curr_depth > self.stats.max_depth {
            self.stats.max_depth = self.stats.curr_depth;
        }
        let res = self.delegate.visit_map(MapAccess::new(visitor, self.stats));
        self.stats.curr_depth -= 1;
        res
    }

    fn visit_enum<A>(self, visitor: A) -> Result<Self::Value, A::Error>
    where
        A: de::EnumAccess<'de>,
    {
        self.delegate
            .visit_enum(EnumAccess::new(visitor, self.stats))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_bytes(v)
    }

    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_borrowed_bytes(v)
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.delegate.visit_byte_buf(v)
    }
}

struct EnumAccess<'stats, D> {
    delegate: D,
    stats: &'stats mut Stats,
}

impl<'stats, D> EnumAccess<'stats, D> {
    fn new(delegate: D, stats: &'stats mut Stats) -> Self {
        EnumAccess { delegate, stats }
    }
}

impl<'de, 'stats, D> de::EnumAccess<'de> for EnumAccess<'stats, D>
where
    D: de::EnumAccess<'de>,
{
    type Error = D::Error;
    type Variant = VariantAccess<'stats, D::Variant>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), D::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        self.delegate
            .variant_seed(DeserializeSeed::new(seed, self.stats))
            .map(|(v, vis)| (v, VariantAccess::new(vis, self.stats)))
    }
}

struct VariantAccess<'stats, D> {
    delegate: D,
    stats: &'stats mut Stats,
}

impl<'stats, D> VariantAccess<'stats, D> {
    fn new(delegate: D, stats: &'stats mut Stats) -> Self {
        VariantAccess { delegate, stats }
    }
}

impl<'de, 'stats, D> de::VariantAccess<'de> for VariantAccess<'stats, D>
where
    D: de::VariantAccess<'de>,
{
    type Error = D::Error;

    fn unit_variant(self) -> Result<(), D::Error> {
        self.delegate.unit_variant()
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, D::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        self.delegate
            .newtype_variant_seed(DeserializeSeed::new(seed, self.stats))
    }

    fn tuple_variant<V>(self, len: usize, visitor: V) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.delegate
            .tuple_variant(len, Visitor::new(visitor, self.stats))
    }

    fn struct_variant<V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error>
    where
        V: de::Visitor<'de>,
    {
        self.delegate
            .struct_variant(fields, Visitor::new(visitor, self.stats))
    }
}

struct DeserializeSeed<'stats, S> {
    delegate: S,
    stats: &'stats mut Stats,
}

impl<'stats, S> DeserializeSeed<'stats, S> {
    fn new(delegate: S, stats: &'stats mut Stats) -> Self {
        DeserializeSeed { delegate, stats }
    }
}

impl<'de, 'stats, S> de::DeserializeSeed<'de> for DeserializeSeed<'stats, S>
where
    S: de::DeserializeSeed<'de>,
{
    type Value = S::Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        self.delegate.deserialize(Deserializer {
            de: deserializer,
            stats: self.stats,
        })
    }
}

struct SeqAccess<'stats, D> {
    delegate: D,
    stats: &'stats mut Stats,
}

impl<'stats, D> SeqAccess<'stats, D> {
    fn new(delegate: D, stats: &'stats mut Stats) -> Self {
        SeqAccess { delegate, stats }
    }
}

impl<'de, 'stats, D> de::SeqAccess<'de> for SeqAccess<'stats, D>
where
    D: de::SeqAccess<'de>,
{
    type Error = D::Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, D::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        self.delegate
            .next_element_seed(DeserializeSeed::new(seed, self.stats))
    }

    fn size_hint(&self) -> Option<usize> {
        self.delegate.size_hint()
    }
}

struct MapAccess<'stats, D> {
    delegate: D,
    stats: &'stats mut Stats,
}

impl<'stats, D> MapAccess<'stats, D> {
    fn new(delegate: D, stats: &'stats mut Stats) -> Self {
        MapAccess { delegate, stats }
    }
}

impl<'de, 'stats, D> de::MapAccess<'de> for MapAccess<'stats, D>
where
    D: de::MapAccess<'de>,
{
    type Error = D::Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, D::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        self.delegate
            .next_key_seed(DeserializeSeed::new(seed, self.stats))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, D::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        self.delegate
            .next_value_seed(DeserializeSeed::new(seed, self.stats))
    }

    fn size_hint(&self) -> Option<usize> {
        self.delegate.size_hint()
    }
}

#[cfg(test)]
mod json_path_tests {
    use serde_json::{de::StrRead, Value};
    use serde_json::json;
    use serde::Deserialize;
    use serde_json::de;
    use crate::depth_deserializer::Stats;
    use crate::depth_deserializer::Deserializer;

    macro_rules! verify_json {(
        json: $json:tt,
        expected_depth: $expected_depth:expr
    ) => {
        let mut de = de::Deserializer::new(StrRead::new(stringify!($json)));
        let mut stats = Stats{max_depth: 0, curr_depth: 0};
        let de = Deserializer::new(&mut de, &mut stats);

        let value: Value = Value::deserialize(de).unwrap();
        assert_eq!(json!($json), value);
        assert_eq!(stats.max_depth, $expected_depth);
        assert_eq!(stats.curr_depth, 0);
    }}

    #[test]
    fn basics() {
        verify_json!(json: {"test":[1, 2]}, expected_depth: 2);
        verify_json!(json: {"test":[1, [1, 2]]}, expected_depth: 3);
        verify_json!(json: {"test":[1, [1, {"foo": "bar"}]]}, expected_depth: 4);
        verify_json!(json: {"test":[1, [1, {"foo": "bar", "bar": [1, 3, 4]}]]}, expected_depth: 5);
    }
}