use std::any::{Any, TypeId, type_name};
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};

type AnyMap = HashMap<TypeId, Box<dyn AnyClone + Send + Sync>, BuildHasherDefault<IdHasher>>;

/// 零成本哈希器。
///
/// `TypeId` 本身就是编译器生成的哈希值，无需再做任何位运算。
/// `IdHasher` 直接持有 `TypeId` 写入的 `u64`，`finish()` 时原样返回。
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId calls write_u64");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }
}

/// 类型安全的扩展容器，基于 `TypeId` 索引。
///
/// 与 `http::Extensions` 理念一致：每个具体类型只能存一个值，
/// 取值时通过泛型参数自动匹配，无需字符串 key，编译期保证类型安全。
///
/// # 设计亮点
///
/// - **懒初始化**：内部使用 `Option<Box<AnyMap>>`，
///   未插入任何数据时仅占 1 word（8 bytes），而非空 `HashMap` 的 3 words。
/// - **零成本哈希**：使用自定义 `IdHasher`，`TypeId` 本身就是哈希值，直接返回。
/// - **可 Clone**：所有存入的类型需实现 `Clone`，容器本身支持 `.clone()`。
/// - **Debug 输出类型名**：调试时显示存储的类型名称而非仅数量。
///
/// # 示例
///
/// ```rust
/// use summer_sharding::extensions::Extensions;
///
/// struct UserId(pub i64);
/// struct DeptId(pub i64);
///
/// let mut ext = Extensions::new();
/// ext.insert(UserId(42));
/// ext.insert(DeptId(7));
///
/// assert_eq!(ext.get::<UserId>().unwrap().0, 42);
/// assert_eq!(ext.get::<DeptId>().unwrap().0, 7);
/// ```
#[derive(Clone, Default)]
pub struct Extensions {
    // 懒初始化：未使用时为 None，仅占 1 word。
    map: Option<Box<AnyMap>>,
}

impl Extensions {
    /// 创建空容器。
    #[inline]
    pub fn new() -> Self {
        Self { map: None }
    }

    /// 插入一个值。若同类型已存在则覆盖，返回旧值。
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use summer_sharding::extensions::Extensions;
    /// let mut ext = Extensions::new();
    /// assert!(ext.insert(5i32).is_none());
    /// assert_eq!(ext.insert(9i32), Some(5i32));
    /// ```
    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .get_or_insert_with(Box::default)
            .insert(TypeId::of::<T>(), Box::new(val))
            .and_then(|boxed| boxed.into_any().downcast().ok().map(|v| *v))
    }

    /// 获取不可变引用。
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use summer_sharding::extensions::Extensions;
    /// let mut ext = Extensions::new();
    /// ext.insert(5i32);
    /// assert_eq!(ext.get::<i32>(), Some(&5i32));
    /// assert_eq!(ext.get::<bool>(), None);
    /// ```
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.map
            .as_ref()
            .and_then(|map| map.get(&TypeId::of::<T>()))
            .and_then(|boxed| (**boxed).as_any().downcast_ref())
    }

    /// 获取可变引用。
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use summer_sharding::extensions::Extensions;
    /// let mut ext = Extensions::new();
    /// ext.insert(String::from("Hello"));
    /// ext.get_mut::<String>().unwrap().push_str(" World");
    /// assert_eq!(ext.get::<String>().unwrap(), "Hello World");
    /// ```
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.map
            .as_mut()
            .and_then(|map| map.get_mut(&TypeId::of::<T>()))
            .and_then(|boxed| (**boxed).as_any_mut().downcast_mut())
    }

    /// 获取可变引用；若不存在则插入 `value`。
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use summer_sharding::extensions::Extensions;
    /// let mut ext = Extensions::new();
    /// *ext.get_or_insert(1i32) += 2;
    /// assert_eq!(*ext.get::<i32>().unwrap(), 3);
    /// ```
    pub fn get_or_insert<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> &mut T {
        self.get_or_insert_with(|| value)
    }

    /// 获取可变引用；若不存在则调用 `f` 创建并插入。
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use summer_sharding::extensions::Extensions;
    /// let mut ext = Extensions::new();
    /// *ext.get_or_insert_with(|| 1i32) += 2;
    /// assert_eq!(*ext.get::<i32>().unwrap(), 3);
    /// ```
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

    /// 获取可变引用；若不存在则插入类型的 `Default` 值。
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use summer_sharding::extensions::Extensions;
    /// let mut ext = Extensions::new();
    /// *ext.get_or_insert_default::<i32>() += 2;
    /// assert_eq!(*ext.get::<i32>().unwrap(), 2);
    /// ```
    pub fn get_or_insert_default<T: Default + Clone + Send + Sync + 'static>(&mut self) -> &mut T {
        self.get_or_insert_with(T::default)
    }

    /// 移除并返回指定类型的值。
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use summer_sharding::extensions::Extensions;
    /// let mut ext = Extensions::new();
    /// ext.insert(5i32);
    /// assert_eq!(ext.remove::<i32>(), Some(5i32));
    /// assert!(ext.get::<i32>().is_none());
    /// ```
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.map
            .as_mut()
            .and_then(|map| map.remove(&TypeId::of::<T>()))
            .and_then(|boxed| boxed.into_any().downcast().ok().map(|v| *v))
    }

    /// 是否包含指定类型。
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.map
            .as_ref()
            .is_some_and(|map| map.contains_key(&TypeId::of::<T>()))
    }

    /// 容器是否为空。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.as_ref().is_none_or(|map| map.is_empty())
    }

    /// 已存储的类型数量。
    #[inline]
    pub fn len(&self) -> usize {
        self.map.as_ref().map_or(0, |map| map.len())
    }

    /// 清空所有扩展数据。
    #[inline]
    pub fn clear(&mut self) {
        if let Some(ref mut map) = self.map {
            map.clear();
        }
    }

    /// 将另一个 `Extensions` 的所有数据合并到 `self` 中。
    /// 同类型冲突时，`other` 中的值会覆盖 `self` 中的值。
    ///
    /// # 示例
    ///
    /// ```rust
    /// # use summer_sharding::extensions::Extensions;
    /// let mut a = Extensions::new();
    /// a.insert(8u8);
    /// a.insert(16u16);
    ///
    /// let mut b = Extensions::new();
    /// b.insert(4u8);
    /// b.insert("hello");
    ///
    /// a.extend(b);
    /// assert_eq!(a.len(), 3);
    /// assert_eq!(a.get::<u8>(), Some(&4u8));    // 被覆盖
    /// assert_eq!(a.get::<u16>(), Some(&16u16)); // 保留
    /// ```
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

// ---------------------------------------------------------------------------
// AnyClone：让 Box<dyn Any> 支持 Clone
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct UserId(i64);

    #[derive(Clone, Debug, PartialEq)]
    struct DeptId(i64);

    #[derive(Clone, Debug, PartialEq)]
    struct Role(String);

    #[test]
    fn empty_extensions() {
        let ext = Extensions::new();
        assert!(ext.is_empty());
        assert_eq!(ext.len(), 0);
        assert_eq!(ext.get::<i32>(), None);
    }

    #[test]
    fn insert_and_get() {
        let mut ext = Extensions::new();
        assert!(ext.insert(UserId(42)).is_none());
        assert!(ext.insert(DeptId(7)).is_none());

        assert_eq!(ext.get::<UserId>(), Some(&UserId(42)));
        assert_eq!(ext.get::<DeptId>(), Some(&DeptId(7)));
        assert_eq!(ext.len(), 2);
    }

    #[test]
    fn insert_overwrites_same_type() {
        let mut ext = Extensions::new();
        assert!(ext.insert(5i32).is_none());
        assert_eq!(ext.insert(9i32), Some(5i32));
        assert_eq!(ext.get::<i32>(), Some(&9i32));
    }

    #[test]
    fn get_mut() {
        let mut ext = Extensions::new();
        ext.insert(String::from("Hello"));
        ext.get_mut::<String>().unwrap().push_str(" World");
        assert_eq!(ext.get::<String>().unwrap(), "Hello World");
    }

    #[test]
    fn remove() {
        let mut ext = Extensions::new();
        ext.insert(UserId(42));
        assert_eq!(ext.remove::<UserId>(), Some(UserId(42)));
        assert!(ext.get::<UserId>().is_none());
        assert!(ext.is_empty());
    }

    #[test]
    fn contains() {
        let mut ext = Extensions::new();
        assert!(!ext.contains::<UserId>());
        ext.insert(UserId(1));
        assert!(ext.contains::<UserId>());
        assert!(!ext.contains::<DeptId>());
    }

    #[test]
    fn clone_is_independent() {
        let mut ext = Extensions::new();
        ext.insert(UserId(42));
        ext.insert(DeptId(7));

        let ext2 = ext.clone();

        // 修改原始不影响克隆
        ext.insert(UserId(100));
        assert_eq!(ext.get::<UserId>(), Some(&UserId(100)));
        assert_eq!(ext2.get::<UserId>(), Some(&UserId(42)));

        // 克隆保持独立
        assert_eq!(ext2.get::<DeptId>(), Some(&DeptId(7)));
    }

    #[test]
    fn get_or_insert() {
        let mut ext = Extensions::new();
        *ext.get_or_insert(1i32) += 2;
        assert_eq!(*ext.get::<i32>().unwrap(), 3);

        // 已存在时不覆盖
        *ext.get_or_insert(100i32) += 1;
        assert_eq!(*ext.get::<i32>().unwrap(), 4);
    }

    #[test]
    fn get_or_insert_with() {
        let mut ext = Extensions::new();
        let value = ext.get_or_insert_with(|| Role("admin".to_string()));
        assert_eq!(value, &Role("admin".to_string()));
    }

    #[test]
    fn get_or_insert_default() {
        let mut ext = Extensions::new();
        *ext.get_or_insert_default::<i32>() += 5;
        assert_eq!(*ext.get::<i32>().unwrap(), 5);
    }

    #[test]
    fn extend_merges() {
        let mut a = Extensions::new();
        a.insert(8u8);
        a.insert(16u16);

        let mut b = Extensions::new();
        b.insert(4u8); // 覆盖 a 的 u8
        b.insert("hello");

        a.extend(b);
        assert_eq!(a.len(), 3);
        assert_eq!(a.get::<u8>(), Some(&4u8));
        assert_eq!(a.get::<u16>(), Some(&16u16));
        assert_eq!(a.get::<&'static str>().copied(), Some("hello"));
    }

    #[test]
    fn extend_into_empty() {
        let mut a = Extensions::new();
        let mut b = Extensions::new();
        b.insert(42i32);

        a.extend(b);
        assert_eq!(a.get::<i32>(), Some(&42i32));
    }

    #[test]
    fn extend_empty_into_existing() {
        let mut a = Extensions::new();
        a.insert(42i32);

        let b = Extensions::new();
        a.extend(b);
        assert_eq!(a.get::<i32>(), Some(&42i32));
    }

    #[test]
    fn clear() {
        let mut ext = Extensions::new();
        ext.insert(1i32);
        ext.insert("hello");
        ext.clear();
        assert!(ext.is_empty());
        assert_eq!(ext.len(), 0);
    }

    #[test]
    fn debug_shows_type_names() {
        let mut ext = Extensions::new();
        assert_eq!(format!("{ext:?}"), "{}");

        ext.insert(UserId(1));
        let dbg = format!("{ext:?}");
        assert!(
            dbg.contains("UserId"),
            "Debug output should contain type name, got: {dbg}"
        );
    }

    #[test]
    fn multiple_distinct_types() {
        let mut ext = Extensions::new();
        ext.insert(1i32);
        ext.insert(2u32);
        ext.insert(3i64);
        ext.insert(4u64);
        ext.insert(true);
        ext.insert("hello");
        ext.insert(String::from("world"));

        assert_eq!(ext.len(), 7);
        assert_eq!(ext.get::<i32>(), Some(&1));
        assert_eq!(ext.get::<u32>(), Some(&2));
        assert_eq!(ext.get::<i64>(), Some(&3));
        assert_eq!(ext.get::<u64>(), Some(&4));
        assert_eq!(ext.get::<bool>(), Some(&true));
        assert_eq!(ext.get::<&str>(), Some(&"hello"));
        assert_eq!(ext.get::<String>(), Some(&String::from("world")));
    }
}
