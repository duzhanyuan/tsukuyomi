//! Components for managing request-local data.

use std::any::TypeId;
use std::collections::hash_map::{self, HashMap};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::marker::PhantomData;

pub use crate::local_key;

/// A macro to create a `LocalKey<T>`.
///
/// # Examples
///
/// ```
/// #[macro_use]
/// extern crate tsukuyomi;
/// # use tsukuyomi::input::local_map::LocalMap;
///
/// # fn main() {
/// local_key!(static KEY: String);
///
/// let mut map = LocalMap::default();
/// map.entry(&KEY).or_insert("Alice".into());
/// # }
/// ```
#[macro_export]
macro_rules! local_key {
    ($(
        $(#[$m:meta])*
        $vis:vis static $NAME:ident : $t:ty;
    )*) => {$(
        $(#[$m])*
        $vis static $NAME: $crate::input::local_map::LocalKey<$t> = {
            fn __type_id() -> ::std::any::TypeId {
                struct __A;
                ::std::any::TypeId::of::<__A>()
            }
            $crate::input::local_map::LocalKey {
                __type_id,
                __marker: ::std::marker::PhantomData,
            }
        };
    )*};
    ($(
        $(#[$m:meta])*
        $vis:vis static $NAME:ident : $t:ty
    );+) => {
        local_key!($(
            $(#[$m])*
            $vis static $NAME: $t;
        )*);
    };

    ($(
        $(#[$m:meta])*
        $vis:vis const $NAME:ident : $t:ty;
    )*) => {$(
        $(#[$m])*
        $vis const $NAME: $crate::input::local_map::LocalKey<$t> = {
            fn __type_id() -> ::std::any::TypeId {
                struct __A;
                ::std::any::TypeId::of::<__A>()
            }
            $crate::input::local_map::LocalKey {
                __type_id,
                __marker: ::std::marker::PhantomData,
            }
        };
    )*};
    ($(
        $(#[$m:meta])*
        $vis:vis const $NAME:ident : $t:ty
    );+) => {
        local_key!($(
            $(#[$m])*
            $vis const $NAME: $t;
        )*);
    };
}

/// A type representing a key for request-local data stored in a `LocalMap`.
///
/// The value of this type are generated by the `local_key!` macro.
#[derive(Debug)]
pub struct LocalKey<T: Send + 'static> {
    // not a public API.
    #[doc(hidden)]
    pub __type_id: fn() -> TypeId,
    // not a public API.
    #[doc(hidden)]
    pub __marker: PhantomData<fn() -> T>,
}

impl<T: Send + 'static> LocalKey<T> {
    #[inline]
    fn type_id(&'static self) -> TypeId {
        (self.__type_id)()
    }
}

struct IdentHasher(u64);

impl Default for IdentHasher {
    fn default() -> Self {
        IdentHasher(0)
    }
}

impl Hasher for IdentHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u8(b);
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.0 = (self.0 << 8) | u64::from(i);
    }

    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}

trait Opaque: Send + 'static {}

impl<T: Send + 'static> Opaque for T {}

impl dyn Opaque {
    unsafe fn downcast_ref_unchecked<T: Send + 'static>(&self) -> &T {
        &*(self as *const dyn Opaque as *const T)
    }

    unsafe fn downcast_mut_unchecked<T: Send + 'static>(&mut self) -> &mut T {
        &mut *(self as *mut dyn Opaque as *mut T)
    }
}

trait BoxDowncastExt {
    unsafe fn downcast_unchecked<T: Send + 'static>(self) -> Box<T>;
}

#[cfg_attr(feature = "cargo-clippy", allow(use_self))]
impl BoxDowncastExt for Box<dyn Opaque> {
    unsafe fn downcast_unchecked<T: Send + 'static>(self) -> Box<T> {
        Box::from_raw(Box::into_raw(self) as *mut T)
    }
}

/// A typed map storing request-local data.
#[derive(Default)]
pub struct LocalMap {
    inner: HashMap<TypeId, Box<dyn Opaque>, BuildHasherDefault<IdentHasher>>,
}

#[cfg_attr(tarpaulin, skip)]
impl fmt::Debug for LocalMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalMap").finish()
    }
}

impl LocalMap {
    /// Returns a shared reference to the value corresponding to the provided `LocalKey`.
    pub fn get<T>(&self, key: &'static LocalKey<T>) -> Option<&T>
    where
        T: Send + 'static,
    {
        Some(unsafe { self.inner.get(&key.type_id())?.downcast_ref_unchecked() })
    }

    /// Returns a mutable reference to the value corresponding to the provided `LocalKey`.
    pub fn get_mut<T>(&mut self, key: &'static LocalKey<T>) -> Option<&mut T>
    where
        T: Send + 'static,
    {
        Some(unsafe { self.inner.get_mut(&key.type_id())?.downcast_mut_unchecked() })
    }

    /// Returns `true` if the map contains a value for the specified `LocalKey`.
    pub fn contains_key<T>(&self, key: &'static LocalKey<T>) -> bool
    where
        T: Send + 'static,
    {
        self.inner.contains_key(&key.type_id())
    }

    /// Inserts a value corresponding to the provided `LocalKey` into the map.
    pub fn insert<T>(&mut self, key: &'static LocalKey<T>, value: T) -> Option<T>
    where
        T: Send + 'static,
    {
        Some(unsafe {
            *self
                .inner
                .insert(key.type_id(), Box::new(value))?
                .downcast_unchecked()
        })
    }

    /// Removes a value corresponding to the provided `LocalKey` from the map.
    pub fn remove<T>(&mut self, key: &'static LocalKey<T>) -> Option<T>
    where
        T: Send + 'static,
    {
        Some(unsafe { *self.inner.remove(&key.type_id())?.downcast_unchecked() })
    }

    /// Create a `Entry` for in-place manipulation corresponds to an entry in the map.
    pub fn entry<T>(&mut self, key: &'static LocalKey<T>) -> Entry<'_, T>
    where
        T: Send + 'static,
    {
        match self.inner.entry(key.type_id()) {
            hash_map::Entry::Occupied(entry) => Entry::Occupied(OccupiedEntry {
                inner: entry,
                #[cfg_attr(tarpaulin, skip)]
                _marker: PhantomData,
            }),
            hash_map::Entry::Vacant(entry) => Entry::Vacant(VacantEntry {
                inner: entry,
                #[cfg_attr(tarpaulin, skip)]
                _marker: PhantomData,
            }),
        }
    }
}

/// A view into a single entry in a `LocalMap`.
#[derive(Debug)]
pub enum Entry<'a, T: Send + 'static> {
    /// An occupied entry.
    Occupied(OccupiedEntry<'a, T>),
    /// A vacant entry.
    Vacant(VacantEntry<'a, T>),
}

impl<'a, T> Entry<'a, T>
where
    T: Send + 'static,
{
    #[allow(missing_docs)]
    pub fn or_insert(self, default: T) -> &'a mut T {
        self.or_insert_with(|| default)
    }

    #[allow(missing_docs)]
    pub fn or_insert_with(self, default: impl FnOnce() -> T) -> &'a mut T {
        match self {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(default()),
        }
    }

    #[allow(missing_docs)]
    pub fn and_modify(self, f: impl FnOnce(&mut T)) -> Entry<'a, T> {
        match self {
            Entry::Occupied(mut entry) => {
                f(entry.get_mut());
                Entry::Occupied(entry)
            }
            Entry::Vacant(entry) => Entry::Vacant(entry),
        }
    }
}

/// An occupied entry.
pub struct OccupiedEntry<'a, T: Send + 'static> {
    inner: hash_map::OccupiedEntry<'a, TypeId, Box<dyn Opaque>>,
    _marker: PhantomData<T>,
}

#[cfg_attr(tarpaulin, skip)]
impl<'a, T> fmt::Debug for OccupiedEntry<'a, T>
where
    T: Send + 'static + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OccupiedEntry").field(self.get()).finish()
    }
}

#[allow(missing_docs)]
impl<'a, T> OccupiedEntry<'a, T>
where
    T: Send + 'static,
{
    pub fn get(&self) -> &T {
        unsafe { self.inner.get().downcast_ref_unchecked() }
    }

    pub fn get_mut(&mut self) -> &mut T {
        unsafe { self.inner.get_mut().downcast_mut_unchecked() }
    }

    pub fn into_mut(self) -> &'a mut T {
        unsafe { self.inner.into_mut().downcast_mut_unchecked() }
    }

    pub fn insert(&mut self, value: T) -> T {
        unsafe { *self.inner.insert(Box::new(value)).downcast_unchecked() }
    }

    pub fn remove(self) -> T {
        unsafe { *self.inner.remove().downcast_unchecked() }
    }
}

/// A vacant entry.
pub struct VacantEntry<'a, T: Send + 'static> {
    inner: hash_map::VacantEntry<'a, TypeId, Box<dyn Opaque>>,
    _marker: PhantomData<T>,
}

#[cfg_attr(tarpaulin, skip)]
impl<'a, T: Send + 'static> fmt::Debug for VacantEntry<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VacantEntry").finish()
    }
}

#[allow(missing_docs)]
impl<'a, T> VacantEntry<'a, T>
where
    T: Send + 'static,
{
    pub fn insert(self, default: T) -> &'a mut T {
        unsafe {
            self.inner
                .insert(Box::new(default))
                .downcast_mut_unchecked()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        let mut map = LocalMap::default();

        local_key!(static KEY: String);

        assert!(!map.contains_key(&KEY));

        assert!(map.insert(&KEY, String::from("foo")).is_none());
        assert!(map.contains_key(&KEY));

        assert_eq!(map.get(&KEY).map(String::as_str), Some("foo"));

        assert_eq!(map.insert(&KEY, String::from("bar")), Some("foo".into()));
        assert!(map.contains_key(&KEY));

        assert_eq!(map.get(&KEY).map(String::as_str), Some("bar"));

        assert_eq!(map.remove(&KEY), Some("bar".into()));
        assert!(!map.contains_key(&KEY));
    }

    #[test]
    fn entry_or_insert() {
        let mut map = LocalMap::default();

        local_key!(static KEY: String);

        map.entry(&KEY).or_insert("foo".into());
        assert_eq!(map.get(&KEY).map(String::as_str), Some("foo"));

        map.entry(&KEY).or_insert("bar".into());
        assert_eq!(map.get(&KEY).map(String::as_str), Some("foo"));
    }

    #[test]
    fn entry_and_modify() {
        let mut map = LocalMap::default();

        local_key!(static KEY: String);

        map.entry(&KEY).and_modify(|s| {
            *s += "foo";
        });
        assert!(!map.contains_key(&KEY));

        map.insert(&KEY, "foo".into());

        map.entry(&KEY).and_modify(|s| {
            *s += "bar";
        });
        assert_eq!(map.get(&KEY).map(String::as_str), Some("foobar"));

        map.entry(&KEY).and_modify(|s| {
            *s += "baz";
        });
        assert_eq!(map.get(&KEY).map(String::as_str), Some("foobarbaz"));
    }

    #[test]
    fn occupied_entry() {
        let mut map = LocalMap::default();

        local_key!(static KEY: String);

        map.insert(&KEY, "foo".into());

        if let Entry::Occupied(mut entry) = map.entry(&KEY) {
            assert_eq!(entry.get(), "foo");
            assert_eq!(entry.insert("bar".into()), "foo");
            assert_eq!(entry.get(), "bar");
            assert_eq!(entry.remove(), "bar");
        }

        assert!(!map.contains_key(&KEY));
    }

    #[test]
    fn local_key_const() {
        let mut map = LocalMap::default();
        local_key!(const KEY: String);
        map.insert(&KEY, "foo".into());
        assert!(map.contains_key(&KEY));
    }
}
