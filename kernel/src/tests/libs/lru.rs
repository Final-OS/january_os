use super::{fail, pass};
use crate::kprintln;
use alloc::vec::Vec;

pub(super) fn run() {
    kprintln!("  [LRU] Starting basic tests...");
    test_lru_basic();
    kprintln!("  [LRU] Starting capacity tests...");
    test_lru_capacity();
    kprintln!("  [LRU] Starting ordering tests...");
    test_lru_ordering();
    kprintln!("  [LRU] Starting operations tests...");
    test_lru_operations();
    kprintln!("  [LRU] Starting iteration tests...");
    test_lru_iteration();
    kprintln!("  [LRU] Starting edge case tests...");
    test_lru_edge_cases();
    pass("lru");
}

fn test_lru_basic() {
    use crate::libs::lru::LruCache;

    // 基本创建和属性
    let mut cache = LruCache::new(3);
    if cache.capacity() != 3 {
        return fail("lru/basic", "capacity != 3");
    }
    if !cache.is_empty() {
        return fail("lru/basic", "should be empty");
    }
    if cache.len() != 0 {
        return fail("lru/basic", "len != 0");
    }

    // 插入元素
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");
    if cache.len() != 3 {
        return fail("lru/basic", "len != 3 after puts");
    }
    if cache.is_empty() {
        return fail("lru/basic", "should not be empty");
    }

    // contains_key
    if !cache.contains_key(&1) {
        return fail("lru/basic", "should contain key 1");
    }
    if !cache.contains_key(&2) {
        return fail("lru/basic", "should contain key 2");
    }
    if cache.contains_key(&99) {
        return fail("lru/basic", "should not contain key 99");
    }

    // get
    if cache.get(&1) != Some(&"a") {
        return fail("lru/basic", "get(1) failed");
    }
    if cache.get(&2) != Some(&"b") {
        return fail("lru/basic", "get(2) failed");
    }
    if cache.get(&99).is_some() {
        return fail("lru/basic", "get(99) should be None");
    }

    // peek (不改变顺序)
    if cache.peek(&3) != Some(&"c") {
        return fail("lru/basic", "peek(3) failed");
    }

    // clear
    cache.clear();
    if !cache.is_empty() {
        return fail("lru/basic", "should be empty after clear");
    }
    if cache.len() != 0 {
        return fail("lru/basic", "len != 0 after clear");
    }
}

fn test_lru_capacity() {
    use crate::libs::lru::LruCache;

    // 测试容量限制和淘汰
    let mut cache = LruCache::new(3);
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");

    // 访问 1，使其成为 MRU
    cache.get(&1);

    // 插入 4，应淘汰 LRU (key=2)
    cache.put(4, "d");
    if cache.len() != 3 {
        return fail("lru/capacity", "len != 3 after eviction");
    }
    if cache.contains_key(&2) {
        return fail("lru/capacity", "key 2 should be evicted");
    }
    if !cache.contains_key(&1) {
        return fail("lru/capacity", "key 1 should remain");
    }
    if !cache.contains_key(&3) {
        return fail("lru/capacity", "key 3 should remain");
    }
    if !cache.contains_key(&4) {
        return fail("lru/capacity", "key 4 should exist");
    }

    // 测试 resize
    let evicted = cache.resize(2);
    if cache.capacity() != 2 {
        return fail("lru/capacity", "capacity != 2 after resize");
    }
    if cache.len() != 2 {
        return fail("lru/capacity", "len != 2 after resize");
    }
    if evicted.len() != 1 {
        return fail("lru/capacity", "should evict 1 item");
    }

    // 扩大容量
    cache.resize(5);
    if cache.capacity() != 5 {
        return fail("lru/capacity", "capacity != 5 after expand");
    }
    if cache.len() != 2 {
        return fail("lru/capacity", "len should remain 2");
    }
}

fn test_lru_ordering() {
    use crate::libs::lru::LruCache;

    let mut cache = LruCache::new(3);
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");

    // 检查 MRU/LRU
    if cache.peek_mru_key() != Some(&3) {
        return fail("lru/ordering", "MRU should be 3");
    }
    if cache.peek_lru_key() != Some(&1) {
        return fail("lru/ordering", "LRU should be 1");
    }

    // 访问 1，使其成为 MRU
    cache.get(&1);
    if cache.peek_mru_key() != Some(&1) {
        return fail("lru/ordering", "MRU should be 1 after get");
    }
    if cache.peek_lru_key() != Some(&2) {
        return fail("lru/ordering", "LRU should be 2 after get");
    }

    // promote
    if !cache.promote(&2) {
        return fail("lru/ordering", "promote should succeed");
    }
    if cache.peek_mru_key() != Some(&2) {
        return fail("lru/ordering", "MRU should be 2 after promote");
    }

    // pop_lru
    let lru = cache.pop_lru();
    if lru != Some((3, "c")) {
        return fail("lru/ordering", "pop_lru should return (3, c)");
    }
    if cache.len() != 2 {
        return fail("lru/ordering", "len != 2 after pop_lru");
    }

    // pop_mru
    let mru = cache.pop_mru();
    if mru != Some((2, "b")) {
        return fail("lru/ordering", "pop_mru should return (2, b)");
    }
    if cache.len() != 1 {
        return fail("lru/ordering", "len != 1 after pop_mru");
    }
}

fn test_lru_operations() {
    use crate::libs::lru::LruCache;

    let mut cache = LruCache::new(3);

    // get_or_insert
    let val = cache.get_or_insert(1, "a");
    if *val != "a" {
        return fail("lru/ops", "get_or_insert failed");
    }
    if cache.len() != 1 {
        return fail("lru/ops", "len != 1 after get_or_insert");
    }

    // get_or_insert 已存在的键
    let val = cache.get_or_insert(1, "b");
    if *val != "a" {
        return fail("lru/ops", "get_or_insert should return existing");
    }

    // get_or_insert_with
    let val = cache.get_or_insert_with(2, || "b");
    if *val != "b" {
        return fail("lru/ops", "get_or_insert_with failed");
    }

    // put_if_absent
    let result = cache.put_if_absent(3, "c");
    if result.is_err() {
        return fail("lru/ops", "put_if_absent should succeed");
    }

    let result = cache.put_if_absent(3, "d");
    if result.is_ok() {
        return fail("lru/ops", "put_if_absent should fail for existing key");
    }

    // get_mut
    if let Some(val) = cache.get_mut(&1) {
        *val = "modified";
    } else {
        return fail("lru/ops", "get_mut failed");
    }
    if cache.get(&1) != Some(&"modified") {
        return fail("lru/ops", "modification failed");
    }

    // peek_mut
    if let Some(val) = cache.peek_mut(&2) {
        *val = "peeked";
    } else {
        return fail("lru/ops", "peek_mut failed");
    }
    if cache.peek(&2) != Some(&"peeked") {
        return fail("lru/ops", "peek_mut modification failed");
    }

    // remove
    let removed = cache.remove(&1);
    if removed != Some("modified") {
        return fail("lru/ops", "remove failed");
    }
    if cache.contains_key(&1) {
        return fail("lru/ops", "key should be removed");
    }

    // 更新已存在的键
    cache.put(2, "updated");
    if cache.get(&2) != Some(&"updated") {
        return fail("lru/ops", "update failed");
    }
}

fn test_lru_iteration() {
    use crate::libs::lru::LruCache;

    let mut cache = LruCache::new(5);
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");

    // iter (MRU to LRU)
    let items: Vec<_> = cache.iter().map(|(k, v)| (*k, *v)).collect();
    if items.len() != 3 {
        return fail("lru/iter", "iter len != 3");
    }
    if items[0] != (3, "c") {
        return fail("lru/iter", "first should be MRU");
    }
    if items[2] != (1, "a") {
        return fail("lru/iter", "last should be LRU");
    }

    // iter_lru (LRU to MRU)
    let items: Vec<_> = cache.iter_lru().map(|(k, v)| (*k, *v)).collect();
    if items[0] != (1, "a") {
        return fail("lru/iter", "iter_lru first should be LRU");
    }
    if items[2] != (3, "c") {
        return fail("lru/iter", "iter_lru last should be MRU");
    }

    // keys
    let keys: Vec<_> = cache.keys().copied().collect();
    if keys.len() != 3 {
        return fail("lru/iter", "keys len != 3");
    }

    // values
    let values: Vec<_> = cache.values().copied().collect();
    if values.len() != 3 {
        return fail("lru/iter", "values len != 3");
    }
}

fn test_lru_edge_cases() {
    use crate::libs::lru::LruCache;

    // 容量为 1
    let mut cache = LruCache::new(1);
    cache.put(1, "a");
    cache.put(2, "b");
    if cache.len() != 1 {
        return fail("lru/edge", "capacity 1: len != 1");
    }
    if cache.contains_key(&1) {
        return fail("lru/edge", "capacity 1: key 1 should be evicted");
    }
    if !cache.contains_key(&2) {
        return fail("lru/edge", "capacity 1: key 2 should exist");
    }

    // 空缓存操作
    let mut cache = LruCache::<u32, &str>::new(3);
    if cache.get(&1).is_some() {
        return fail("lru/edge", "empty: get should be None");
    }
    if cache.pop_lru().is_some() {
        return fail("lru/edge", "empty: pop_lru should be None");
    }
    if cache.pop_mru().is_some() {
        return fail("lru/edge", "empty: pop_mru should be None");
    }
    if cache.peek_mru_key().is_some() {
        return fail("lru/edge", "empty: peek_mru_key should be None");
    }
    if cache.peek_lru_key().is_some() {
        return fail("lru/edge", "empty: peek_lru_key should be None");
    }

    // 重复插入相同键
    let mut cache = LruCache::new(3);
    let (old, evicted) = cache.put(1, "a");
    if old.is_some() {
        return fail("lru/edge", "first put should return None");
    }
    if evicted.is_some() {
        return fail("lru/edge", "first put should not evict");
    }

    let (old, evicted) = cache.put(1, "b");
    if old != Some("a") {
        return fail("lru/edge", "update should return old value");
    }
    if evicted.is_some() {
        return fail("lru/edge", "update should not evict");
    }
    if cache.len() != 1 {
        return fail("lru/edge", "len should remain 1 after update");
    }
}
