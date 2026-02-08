//! 内核数据结构 (libs) 测试

use crate::{kprintln, ok, error};

pub fn run() {
    kprintln!("=== Libs Data Structure Tests ===");
    test_rbtree();
    test_lru();
    test_radix_tree();
    test_maple_tree();
    test_rcu();
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
    use crate::libs::lru::LruCache;

    let mut cache = LruCache::new(3);
    cache.put(1, "a");
    cache.put(2, "b");
    cache.put(3, "c");

    if cache.len() != 3 { return fail("lru", "len != 3"); }

    // 访问 1，使其成为 MRU
    if cache.get(&1) != Some(&"a") { return fail("lru", "get(1) failed"); }

    // 插入 4，应淘汰 LRU (key=2)
    cache.put(4, "d");
    if cache.len() != 3 { return fail("lru", "eviction len != 3"); }
    if cache.get(&2).is_some() { return fail("lru", "key 2 should be evicted"); }
    if cache.get(&4) != Some(&"d") { return fail("lru", "get(4) failed"); }

    pass("lru");
}

fn test_radix_tree() {
    use crate::libs::rdtree::RadixTree;

    let mut rt = RadixTree::new();
    rt.insert(0, "zero");
    rt.insert(100, "hundred");
    rt.insert(50, "fifty");

    if rt.len() != 3 { return fail("rdtree", "len != 3"); }
    if rt.get(50) != Some(&"fifty") { return fail("rdtree", "get(50) failed"); }

    // gap search
    if rt.find_first_gap_from(0) != Some(1) { return fail("rdtree", "gap from 0 != 1"); }

    rt.remove(50);
    if rt.contains(50) { return fail("rdtree", "remove failed"); }

    pass("rdtree");
}

fn test_maple_tree() {
    use crate::libs::mptree::{MapleTree, MapleInsertError};

    // --- 基本插入与查找 ---
    let mut mt = MapleTree::new();
    if mt.insert(0, 100, "a").is_err() { return fail("mptree", "insert a"); }
    if mt.insert(200, 300, "b").is_err() { return fail("mptree", "insert b"); }

    if mt.len() != 2 { return fail("mptree", "len != 2"); }
    if mt.find(50) != Some((0, 100, &"a")) { return fail("mptree", "find(50)"); }
    if mt.find(150).is_some() { return fail("mptree", "find(150) should be None"); }
    if mt.find(250) != Some((200, 300, &"b")) { return fail("mptree", "find(250)"); }

    // --- 重叠检测 ---
    if mt.insert(50, 150, "c") != Err(MapleInsertError::Overlap) {
        return fail("mptree", "overlap not detected");
    }
    if mt.insert(300, 200, "d") != Err(MapleInsertError::InvalidRange) {
        return fail("mptree", "invalid range not detected");
    }

    // --- gap search 正向 ---
    if let Some(start) = mt.find_gap(100, 0, usize::MAX) {
        if start != 100 { return fail("mptree", "gap start != 100"); }
    } else {
        return fail("mptree", "find_gap failed");
    }

    // --- gap search 反向 ---
    if let Some(start) = mt.find_gap_reverse(100, 400, 0) {
        if start != 300 { return fail("mptree", "gap_rev start != 300"); }
    } else {
        return fail("mptree", "find_gap_reverse failed");
    }

    // --- 删除 ---
    if mt.remove(0) != Some((100, "a")) { return fail("mptree", "remove(0)"); }
    if mt.len() != 1 { return fail("mptree", "len after remove != 1"); }
    if mt.find(50).is_some() { return fail("mptree", "find(50) after remove"); }

    // --- update_end ---
    if mt.update_end(200, 350).is_err() { return fail("mptree", "update_end"); }
    if mt.find(320) != Some((200, 350, &"b")) { return fail("mptree", "find after update_end"); }
    // 恢复
    if mt.update_end(200, 300).is_err() { return fail("mptree", "update_end restore"); }

    // --- replace ---
    if mt.insert(0, 100, "a").is_err() { return fail("mptree", "re-insert a"); }
    let old = mt.replace(0, 120, "a2");
    if old != Ok(Some((100, "a"))) { return fail("mptree", "replace"); }
    if mt.find(110) != Some((0, 120, &"a2")) { return fail("mptree", "find after replace"); }

    // --- insert_overwrite ---
    let removed = mt.insert_overwrite(50, 250, "ow");
    match removed {
        Ok(v) => {
            if v.len() != 2 { return fail("mptree", "overwrite removed count"); }
        }
        Err(_) => return fail("mptree", "insert_overwrite failed"),
    }
    if mt.find(100) != Some((50, 250, &"ow")) { return fail("mptree", "find after overwrite"); }

    // --- split_at ---
    mt.clear();
    if mt.insert(0, 100, "s").is_err() { return fail("mptree", "insert for split"); }
    if !mt.split_at(50) { return fail("mptree", "split_at"); }
    if mt.len() != 2 { return fail("mptree", "len after split != 2"); }
    if mt.find(25) != Some((0, 50, &"s")) { return fail("mptree", "left half"); }
    if mt.find(75) != Some((50, 100, &"s")) { return fail("mptree", "right half"); }

    // --- merge_adjacent_equal 链式合并 ---
    mt.clear();
    if mt.insert(0, 10, "m").is_err() { return fail("mptree", "merge insert 1"); }
    if mt.insert(10, 20, "m").is_err() { return fail("mptree", "merge insert 2"); }
    if mt.insert(20, 30, "m").is_err() { return fail("mptree", "merge insert 3"); }
    let merges = mt.merge_adjacent_equal();
    if merges != 2 { return fail("mptree", "chain merge count != 2"); }
    if mt.len() != 1 { return fail("mptree", "len after chain merge != 1"); }
    if mt.find(15) != Some((0, 30, &"m")) { return fail("mptree", "merged interval"); }

    // --- clone ---
    let mt2 = mt.clone();
    if mt2.len() != mt.len() { return fail("mptree", "clone len"); }
    if mt2.find(15) != Some((0, 30, &"m")) { return fail("mptree", "clone find"); }

    // --- 边界：空树 ---
    let empty: MapleTree<()> = MapleTree::new();
    if empty.find(0).is_some() { return fail("mptree", "empty find"); }
    if empty.find_gap(10, 0, 100) != Some(0) { return fail("mptree", "empty gap"); }

    pass("mptree");
}

fn test_rcu() {
    use crate::libs::rcu::Rcu;

    let rcu = Rcu::new(42u64);

    // 读取
    {
        let guard = rcu.read();
        if *guard != 42 { return fail("rcu", "initial read != 42"); }
    }

    // 更新
    let old = rcu.update(99);
    if old != 42 { return fail("rcu", "update old != 42"); }

    {
        let guard = rcu.read();
        if *guard != 99 { return fail("rcu", "read after update != 99"); }
    }

    pass("rcu");
}
