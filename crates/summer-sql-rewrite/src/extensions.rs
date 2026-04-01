use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};

type AnyMap = HashMap<TypeId, Box<dyn AnyClone + Send + Sync>, BuildHasherDefault<IdHasher>>;

#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId only writes u64");
    }

    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }
}

#[derive(Clone, Default)]
pub struct Extensions {
    map: Option<Box<AnyMap>>,
}

impl Extensions {
    pub fn new() -> Self {
        Self { map: None }
    }

    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .get_or_insert_with(Box::default)
            .insert(TypeId::of::<T>(), Box::new(val))
            .and_then(|boxed| boxed.into_any().downcast().ok().map(|value| *value))
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.map
            .as_ref()
            .and_then(|map| map.get(&TypeId::of::<T>()))
            .and_then(|boxed| (**boxed).as_any().downcast_ref())
    }

    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.map
            .as_mut()
            .and_then(|map| map.get_mut(&TypeId::of::<T>()))
            .and_then(|boxed| (**boxed).as_any_mut().downcast_mut())
    }

    pub fn get_or_insert<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> &mut T {
        self.get_or_insert_with(|| value)
    }

    pub fn get_or_insert_with<T: Clone + Send + Sync + 'static>(
        &mut self,
        f: impl FnOnce() -> T,
    ) -> &mut T {
        let out = self
            .map
            .get_or_insert_with(Box::default)
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(f()));
        (**out).as_any_mut().downcast_mut().unwrap()
    }

    pub fn get_or_insert_default<T: Default + Clone + Send + Sync + 'static>(&mut self) -> &mut T {
        self.get_or_insert_with(T::default)
    }

    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.map
            .as_mut()
            .and_then(|map| map.remove(&TypeId::of::<T>()))
            .and_then(|boxed| boxed.into_any().downcast().ok().map(|value| *value))
    }

    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.map
            .as_ref()
            .is_some_and(|map| map.contains_key(&TypeId::of::<T>()))
    }

    pub fn is_empty(&self) -> bool {
        self.map.as_ref().is_none_or(|map| map.is_empty())
    }

    pub fn len(&self) -> usize {
        self.map.as_ref().map_or(0, |map| map.len())
    }

    pub fn clear(&mut self) {
        if let Some(map) = &mut self.map {
            map.clear();
        }
    }

    pub fn extend(&mut self, other: Self) {
        if let Some(other) = other.map {
            if let Some(map) = &mut self.map {
                map.extend(*other);
            } else {
                self.map = Some(other);
            }
        }
    }
}

impl fmt::Debug for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct TypeName(&'static str);
        impl fmt::Debug for TypeName {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.0)
            }
        }

        let mut set = f.debug_set();
        if let Some(map) = &self.map {
            set.entries(
                map.values()
                    .map(|any_clone| TypeName(any_clone.as_ref().type_name())),
            );
        }
        set.finish()
    }
}

trait AnyClone: Any {
    fn clone_box(&self) -> Box<dyn AnyClone + Send + Sync>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
    fn type_name(&self) -> &'static str;
}

impl<T: Clone + Send + Sync + 'static> AnyClone for T {
    fn clone_box(&self) -> Box<dyn AnyClone + Send + Sync> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }

    fn type_name(&self) -> &'static str {
        type_name::<T>()
    }
}

impl Clone for Box<dyn AnyClone + Send + Sync> {
    fn clone(&self) -> Self {
        (**self).clone_box()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct UserId(i64);

    #[derive(Clone, Debug, PartialEq)]
    struct DeptId(i64);

    #[test]
    fn insert_and_clone_are_independent() {
        let mut ext = Extensions::new();
        ext.insert(UserId(42));
        ext.insert(DeptId(7));

        let copied = ext.clone();
        ext.insert(UserId(100));

        assert_eq!(ext.get::<UserId>(), Some(&UserId(100)));
        assert_eq!(copied.get::<UserId>(), Some(&UserId(42)));
        assert_eq!(copied.get::<DeptId>(), Some(&DeptId(7)));
    }

    #[test]
    fn extend_overwrites_same_type() {
        let mut left = Extensions::new();
        left.insert(1i32);
        let mut right = Extensions::new();
        right.insert(2i32);
        right.insert("hello");

        left.extend(right);

        assert_eq!(left.get::<i32>(), Some(&2));
        assert_eq!(left.get::<&'static str>(), Some(&"hello"));
    }
}
