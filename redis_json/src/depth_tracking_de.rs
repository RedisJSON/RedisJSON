use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Deserializer as JsonDeserializer;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
struct DepthTracker {
    current: Rc<RefCell<usize>>,
    max: Rc<RefCell<usize>>,
}

impl DepthTracker {
    fn new() -> Self {
        Self {
            current: Rc::new(RefCell::new(0)),
            max: Rc::new(RefCell::new(0)),
        }
    }

    fn enter(&self) {
        let mut current = self.current.borrow_mut();
        *current += 1;
        let mut max = self.max.borrow_mut();
        if *current > *max {
            *max = *current;
        }
    }

    fn exit(&self) {
        let mut current = self.current.borrow_mut();
        *current = current.saturating_sub(1);
    }

    fn max_depth(&self) -> usize {
        *self.max.borrow()
    }
}

pub struct DepthTrackingDeserializer<'de> {
    de: JsonDeserializer<serde_json::de::StrRead<'de>>,
    tracker: DepthTracker,
}

impl<'de> DepthTrackingDeserializer<'de> {
    pub fn from_str(s: &'de str) -> Self {
        Self {
            de: JsonDeserializer::from_str(s),
            tracker: DepthTracker::new(),
        }
    }

    pub fn disable_recursion_limit(&mut self) {
        self.de.disable_recursion_limit();
    }

    pub fn max_depth(&self) -> usize {
        self.tracker.max_depth()
    }
}

struct TrackingVisitor<V> {
    inner: V,
    tracker: DepthTracker,
}

impl<'de, V: Visitor<'de>> Visitor<'de> for TrackingVisitor<V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.inner.expecting(formatter)
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.tracker.enter();
        let result = self.inner.visit_map(TrackingMapAccess {
            inner: map,
            tracker: self.tracker.clone(),
        });
        self.tracker.exit();
        result
    }

    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        self.tracker.enter();
        let result = self.inner.visit_seq(TrackingSeqAccess {
            inner: seq,
            tracker: self.tracker.clone(),
        });
        self.tracker.exit();
        result
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        self.inner.visit_bool(v)
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        self.inner.visit_i64(v)
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        self.inner.visit_u64(v)
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        self.inner.visit_f64(v)
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.inner.visit_str(v)
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        self.inner.visit_string(v)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }
    fn visit_some<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        self.inner.visit_some(deserializer)
    }
}

struct TrackingSeed<S> {
    inner: S,
    tracker: DepthTracker,
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for TrackingSeed<S> {
    type Value = S::Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        self.inner.deserialize(TrackingDeserializerGeneric {
            de: deserializer,
            tracker: self.tracker,
        })
    }
}

struct TrackingDeserializerGeneric<D> {
    de: D,
    tracker: DepthTracker,
}

impl<'de, D: de::Deserializer<'de>> de::Deserializer<'de> for TrackingDeserializerGeneric<D> {
    type Error = D::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.de.deserialize_any(TrackingVisitor {
            inner: visitor,
            tracker: self.tracker,
        })
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.tracker.enter();
        let result = self.de.deserialize_map(TrackingVisitor {
            inner: visitor,
            tracker: self.tracker.clone(),
        });
        self.tracker.exit();
        result
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.tracker.enter();
        let result = self.de.deserialize_seq(TrackingVisitor {
            inner: visitor,
            tracker: self.tracker.clone(),
        });
        self.tracker.exit();
        result
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.tracker.enter();
        let result = self.de.deserialize_struct(
            name,
            fields,
            TrackingVisitor {
                inner: visitor,
                tracker: self.tracker.clone(),
            },
        );
        self.tracker.exit();
        result
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct tuple
        tuple_struct enum identifier ignored_any
    }
}

struct TrackingMapAccess<A> {
    inner: A,
    tracker: DepthTracker,
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for TrackingMapAccess<A> {
    type Error = A::Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        self.inner.next_key_seed(seed)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        self.inner.next_value_seed(TrackingSeed {
            inner: seed,
            tracker: self.tracker.clone(),
        })
    }
}

struct TrackingSeqAccess<A> {
    inner: A,
    tracker: DepthTracker,
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for TrackingSeqAccess<A> {
    type Error = A::Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        self.inner.next_element_seed(TrackingSeed {
            inner: seed,
            tracker: self.tracker.clone(),
        })
    }
}

impl<'de> de::Deserializer<'de> for &mut DepthTrackingDeserializer<'de> {
    type Error = serde_json::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.de.deserialize_any(TrackingVisitor {
            inner: visitor,
            tracker: self.tracker.clone(),
        })
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
        byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}
