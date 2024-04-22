//! The JSON object array storage.

use std::ops::Deref;
use std::{fmt::Debug, ops::DerefMut};

use listpack_redis::{allocator::ListpackAllocator, Listpack};
use redis_custom_allocator::{CustomAllocator, MemoryConsumption};

use crate::Value;

/// The array implementation.
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq)]
pub struct Array<Allocator>(Listpack<Allocator>)
where
    Allocator: CustomAllocator;

impl<Allocator> Default for Array<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    fn default() -> Self {
        Self(Listpack::default())
    }
}

impl<Allocator> Clone for Array<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as redis_custom_allocator::CustomAllocator>::Error: std::fmt::Debug,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A> Deref for Array<A>
where
    A: CustomAllocator,
{
    type Target = Listpack<A>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<A> DerefMut for Array<A>
where
    A: CustomAllocator,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<Allocator> From<&[Value<Allocator>]> for Array<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as CustomAllocator>::Error: Debug,
{
    fn from(v: &[Value<Allocator>]) -> Self {
        Self(Listpack::from(v))
    }
}

impl<'a, Allocator> FromIterator<&'a Value<Allocator>> for Array<Allocator>
where
    Allocator: ListpackAllocator,
    <Allocator as CustomAllocator>::Error: Debug,
{
    fn from_iter<T: IntoIterator<Item = &'a Value<Allocator>>>(iter: T) -> Self {
        Self(Listpack::from_iter(iter))
    }
}

impl<A> MemoryConsumption for Array<A>
where
    A: CustomAllocator,
{
    fn memory_consumption(&self) -> usize {
        self.0.memory_consumption()
    }
}
