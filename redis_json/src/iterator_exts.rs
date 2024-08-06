use std::collections::HashMap;
use std::hash::Hash;

pub trait HashIterator: Sized + Iterator {
    type H: FromIterator<Self::Item>;
}

impl<T1: Eq + Hash, T2, T: Iterator<Item = (T1, T2)>> HashIterator for T {
    type H = HashMap<T1, T2>;
}

pub trait ResultHashIterator: Sized + Iterator {
    type H: FromIterator<Self::Item>;
}

impl<T1: Eq + Hash, T2, E, T: Iterator<Item = Result<(T1, T2), E>>> ResultHashIterator for T {
    type H = Result<HashMap<T1, T2>, E>;
}

pub trait IteratorExts: Iterator {
    fn to_vec(self) -> Vec<Self::Item>
    where
        Self: Sized,
    {
        self.collect::<Vec<_>>()
    }
    fn try_vec<T, E>(self) -> Result<Vec<T>, E>
    where
        Self: Sized,
        Result<Vec<T>, E>: FromIterator<Self::Item>,
    {
        self.collect::<Result<Vec<T>, E>>()
    }
    fn to_hashmap(self) -> Self::H
    where
        Self: HashIterator,
    {
        self.collect::<Self::H>()
    }
    fn try_hashmap(self) -> Self::H
    where
        Self: ResultHashIterator,
    {
        self.collect::<Self::H>()
    }
}

impl<T: Iterator> IteratorExts for T {}
