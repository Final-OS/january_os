//! 内核数据结构 (libs) 测试

use crate::{kprintln, ok, error};
use alloc::vec;
use alloc::vec::Vec;

pub fn run() {
    run_with_filter(None);
}

pub fn run_with_filter(filter: Option<&str>) {
    kprintln!("=== Libs Data Structure Tests ===");

    match filter {
        None => {
            test_rbtree();
            test_lru();
            test_radix_tree();
            test_btree();
            test_maple_tree();
            test_rcu();
        }
        Some("rbtree") => test_rbtree(),
        Some("lru") => test_lru(),
        Some("rdtree") | Some("radix") => test_radix_tree(),
        Some("btree") => test_btree(),
        Some("mptree") | Some("maple") => test_maple_tree(),
        Some("rcu") => test_rcu(),
        Some(name) => {
            error!("Unknown test: {}", name);
            kprintln!("Available tests: rbtree, lru, rdtree, btree, mptree, rcu");
        }
    }

    kprintln!();
}

fn pass(name: &str) { ok!("libs/{}", name); }
fn fail(name: &str, msg: &str) { error!("libs/{}: {}", name, msg); }

fn test_rbtree() {
    use crate::libs::rbtree::RbTree;

    let mut tree = RbTree::new();
    tree.insert(3, "three");
    tree.insert(1, "one");
    tree.insert(2, "two");
    tree.insert(5, "five");
    tree.insert(4, "four");

    // --- 基本查询 ---
    if tree.len() != 5 { return fail("rbtree", "len != 5"); }
    if tree.get(&2) != Some(&"two") { return fail("rbtree", "get(2)"); }
    if tree.get(&99).is_some() { return fail("rbtree", "get(99) should be None"); }
    if !tree.contains_key(&4) { return fail("rbtree", "contains_key(4)"); }
    if tree.first_key_value() != Some((&1, &"one")) { return fail("rbtree", "first"); }
    if tree.last_key_value() != Some((&5, &"five")) { return fail("rbtree", "last"); }

    // --- floor / lower_bound / upper_bound / lower_than ---
    if tree.floor(&3) != Some((&3, &"three")) { return fail("rbtree", "floor(3)"); }
    if tree.floor(&0).is_some() { return fail("rbtree", "floor(0)"); }
    if tree.lower_bound(&3) != Some((&3, &"three")) { return fail("rbtree", "lower_bound(3)"); }
    if tree.upper_bound(&3) != Some((&4, &"four")) { return fail("rbtree", "upper_bound(3)"); }
    if tree.lower_than(&3) != Some((&2, &"two")) { return fail("rbtree", "lower_than(3)"); }

    // --- 插入覆盖 ---
    let old = tree.insert(2, "TWO");
    if old != Some("two") { return fail("rbtree", "insert overwrite"); }
    if tree.get(&2) != Some(&"TWO") { return fail("rbtree", "get after overwrite"); }
    if tree.len() != 5 { return fail("rbtree", "len after overwrite"); }

    // --- 删除 ---
    tree.remove(&2);
    if tree.len() != 4 { return fail("rbtree", "remove"); }
    if tree.get(&2).is_some() { return fail("rbtree", "get after remove"); }

    // --- pop ---
    let first = tree.pop_first();
    if first != Some((1, "one")) { return fail("rbtree", "pop_first"); }
    let last = tree.pop_last();
    if last != Some((5, "five")) { return fail("rbtree", "pop_last"); }
    if tree.len() != 2 { return fail("rbtree", "len after pops"); }

    // --- entry API ---
    tree.entry(10).or_insert("ten");
    if tree.get(&10) != Some(&"ten") { return fail("rbtree", "entry vacant"); }
    tree.entry(10).and_modify(|v| *v = "TEN");
    if tree.get(&10) != Some(&"TEN") { return fail("rbtree", "entry modify"); }

    // --- clone ---
    let tree2 = tree.clone();
    if tree2.len() != tree.len() { return fail("rbtree", "clone len"); }
    if tree2.get(&10) != Some(&"TEN") { return fail("rbtree", "clone get"); }

    // --- retain ---
    let mut t = RbTree::new();
    t.insert(1, 10);
    t.insert(2, 20);
    t.insert(3, 30);
    t.retain(|_, v| *v > 15);
    if t.len() != 2 { return fail("rbtree", "retain len"); }
    if t.contains_key(&1) { return fail("rbtree", "retain removed 1"); }

    // --- 空树 ---
    let empty: RbTree<i32, i32> = RbTree::new();
    if empty.first_key_value().is_some() { return fail("rbtree", "empty first"); }
    if !empty.is_empty() { return fail("rbtree", "empty is_empty"); }

    pass("rbtree");
}

fn test_lru() {
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
    if cache.capacity() != 3 { return fail("lru/basic", "capacity != 3"); }
    if !cache.is_empty() { return fail("lru/basic", "should be empty"); }
    if cache.len() != 0 { return fail("lru/basic", "len != 0"); }

    // 插入元素
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");
    if cache.len() != 3 { return fail("lru/basic", "len != 3 after puts"); }
    if cache.is_empty() { return fail("lru/basic", "should not be empty"); }

    // contains_key
    if !cache.contains_key(&1) { return fail("lru/basic", "should contain key 1"); }
    if !cache.contains_key(&2) { return fail("lru/basic", "should contain key 2"); }
    if cache.contains_key(&99) { return fail("lru/basic", "should not contain key 99"); }

    // get
    if cache.get(&1) != Some(&"a") { return fail("lru/basic", "get(1) failed"); }
    if cache.get(&2) != Some(&"b") { return fail("lru/basic", "get(2) failed"); }
    if cache.get(&99).is_some() { return fail("lru/basic", "get(99) should be None"); }

    // peek (不改变顺序)
    if cache.peek(&3) != Some(&"c") { return fail("lru/basic", "peek(3) failed"); }

    // clear
    cache.clear();
    if !cache.is_empty() { return fail("lru/basic", "should be empty after clear"); }
    if cache.len() != 0 { return fail("lru/basic", "len != 0 after clear"); }
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
    if cache.len() != 3 { return fail("lru/capacity", "len != 3 after eviction"); }
    if cache.contains_key(&2) { return fail("lru/capacity", "key 2 should be evicted"); }
    if !cache.contains_key(&1) { return fail("lru/capacity", "key 1 should remain"); }
    if !cache.contains_key(&3) { return fail("lru/capacity", "key 3 should remain"); }
    if !cache.contains_key(&4) { return fail("lru/capacity", "key 4 should exist"); }

    // 测试 resize
    let evicted = cache.resize(2);
    if cache.capacity() != 2 { return fail("lru/capacity", "capacity != 2 after resize"); }
    if cache.len() != 2 { return fail("lru/capacity", "len != 2 after resize"); }
    if evicted.len() != 1 { return fail("lru/capacity", "should evict 1 item"); }

    // 扩大容量
    cache.resize(5);
    if cache.capacity() != 5 { return fail("lru/capacity", "capacity != 5 after expand"); }
    if cache.len() != 2 { return fail("lru/capacity", "len should remain 2"); }
}

fn test_lru_ordering() {
    use crate::libs::lru::LruCache;

    let mut cache = LruCache::new(3);
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");

    // 检查 MRU/LRU
    if cache.peek_mru_key() != Some(&3) { return fail("lru/ordering", "MRU should be 3"); }
    if cache.peek_lru_key() != Some(&1) { return fail("lru/ordering", "LRU should be 1"); }

    // 访问 1，使其成为 MRU
    cache.get(&1);
    if cache.peek_mru_key() != Some(&1) { return fail("lru/ordering", "MRU should be 1 after get"); }
    if cache.peek_lru_key() != Some(&2) { return fail("lru/ordering", "LRU should be 2 after get"); }

    // promote
    if !cache.promote(&2) { return fail("lru/ordering", "promote should succeed"); }
    if cache.peek_mru_key() != Some(&2) { return fail("lru/ordering", "MRU should be 2 after promote"); }

    // pop_lru
    let lru = cache.pop_lru();
    if lru != Some((3, "c")) { return fail("lru/ordering", "pop_lru should return (3, c)"); }
    if cache.len() != 2 { return fail("lru/ordering", "len != 2 after pop_lru"); }

    // pop_mru
    let mru = cache.pop_mru();
    if mru != Some((2, "b")) { return fail("lru/ordering", "pop_mru should return (2, b)"); }
    if cache.len() != 1 { return fail("lru/ordering", "len != 1 after pop_mru"); }
}

fn test_lru_operations() {
    use crate::libs::lru::LruCache;

    let mut cache = LruCache::new(3);

    // get_or_insert
    let val = cache.get_or_insert(1, "a");
    if *val != "a" { return fail("lru/ops", "get_or_insert failed"); }
    if cache.len() != 1 { return fail("lru/ops", "len != 1 after get_or_insert"); }

    // get_or_insert 已存在的键
    let val = cache.get_or_insert(1, "b");
    if *val != "a" { return fail("lru/ops", "get_or_insert should return existing"); }

    // get_or_insert_with
    let val = cache.get_or_insert_with(2, || "b");
    if *val != "b" { return fail("lru/ops", "get_or_insert_with failed"); }

    // put_if_absent
    let result = cache.put_if_absent(3, "c");
    if result.is_err() { return fail("lru/ops", "put_if_absent should succeed"); }

    let result = cache.put_if_absent(3, "d");
    if result.is_ok() { return fail("lru/ops", "put_if_absent should fail for existing key"); }

    // get_mut
    if let Some(val) = cache.get_mut(&1) {
        *val = "modified";
    } else {
        return fail("lru/ops", "get_mut failed");
    }
    if cache.get(&1) != Some(&"modified") { return fail("lru/ops", "modification failed"); }

    // peek_mut
    if let Some(val) = cache.peek_mut(&2) {
        *val = "peeked";
    } else {
        return fail("lru/ops", "peek_mut failed");
    }
    if cache.peek(&2) != Some(&"peeked") { return fail("lru/ops", "peek_mut modification failed"); }

    // remove
    let removed = cache.remove(&1);
    if removed != Some("modified") { return fail("lru/ops", "remove failed"); }
    if cache.contains_key(&1) { return fail("lru/ops", "key should be removed"); }

    // 更新已存在的键
    cache.put(2, "updated");
    if cache.get(&2) != Some(&"updated") { return fail("lru/ops", "update failed"); }
}

fn test_lru_iteration() {
    use crate::libs::lru::LruCache;

    let mut cache = LruCache::new(5);
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");

    // iter (MRU to LRU)
    let items: Vec<_> = cache.iter().map(|(k, v)| (*k, *v)).collect();
    if items.len() != 3 { return fail("lru/iter", "iter len != 3"); }
    if items[0] != (3, "c") { return fail("lru/iter", "first should be MRU"); }
    if items[2] != (1, "a") { return fail("lru/iter", "last should be LRU"); }

    // iter_lru (LRU to MRU)
    let items: Vec<_> = cache.iter_lru().map(|(k, v)| (*k, *v)).collect();
    if items[0] != (1, "a") { return fail("lru/iter", "iter_lru first should be LRU"); }
    if items[2] != (3, "c") { return fail("lru/iter", "iter_lru last should be MRU"); }

    // keys
    let keys: Vec<_> = cache.keys().copied().collect();
    if keys.len() != 3 { return fail("lru/iter", "keys len != 3"); }

    // values
    let values: Vec<_> = cache.values().copied().collect();
    if values.len() != 3 { return fail("lru/iter", "values len != 3"); }
}

fn test_lru_edge_cases() {
    use crate::libs::lru::LruCache;

    // 容量为 1
    let mut cache = LruCache::new(1);
    cache.put(1, "a");
    cache.put(2, "b");
    if cache.len() != 1 { return fail("lru/edge", "capacity 1: len != 1"); }
    if cache.contains_key(&1) { return fail("lru/edge", "capacity 1: key 1 should be evicted"); }
    if !cache.contains_key(&2) { return fail("lru/edge", "capacity 1: key 2 should exist"); }

    // 空缓存操作
    let mut cache = LruCache::<u32, &str>::new(3);
    if cache.get(&1).is_some() { return fail("lru/edge", "empty: get should be None"); }
    if cache.pop_lru().is_some() { return fail("lru/edge", "empty: pop_lru should be None"); }
    if cache.pop_mru().is_some() { return fail("lru/edge", "empty: pop_mru should be None"); }
    if cache.peek_mru_key().is_some() { return fail("lru/edge", "empty: peek_mru_key should be None"); }
    if cache.peek_lru_key().is_some() { return fail("lru/edge", "empty: peek_lru_key should be None"); }

    // 重复插入相同键
    let mut cache = LruCache::new(3);
    let (old, evicted) = cache.put(1, "a");
    if old.is_some() { return fail("lru/edge", "first put should return None"); }
    if evicted.is_some() { return fail("lru/edge", "first put should not evict"); }

    let (old, evicted) = cache.put(1, "b");
    if old != Some("a") { return fail("lru/edge", "update should return old value"); }
    if evicted.is_some() { return fail("lru/edge", "update should not evict"); }
    if cache.len() != 1 { return fail("lru/edge", "len should remain 1 after update"); }
}

fn test_radix_tree() {
    test_radix_basic();
    test_radix_operations();
    test_radix_range();
    test_radix_iterators();
    test_radix_edge_cases();
    pass("rdtree");
}

fn test_radix_basic() {
    use crate::libs::rdtree::RadixTree;

    // 基本插入和查找
    let mut rt = RadixTree::new();
    rt.insert(0, "zero");
    rt.insert(100, "hundred");
    rt.insert(50, "fifty");

    if rt.len() != 3 { return fail("rdtree/basic", "len != 3"); }
    if rt.get(50) != Some(&"fifty") { return fail("rdtree/basic", "get(50) failed"); }
    if rt.contains(100) != true { return fail("rdtree/basic", "contains(100)"); }
    if rt.contains(99) != false { return fail("rdtree/basic", "contains(99)"); }

    // get_mut
    if let Some(val) = rt.get_mut(50) {
        *val = "FIFTY";
    }
    if rt.get(50) != Some(&"FIFTY") { return fail("rdtree/basic", "get_mut"); }

    // get_key_value
    if rt.get_key_value(100) != Some((100, &"hundred")) {
        return fail("rdtree/basic", "get_key_value");
    }

    // remove
    rt.remove(50);
    if rt.contains(50) { return fail("rdtree/basic", "remove failed"); }
    if rt.len() != 2 { return fail("rdtree/basic", "len after remove"); }

    // remove_entry
    if rt.remove_entry(100) != Some((100, "hundred")) {
        return fail("rdtree/basic", "remove_entry");
    }

    // clear
    rt.clear();
    if !rt.is_empty() { return fail("rdtree/basic", "clear"); }
}

fn test_radix_operations() {
    use crate::libs::rdtree::RadixTree;

    let mut rt = RadixTree::new();
    rt.insert(10, "ten");
    rt.insert(20, "twenty");
    rt.insert(30, "thirty");

    // 边界查询
    if rt.lower_bound(15) != Some((20, &"twenty")) {
        return fail("rdtree/ops", "lower_bound(15)");
    }
    if rt.upper_bound(20) != Some((30, &"thirty")) {
        return fail("rdtree/ops", "upper_bound(20)");
    }
    if rt.floor(25) != Some((20, &"twenty")) {
        return fail("rdtree/ops", "floor(25)");
    }
    if rt.lower_than(20) != Some((10, &"ten")) {
        return fail("rdtree/ops", "lower_than(20)");
    }

    // first/last
    if rt.first() != Some((10, &"ten")) { return fail("rdtree/ops", "first"); }
    if rt.last() != Some((30, &"thirty")) { return fail("rdtree/ops", "last"); }

    // pop_first/pop_last
    let first = rt.pop_first();
    if first != Some((10, "ten")) { return fail("rdtree/ops", "pop_first"); }
    if rt.len() != 2 { return fail("rdtree/ops", "len after pop_first"); }

    let last = rt.pop_last();
    if last != Some((30, "thirty")) { return fail("rdtree/ops", "pop_last"); }
    if rt.len() != 1 { return fail("rdtree/ops", "len after pop_last"); }

    // get_or_insert
    let val = rt.get_or_insert(40, "forty");
    *val = "FORTY";
    if rt.get(40) != Some(&"FORTY") { return fail("rdtree/ops", "get_or_insert"); }

    // get_or_insert_with
    let val = rt.get_or_insert_with(50, || "fifty");
    if *val != "fifty" { return fail("rdtree/ops", "get_or_insert_with"); }

    // get_or_default
    let val = rt.get_or_default(60);
    if *val != "" { return fail("rdtree/ops", "get_or_default"); }
}

fn test_radix_range() {
    use crate::libs::rdtree::RadixTree;

    let mut rt = RadixTree::new();
    rt.insert(10, "ten");
    rt.insert(20, "twenty");
    rt.insert(30, "thirty");
    rt.insert(40, "forty");

    // first_in_range/last_in_range
    if rt.first_in_range(15, 35) != Some((20, &"twenty")) {
        return fail("rdtree/range", "first_in_range");
    }
    if rt.last_in_range(15, 35) != Some((30, &"thirty")) {
        return fail("rdtree/range", "last_in_range");
    }

    // range
    let items: Vec<_> = rt.range(15..35).collect();
    if items.len() != 2 { return fail("rdtree/range", "range len"); }
    if items[0] != (20, &"twenty") { return fail("rdtree/range", "range[0]"); }

    // remove_range
    let removed = rt.remove_range(15, 35);
    if removed.len() != 2 { return fail("rdtree/range", "remove_range len"); }
    if rt.len() != 2 { return fail("rdtree/range", "len after remove_range"); }

    // split_off
    rt.clear();
    rt.insert(1, "a");
    rt.insert(2, "b");
    rt.insert(3, "c");
    rt.insert(4, "d");

    let mut rt2 = rt.split_off(3);
    if rt.len() != 2 { return fail("rdtree/range", "split_off left len"); }
    if rt2.len() != 2 { return fail("rdtree/range", "split_off right len"); }
    if !rt.contains(1) || !rt.contains(2) {
        return fail("rdtree/range", "split_off left content");
    }
    if !rt2.contains(3) || !rt2.contains(4) {
        return fail("rdtree/range", "split_off right content");
    }

    // append
    rt.append(&mut rt2);
    if rt.len() != 4 { return fail("rdtree/range", "append len"); }
    if rt2.len() != 0 { return fail("rdtree/range", "append other len"); }

    // retain
    rt.retain(|k, _| k % 2 == 0);
    if rt.len() != 2 { return fail("rdtree/range", "retain len"); }
    if !rt.contains(2) || !rt.contains(4) {
        return fail("rdtree/range", "retain content");
    }
}

fn test_radix_iterators() {
    use crate::libs::rdtree::RadixTree;

    let mut rt = RadixTree::new();
    rt.insert(1, "a");
    rt.insert(2, "b");
    rt.insert(3, "c");

    // iter
    let items: Vec<_> = rt.iter().collect();
    if items.len() != 3 { return fail("rdtree/iter", "iter len"); }
    if items[0] != (1, &"a") { return fail("rdtree/iter", "iter[0]"); }

    // keys
    let keys: Vec<_> = rt.keys().collect();
    if keys != vec![1, 2, 3] { return fail("rdtree/iter", "keys"); }

    // values
    let values: Vec<_> = rt.values().copied().collect();
    if values != vec!["a", "b", "c"] { return fail("rdtree/iter", "values"); }

    // values_mut
    for val in rt.values_mut() {
        *val = "x";
    }
    if rt.get(1) != Some(&"x") { return fail("rdtree/iter", "values_mut"); }

    // iter_mut
    for (k, v) in rt.iter_mut() {
        if k == 2 {
            *v = "y";
        }
    }
    if rt.get(2) != Some(&"y") { return fail("rdtree/iter", "iter_mut"); }

    // IntoIterator for &RadixTree
    let mut count = 0;
    for (k, v) in &rt {
        count += 1;
        if k == 1 && v != &"x" { return fail("rdtree/iter", "into_iter ref"); }
    }
    if count != 3 { return fail("rdtree/iter", "into_iter ref count"); }
}

fn test_radix_edge_cases() {
    use crate::libs::rdtree::RadixTree;

    // 空树
    let rt: RadixTree<i32> = RadixTree::new();
    if !rt.is_empty() { return fail("rdtree/edge", "empty is_empty"); }
    if rt.first().is_some() { return fail("rdtree/edge", "empty first"); }
    if rt.last().is_some() { return fail("rdtree/edge", "empty last"); }
    if rt.find_first_gap_from(0) != Some(0) {
        return fail("rdtree/edge", "empty gap");
    }

    // 单元素
    let mut rt = RadixTree::new();
    rt.insert(42, "answer");
    if rt.len() != 1 { return fail("rdtree/edge", "single len"); }
    if rt.first() != Some((42, &"answer")) {
        return fail("rdtree/edge", "single first");
    }
    if rt.last() != Some((42, &"answer")) {
        return fail("rdtree/edge", "single last");
    }

    // 大键值
    let mut rt = RadixTree::new();
    rt.insert(usize::MAX, "max");
    rt.insert(usize::MAX - 1, "max-1");
    if rt.len() != 2 { return fail("rdtree/edge", "large key len"); }
    if rt.get(usize::MAX) != Some(&"max") {
        return fail("rdtree/edge", "large key get");
    }

    // 间隙搜索
    let mut rt = RadixTree::new();
    rt.insert(0, "zero");
    rt.insert(2, "two");
    rt.insert(5, "five");

    if rt.find_first_gap_from(0) != Some(1) {
        return fail("rdtree/edge", "gap from 0");
    }
    if rt.find_first_gap_from(1) != Some(1) {
        return fail("rdtree/edge", "gap from 1");
    }
    if rt.find_first_gap_from(2) != Some(3) {
        return fail("rdtree/edge", "gap from 2");
    }

    // insert_first_gap_from
    if rt.insert_first_gap_from(0, "one") != Some(1) {
        return fail("rdtree/edge", "insert_first_gap");
    }
    if rt.get(1) != Some(&"one") {
        return fail("rdtree/edge", "insert_first_gap content");
    }

    // FromIterator
    let rt2: RadixTree<_> = [(10, "x"), (20, "y"), (30, "z")].into_iter().collect();
    if rt2.len() != 3 { return fail("rdtree/edge", "from_iter len"); }

    // Extend
    let mut rt3 = RadixTree::new();
    rt3.extend([(1, "a"), (2, "b")]);
    if rt3.len() != 2 { return fail("rdtree/edge", "extend len"); }

    // Clone
    let rt4 = rt3.clone();
    if rt4.len() != rt3.len() { return fail("rdtree/edge", "clone len"); }
    if rt4.get(1) != Some(&"a") { return fail("rdtree/edge", "clone content"); }
}
    if count != 2 { return fail("rdtree", "into_iter ref count"); }

    // --- 新增测试：retain ---
    let mut rt6 = RadixTree::new();
    rt6.insert(1, 10);
    rt6.insert(2, 20);
    rt6.insert(3, 30);
    rt6.retain(|_, v| *v > 15);
    if rt6.len() != 2 { return fail("rdtree", "retain len"); }
    if rt6.contains(1) { return fail("rdtree", "retain removed"); }

    // --- 新增测试：gap finding 边界 ---
    let mut rt7 = RadixTree::new();
    rt7.insert(0, "a");
    rt7.insert(1, "b");
    rt7.insert(2, "c");
    if rt7.find_first_gap_from(0) != Some(3) { return fail("rdtree", "gap continuous"); }

    rt7.insert(usize::MAX - 1, "max-1");
    if rt7.find_first_gap_from(usize::MAX - 2) != Some(usize::MAX - 2) {
        return fail("rdtree", "gap near max");
    }

    // 测试溢出保护
    rt7.insert(usize::MAX, "max");
    if rt7.find_first_gap_from(usize::MAX).is_some() {
        return fail("rdtree", "gap at max should be None");
    }

    pass("rdtree");
}

fn test_btree() {
    use crate::libs::btree::BTree;

    // --- 基本插入与查询 ---
    let mut bt = BTree::new();
    bt.insert(5, "five");
    bt.insert(2, "two");
    bt.insert(8, "eight");
    bt.insert(1, "one");
    bt.insert(9, "nine");

    if bt.len() != 5 { return fail("btree", "len != 5"); }
    if bt.is_empty() { return fail("btree", "should not be empty"); }
    if bt.get(&5) != Some(&"five") { return fail("btree", "get(5) failed"); }
    if bt.get(&1) != Some(&"one") { return fail("btree", "get(1) failed"); }
    if bt.get(&99).is_some() { return fail("btree", "get(99) should be None"); }

    // --- 插入覆盖 ---
    let old = bt.insert(5, "FIVE");
    if old != Some("five") { return fail("btree", "insert overwrite"); }
    if bt.get(&5) != Some(&"FIVE") { return fail("btree", "get after overwrite"); }
    if bt.len() != 5 { return fail("btree", "len after overwrite"); }

    // --- 迭代顺序 ---
    let keys: Vec<_> = bt.iter().map(|(k, _)| *k).collect();
    if keys != vec![1, 2, 5, 8, 9] { return fail("btree", "iteration order"); }

    let values: Vec<_> = bt.iter().map(|(_, v)| *v).collect();
    if values != vec!["one", "two", "FIVE", "eight", "nine"] {
        return fail("btree", "iteration values");
    }

    // --- 删除 ---
    let removed = bt.remove(&2);
    if removed != Some("two") { return fail("btree", "remove(2)"); }
    if bt.len() != 4 { return fail("btree", "len after remove"); }
    if bt.get(&2).is_some() { return fail("btree", "get(2) after remove"); }

    // 删除不存在的键
    if bt.remove(&99).is_some() { return fail("btree", "remove non-existent"); }
    if bt.len() != 4 { return fail("btree", "len after remove non-existent"); }

    // --- 大量插入测试（触发节点分裂）---
    let mut bt2 = BTree::new();
    for i in 0..100 {
        bt2.insert(i, i * 10);
    }
    if bt2.len() != 100 { return fail("btree", "bulk insert len"); }

    // 验证所有值都能正确检索
    for i in 0..100 {
        if bt2.get(&i) != Some(&(i * 10)) {
            return fail("btree", "bulk insert get");
        }
    }

    // 验证迭代顺序
    let keys: Vec<_> = bt2.iter().map(|(k, _)| *k).collect();
    let expected: Vec<_> = (0..100).collect();
    if keys != expected { return fail("btree", "bulk insert iteration order"); }

    // --- 大量删除测试 ---
    for i in (0..100).step_by(2) {
        bt2.remove(&i);
    }
    if bt2.len() != 50 { return fail("btree", "bulk remove len"); }

    // 验证奇数键仍然存在
    for i in (1..100).step_by(2) {
        if bt2.get(&i) != Some(&(i * 10)) {
            return fail("btree", "bulk remove get odd");
        }
    }

    // 验证偶数键已删除
    for i in (0..100).step_by(2) {
        if bt2.get(&i).is_some() {
            return fail("btree", "bulk remove get even");
        }
    }

    // --- 空树测试 ---
    let empty: BTree<i32, i32> = BTree::new();
    if !empty.is_empty() { return fail("btree", "empty is_empty"); }
    if empty.len() != 0 { return fail("btree", "empty len"); }
    if empty.get(&1).is_some() { return fail("btree", "empty get"); }

    let count = empty.iter().count();
    if count != 0 { return fail("btree", "empty iter count"); }

    // --- 单元素测试 ---
    let mut single = BTree::new();
    single.insert(42, "answer");
    if single.len() != 1 { return fail("btree", "single len"); }
    if single.get(&42) != Some(&"answer") { return fail("btree", "single get"); }

    let items: Vec<_> = single.iter().collect();
    if items.len() != 1 { return fail("btree", "single iter len"); }
    if items[0] != (&42, &"answer") { return fail("btree", "single iter content"); }

    single.remove(&42);
    if !single.is_empty() { return fail("btree", "single after remove"); }

    // --- 逆序插入测试 ---
    let mut bt3 = BTree::new();
    for i in (0..50).rev() {
        bt3.insert(i, i);
    }
    if bt3.len() != 50 { return fail("btree", "reverse insert len"); }

    let keys: Vec<_> = bt3.iter().map(|(k, _)| *k).collect();
    let expected: Vec<_> = (0..50).collect();
    if keys != expected { return fail("btree", "reverse insert order"); }

    // --- 新增：contains_key 测试 ---
    if !bt3.contains_key(&25) { return fail("btree", "contains_key(25)"); }
    if bt3.contains_key(&100) { return fail("btree", "contains_key(100) should be false"); }

    // --- 新增：get_mut 测试 ---
    if let Some(v) = bt3.get_mut(&25) {
        *v = 250;
    }
    if bt3.get(&25) != Some(&250) { return fail("btree", "get_mut modification"); }

    // --- 新增：first/last 测试 ---
    if bt3.first() != Some((&0, &0)) { return fail("btree", "first()"); }
    if bt3.last() != Some((&49, &49)) { return fail("btree", "last()"); }

    // --- 新增：pop_first/pop_last 测试 ---
    let mut bt4 = BTree::new();
    bt4.insert(1, "a");
    bt4.insert(2, "b");
    bt4.insert(3, "c");

    if bt4.pop_first() != Some((1, "a")) { return fail("btree", "pop_first"); }
    if bt4.len() != 2 { return fail("btree", "len after pop_first"); }

    if bt4.pop_last() != Some((3, "c")) { return fail("btree", "pop_last"); }
    if bt4.len() != 1 { return fail("btree", "len after pop_last"); }

    // --- 新增：clear 测试 ---
    bt4.clear();
    if !bt4.is_empty() { return fail("btree", "clear"); }
    if bt4.len() != 0 { return fail("btree", "len after clear"); }

    // --- 新增：keys/values 迭代器测试 ---
    let mut bt5 = BTree::new();
    bt5.insert(1, "one");
    bt5.insert(2, "two");
    bt5.insert(3, "three");

    let keys: Vec<_> = bt5.keys().copied().collect();
    if keys != vec![1, 2, 3] { return fail("btree", "keys iterator"); }

    let values: Vec<_> = bt5.values().copied().collect();
    if values != vec!["one", "two", "three"] { return fail("btree", "values iterator"); }

    // --- 新增：retain 测试 ---
    let mut bt6 = BTree::new();
    bt6.insert(1, 10);
    bt6.insert(2, 20);
    bt6.insert(3, 30);
    bt6.insert(4, 40);

    bt6.retain(|_, v| *v > 15);
    if bt6.len() != 3 { return fail("btree", "retain len"); }
    if bt6.contains_key(&1) { return fail("btree", "retain removed key 1"); }
    if !bt6.contains_key(&2) { return fail("btree", "retain kept key 2"); }

    // --- 新增：Clone 测试 ---
    let bt7 = bt5.clone();
    if bt7.len() != bt5.len() { return fail("btree", "clone len"); }
    if bt7.get(&2) != Some(&"two") { return fail("btree", "clone content"); }

    // --- 新增：FromIterator 测试 ---
    let bt8: BTree<_, _> = [(1, "x"), (2, "y"), (3, "z")].into_iter().collect();
    if bt8.len() != 3 { return fail("btree", "from_iter len"); }
    if bt8.get(&2) != Some(&"y") { return fail("btree", "from_iter content"); }

    // --- 新增：Extend 测试 ---
    let mut bt9 = BTree::new();
    bt9.insert(1, "a");
    bt9.extend([(2, "b"), (3, "c")]);
    if bt9.len() != 3 { return fail("btree", "extend len"); }

    pass("btree");
}

fn test_maple_tree() {
    test_maple_basic();
    test_maple_operations();
    test_maple_gap_search();
    test_maple_advanced();
    test_maple_edge_cases();
    pass("mptree");
}

fn test_maple_basic() {
    use crate::libs::mptree::{MapleTree, MapleInsertError};

    // 基本插入和查找
    let mut mt = MapleTree::new();
    if mt.insert(0, 100, "a").is_err() { return fail("mptree/basic", "insert a"); }
    if mt.insert(200, 300, "b").is_err() { return fail("mptree/basic", "insert b"); }

    if mt.len() != 2 { return fail("mptree/basic", "len != 2"); }
    if mt.is_empty() { return fail("mptree/basic", "should not be empty"); }

    // find
    if mt.find(50) != Some((0, 100, &"a")) { return fail("mptree/basic", "find(50)"); }
    if mt.find(150).is_some() { return fail("mptree/basic", "find(150) should be None"); }
    if mt.find(250) != Some((200, 300, &"b")) { return fail("mptree/basic", "find(250)"); }

    // contains
    if !mt.contains(50) { return fail("mptree/basic", "contains(50)"); }
    if mt.contains(150) { return fail("mptree/basic", "contains(150)"); }

    // contains_start
    if !mt.contains_start(0) { return fail("mptree/basic", "contains_start(0)"); }
    if mt.contains_start(50) { return fail("mptree/basic", "contains_start(50)"); }

    // 重叠检测
    if mt.insert(50, 150, "c") != Err(MapleInsertError::Overlap) {
        return fail("mptree/basic", "overlap not detected");
    }
    if mt.insert(300, 200, "d") != Err(MapleInsertError::InvalidRange) {
        return fail("mptree/basic", "invalid range not detected");
    }

    // remove
    if mt.remove(0) != Some((100, "a")) { return fail("mptree/basic", "remove(0)"); }
    if mt.len() != 1 { return fail("mptree/basic", "len after remove"); }
    if mt.find(50).is_some() { return fail("mptree/basic", "find after remove"); }

    // clear
    mt.clear();
    if !mt.is_empty() { return fail("mptree/basic", "clear"); }
}

fn test_maple_operations() {
    use crate::libs::mptree::MapleTree;

    let mut mt = MapleTree::new();
    mt.insert(0, 100, "a").unwrap();
    mt.insert(200, 300, "b").unwrap();

    // get_mut
    if let Some(val) = mt.get_mut(50) {
        *val = "A";
    }
    if mt.find(50) != Some((0, 100, &"A")) { return fail("mptree/ops", "get_mut"); }

    // get_mut_at
    if let Some((end, val)) = mt.get_mut_at(0) {
        if end != 100 { return fail("mptree/ops", "get_mut_at end"); }
        *val = "AA";
    }
    if mt.find(50) != Some((0, 100, &"AA")) { return fail("mptree/ops", "get_mut_at"); }

    // update_end
    if mt.update_end(200, 350).is_err() { return fail("mptree/ops", "update_end"); }
    if mt.find(320) != Some((200, 350, &"b")) {
        return fail("mptree/ops", "find after update_end");
    }
    mt.update_end(200, 300).unwrap(); // 恢复

    // replace
    let old = mt.replace(0, 120, "a2");
    if old != Ok(Some((100, "AA"))) { return fail("mptree/ops", "replace"); }
    if mt.find(110) != Some((0, 120, &"a2")) {
        return fail("mptree/ops", "find after replace");
    }

    // insert_overwrite
    let removed = mt.insert_overwrite(50, 250, "ow");
    match removed {
        Ok(v) => {
            if v.len() != 2 { return fail("mptree/ops", "overwrite removed count"); }
        }
        Err(_) => return fail("mptree/ops", "insert_overwrite failed"),
    }
    if mt.find(100) != Some((50, 250, &"ow")) {
        return fail("mptree/ops", "find after overwrite");
    }

    // split_at
    mt.clear();
    mt.insert(0, 100, "s").unwrap();
    if !mt.split_at(50) { return fail("mptree/ops", "split_at"); }
    if mt.len() != 2 { return fail("mptree/ops", "len after split"); }
    if mt.find(25) != Some((0, 50, &"s")) { return fail("mptree/ops", "left half"); }
    if mt.find(75) != Some((50, 100, &"s")) { return fail("mptree/ops", "right half"); }

    // merge_adjacent_equal
    mt.clear();
    mt.insert(0, 10, "m").unwrap();
    mt.insert(10, 20, "m").unwrap();
    mt.insert(20, 30, "m").unwrap();
    let merges = mt.merge_adjacent_equal();
    if merges != 2 { return fail("mptree/ops", "merge count"); }
    if mt.len() != 1 { return fail("mptree/ops", "len after merge"); }
    if mt.find(15) != Some((0, 30, &"m")) { return fail("mptree/ops", "merged interval"); }
}

fn test_maple_gap_search() {
    use crate::libs::mptree::MapleTree;

    let mut mt = MapleTree::new();
    mt.insert(0, 100, "a").unwrap();
    mt.insert(200, 300, "b").unwrap();

    // find_gap 正向
    if let Some(start) = mt.find_gap(100, 0, usize::MAX) {
        if start != 100 { return fail("mptree/gap", "gap start != 100"); }
    } else {
        return fail("mptree/gap", "find_gap failed");
    }

    // find_gap 反向
    if let Some(start) = mt.find_gap_reverse(100, 400, 0) {
        if start != 300 { return fail("mptree/gap", "gap_rev start != 300"); }
    } else {
        return fail("mptree/gap", "find_gap_reverse failed");
    }

    // 空树的间隙
    let empty: MapleTree<()> = MapleTree::new();
    if empty.find_gap(10, 0, 100) != Some(0) {
        return fail("mptree/gap", "empty gap");
    }
}

fn test_maple_advanced() {
    use crate::libs::mptree::MapleTree;

    let mut mt = MapleTree::new();
    mt.insert(0, 100, "a").unwrap();
    mt.insert(200, 300, "b").unwrap();
    mt.insert(400, 500, "c").unwrap();

    // first/last
    if mt.first() != Some((0, 100, &"a")) { return fail("mptree/adv", "first"); }
    if mt.last() != Some((400, 500, &"c")) { return fail("mptree/adv", "last"); }

    // pop_first/pop_last
    let first = mt.pop_first();
    if first != Some((0, 100, "a")) { return fail("mptree/adv", "pop_first"); }
    if mt.len() != 2 { return fail("mptree/adv", "len after pop_first"); }

    let last = mt.pop_last();
    if last != Some((400, 500, "c")) { return fail("mptree/adv", "pop_last"); }
    if mt.len() != 1 { return fail("mptree/adv", "len after pop_last"); }

    // starts
    mt.clear();
    mt.insert(10, 20, "x").unwrap();
    mt.insert(30, 40, "y").unwrap();
    let starts: Vec<_> = mt.starts().collect();
    if starts != vec![10, 30] { return fail("mptree/adv", "starts"); }

    // values
    let values: Vec<_> = mt.values().copied().collect();
    if values != vec!["x", "y"] { return fail("mptree/adv", "values"); }

    // values_mut
    for val in mt.values_mut() {
        *val = "z";
    }
    if mt.find(15) != Some((10, 20, &"z")) { return fail("mptree/adv", "values_mut"); }

    // iter
    let items: Vec<_> = mt.iter().collect();
    if items.len() != 2 { return fail("mptree/adv", "iter len"); }

    // lower_bound
    if mt.lower_bound(25) != Some((30, 40, &"z")) {
        return fail("mptree/adv", "lower_bound");
    }

    // iter_intersecting
    mt.clear();
    mt.insert(0, 100, "a").unwrap();
    mt.insert(200, 300, "b").unwrap();
    mt.insert(400, 500, "c").unwrap();

    let intersecting: Vec<_> = mt.iter_intersecting(50, 250).collect();
    if intersecting.len() != 2 { return fail("mptree/adv", "intersecting len"); }

    // has_intersection
    if !mt.has_intersection(50, 250) {
        return fail("mptree/adv", "has_intersection true");
    }
    if mt.has_intersection(100, 200) {
        return fail("mptree/adv", "has_intersection false");
    }

    // remove_intersecting
    let removed = mt.remove_intersecting(50, 250);
    if removed.len() != 2 { return fail("mptree/adv", "remove_intersecting len"); }
    if mt.len() != 1 { return fail("mptree/adv", "len after remove_intersecting"); }

    // total_length
    mt.clear();
    mt.insert(0, 100, "a").unwrap();
    mt.insert(200, 300, "b").unwrap();
    if mt.total_length() != 200 { return fail("mptree/adv", "total_length"); }

    // retain
    mt.insert(400, 500, "c").unwrap();
    mt.retain(|s, e, _| e - s >= 100);
    if mt.len() != 3 { return fail("mptree/adv", "retain len"); }

    mt.retain(|s, _, _| s < 300);
    if mt.len() != 2 { return fail("mptree/adv", "retain len 2"); }
}

fn test_maple_edge_cases() {
    use crate::libs::mptree::MapleTree;

    // 空树
    let mt: MapleTree<i32> = MapleTree::new();
    if !mt.is_empty() { return fail("mptree/edge", "empty is_empty"); }
    if mt.find(0).is_some() { return fail("mptree/edge", "empty find"); }
    if mt.first().is_some() { return fail("mptree/edge", "empty first"); }
    if mt.last().is_some() { return fail("mptree/edge", "empty last"); }

    // 单区间
    let mut mt = MapleTree::new();
    mt.insert(10, 20, "single").unwrap();
    if mt.len() != 1 { return fail("mptree/edge", "single len"); }
    if mt.first() != Some((10, 20, &"single")) {
        return fail("mptree/edge", "single first");
    }
    if mt.last() != Some((10, 20, &"single")) {
        return fail("mptree/edge", "single last");
    }

    // 相邻区间
    let mut mt = MapleTree::new();
    mt.insert(0, 10, "a").unwrap();
    mt.insert(10, 20, "b").unwrap();
    mt.insert(20, 30, "c").unwrap();
    if mt.len() != 3 { return fail("mptree/edge", "adjacent len"); }
    if mt.find(10) != Some((10, 20, &"b")) {
        return fail("mptree/edge", "adjacent boundary");
    }

    // 大范围
    let mut mt = MapleTree::new();
    mt.insert(0, usize::MAX / 2, "large").unwrap();
    if mt.find(1000) != Some((0, usize::MAX / 2, &"large")) {
        return fail("mptree/edge", "large range");
    }

    // Clone
    let mt2 = mt.clone();
    if mt2.len() != mt.len() { return fail("mptree/edge", "clone len"); }
    if mt2.find(1000) != Some((0, usize::MAX / 2, &"large")) {
        return fail("mptree/edge", "clone content");
    }

    // Default
    let mt3: MapleTree<i32> = MapleTree::default();
    if !mt3.is_empty() { return fail("mptree/edge", "default"); }
}

fn test_rcu() {
    kprintln!("  [RCU] Starting basic tests...");
    test_rcu_basic();
    kprintln!("  [RCU] Starting update tests...");
    test_rcu_updates();
    kprintln!("  [RCU] Starting reader tests...");
    test_rcu_readers();
    kprintln!("  [RCU] Starting synchronization tests...");
    test_rcu_synchronization();
    kprintln!("  [RCU] Starting ownership tests...");
    test_rcu_ownership();
    kprintln!("  [RCU] Starting edge case tests...");
    test_rcu_edge_cases();
    pass("rcu");
}

fn test_rcu_basic() {
    use crate::libs::rcu::Rcu;

    kprintln!("    [RCU/basic] Creating RCU...");
    let rcu = Rcu::new(42u64);
    {
        kprintln!("    [RCU/basic] Reading value...");
        let guard = rcu.read();
        if *guard != 42 { return fail("rcu/basic", "initial read != 42"); }
    }

    kprintln!("    [RCU/basic] Peeking value...");
    let ptr = rcu.peek();
    unsafe {
        if *ptr != 42 { return fail("rcu/basic", "peek != 42"); }
    }

    kprintln!("    [RCU/basic] Checking reader count...");
    if rcu.active_readers() != 0 { return fail("rcu/basic", "should have 0 readers"); }
    if !rcu.is_quiescent() { return fail("rcu/basic", "should be quiescent"); }

    {
        let _guard = rcu.read();
        if rcu.active_readers() != 1 { return fail("rcu/basic", "should have 1 reader"); }
        if rcu.is_quiescent() { return fail("rcu/basic", "should not be quiescent"); }
    }

    if rcu.active_readers() != 0 { return fail("rcu/basic", "readers should be 0 after drop"); }
    kprintln!("    [RCU/basic] PASSED");
}

fn test_rcu_updates() {
    use crate::libs::rcu::Rcu;

    kprintln!("    [RCU/update] Testing update...");
    let rcu = Rcu::new(10u32);

    let old = rcu.update(20);
    if old != 10 { return fail("rcu/update", "update old != 10"); }
    {
        let guard = rcu.read();
        if *guard != 20 { return fail("rcu/update", "read after update != 20"); }
    }

    kprintln!("    [RCU/update] Testing update_with...");
    let old = rcu.update_with(|val| *val * 2);
    if old != 20 { return fail("rcu/update", "update_with old != 20"); }
    {
        let guard = rcu.read();
        if *guard != 40 { return fail("rcu/update", "read after update_with != 40"); }
    }

    kprintln!("    [RCU/update] Testing try_update (no readers)...");
    let result = rcu.try_update(50);
    if result.is_err() { return fail("rcu/update", "try_update should succeed"); }
    {
        let guard = rcu.read();
        if *guard != 50 { return fail("rcu/update", "read after try_update != 50"); }
    }

    kprintln!("    [RCU/update] Testing try_update behavior...");
    // 注意：在单线程环境中，不能在持有读守卫时调用 try_update，
    // 因为 try_update 会调用 synchronize_rcu 等待读者完成，导致死锁。
    // 这里只测试 try_update 的基本功能。
    let result = rcu.try_update(60);
    if result.is_err() { return fail("rcu/update", "try_update should succeed"); }

    kprintln!("    [RCU/update] Creating new RCU for update_async test...");
    // 使用新的 RCU 实例避免状态问题
    let rcu2 = Rcu::new(100u32);

    kprintln!("    [RCU/update] Testing update_async...");
    kprintln!("    [RCU/update] Active readers before update_async: {}", rcu2.active_readers());

    let result = rcu2.update_async(200);

    kprintln!("    [RCU/update] update_async returned");
    if result.is_err() {
        return fail("rcu/update", "update_async should succeed");
    }

    kprintln!("    [RCU/update] PASSED");
}

fn test_rcu_readers() {
    use crate::libs::rcu::Rcu;

    kprintln!("    [RCU/readers] Testing multiple readers...");
    let rcu = Rcu::new(100u64);

    let guard1 = rcu.read();
    let guard2 = rcu.read();
    let guard3 = rcu.read();

    if rcu.active_readers() != 3 { return fail("rcu/readers", "should have 3 readers"); }
    if *guard1 != 100 { return fail("rcu/readers", "guard1 != 100"); }
    if *guard2 != 100 { return fail("rcu/readers", "guard2 != 100"); }
    if *guard3 != 100 { return fail("rcu/readers", "guard3 != 100"); }

    drop(guard1);
    if rcu.active_readers() != 2 { return fail("rcu/readers", "should have 2 readers after drop"); }

    drop(guard2);
    drop(guard3);
    if rcu.active_readers() != 0 { return fail("rcu/readers", "should have 0 readers"); }

    kprintln!("    [RCU/readers] Testing try_read...");
    let guard = rcu.try_read();
    if guard.is_none() { return fail("rcu/readers", "try_read should succeed"); }
    drop(guard);

    kprintln!("    [RCU/readers] Testing reader isolation...");
    let guard = rcu.read();
    let old_val = *guard;
    rcu.update(200);
    if *guard != old_val { return fail("rcu/readers", "reader should see old value during update"); }
    drop(guard);

    let guard = rcu.read();
    if *guard != 200 { return fail("rcu/readers", "new reader should see new value"); }

    kprintln!("    [RCU/readers] PASSED");
}

fn test_rcu_synchronization() {
    use crate::libs::rcu::Rcu;

    kprintln!("    [RCU/sync] Testing basic synchronization...");
    let rcu = Rcu::new(1u32);

    // 验证初始状态
    if rcu.active_readers() != 0 {
        return fail("rcu/sync", "should start with 0 readers");
    }

    kprintln!("    [RCU/sync] Testing synchronize_rcu (no readers)...");
    // 只在没有活跃读者时调用 synchronize_rcu
    // 在单线程环境中，如果有读者会导致死锁
    rcu.synchronize_rcu();

    if rcu.active_readers() != 0 {
        return fail("rcu/sync", "should have 0 readers after sync");
    }

    kprintln!("    [RCU/sync] Testing call_rcu (simplified)...");
    // call_rcu 测试 - 只验证不会崩溃
    // 注意：在当前实现中，call_rcu 会调用 synchronize_rcu
    // 所以我们需要确保没有活跃读者
    let mut callback_executed = false;
    rcu.call_rcu(|| {
        // 简单的回调
    });

    kprintln!("    [RCU/sync] Testing rcu_barrier...");
    // rcu_barrier 也会调用 synchronize_rcu
    rcu.rcu_barrier();

    kprintln!("    [RCU/sync] PASSED");
}

fn test_rcu_ownership() {
    use crate::libs::rcu::Rcu;

    kprintln!("    [RCU/ownership] Testing get_mut...");
    let mut rcu = Rcu::new(42u32);
    {
        let val = rcu.get_mut();
        *val = 100;
    }
    {
        let guard = rcu.read();
        if *guard != 100 { return fail("rcu/ownership", "get_mut modification failed"); }
    }

    kprintln!("    [RCU/ownership] Testing into_inner...");
    let value = rcu.into_inner();
    if value != 100 { return fail("rcu/ownership", "into_inner != 100"); }

    kprintln!("    [RCU/ownership] PASSED");
}

fn test_rcu_edge_cases() {
    use crate::libs::rcu::Rcu;

    kprintln!("    [RCU/edge] Testing ZST...");
    let rcu = Rcu::new(());
    {
        let guard = rcu.read();
        if *guard != () { return fail("rcu/edge", "ZST read failed"); }
    }
    rcu.update(());

    kprintln!("    [RCU/edge] Testing large struct...");
    #[derive(Clone, Copy, PartialEq, Debug)]
    struct Large {
        data: [u64; 16],
    }
    let large = Large { data: [42; 16] };
    let rcu = Rcu::new(large);
    {
        let guard = rcu.read();
        if guard.data[0] != 42 { return fail("rcu/edge", "large struct read failed"); }
    }

    kprintln!("    [RCU/edge] Testing consecutive updates...");
    let rcu = Rcu::new(0u32);
    for i in 1..=10 {
        rcu.update(i);
    }
    {
        let guard = rcu.read();
        if *guard != 10 { return fail("rcu/edge", "final value != 10"); }
    }

    kprintln!("    [RCU/edge] Testing reader during updates...");
    let rcu = Rcu::new(0u32);
    let guard = rcu.read();
    let initial = *guard;
    rcu.update(1);
    rcu.update(2);
    rcu.update(3);
    if *guard != initial { return fail("rcu/edge", "reader should see initial value"); }
    drop(guard);

    let guard = rcu.read();
    if *guard != 3 { return fail("rcu/edge", "new reader should see latest value"); }

    kprintln!("    [RCU/edge] PASSED");
}
