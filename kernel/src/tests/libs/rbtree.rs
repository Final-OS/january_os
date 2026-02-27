use super::{fail, pass};

pub(super) fn run() {
    use crate::libs::rbtree::RbTree;

    let mut tree = RbTree::new();
    tree.insert(3, "three");
    tree.insert(1, "one");
    tree.insert(2, "two");
    tree.insert(5, "five");
    tree.insert(4, "four");

    // --- 基本查询 ---
    if tree.len() != 5 {
        return fail("rbtree", "len != 5");
    }
    if tree.get(&2) != Some(&"two") {
        return fail("rbtree", "get(2)");
    }
    if tree.get(&99).is_some() {
        return fail("rbtree", "get(99) should be None");
    }
    if !tree.contains_key(&4) {
        return fail("rbtree", "contains_key(4)");
    }
    if tree.first_key_value() != Some((&1, &"one")) {
        return fail("rbtree", "first");
    }
    if tree.last_key_value() != Some((&5, &"five")) {
        return fail("rbtree", "last");
    }

    // --- floor / lower_bound / upper_bound / lower_than ---
    if tree.floor(&3) != Some((&3, &"three")) {
        return fail("rbtree", "floor(3)");
    }
    if tree.floor(&0).is_some() {
        return fail("rbtree", "floor(0)");
    }
    if tree.lower_bound(&3) != Some((&3, &"three")) {
        return fail("rbtree", "lower_bound(3)");
    }
    if tree.upper_bound(&3) != Some((&4, &"four")) {
        return fail("rbtree", "upper_bound(3)");
    }
    if tree.lower_than(&3) != Some((&2, &"two")) {
        return fail("rbtree", "lower_than(3)");
    }

    // --- 插入覆盖 ---
    let old = tree.insert(2, "TWO");
    if old != Some("two") {
        return fail("rbtree", "insert overwrite");
    }
    if tree.get(&2) != Some(&"TWO") {
        return fail("rbtree", "get after overwrite");
    }
    if tree.len() != 5 {
        return fail("rbtree", "len after overwrite");
    }

    // --- 删除 ---
    tree.remove(&2);
    if tree.len() != 4 {
        return fail("rbtree", "remove");
    }
    if tree.get(&2).is_some() {
        return fail("rbtree", "get after remove");
    }

    // --- pop ---
    let first = tree.pop_first();
    if first != Some((1, "one")) {
        return fail("rbtree", "pop_first");
    }
    let last = tree.pop_last();
    if last != Some((5, "five")) {
        return fail("rbtree", "pop_last");
    }
    if tree.len() != 2 {
        return fail("rbtree", "len after pops");
    }

    // --- entry API ---
    tree.entry(10).or_insert("ten");
    if tree.get(&10) != Some(&"ten") {
        return fail("rbtree", "entry vacant");
    }
    tree.entry(10).and_modify(|v| *v = "TEN");
    if tree.get(&10) != Some(&"TEN") {
        return fail("rbtree", "entry modify");
    }

    // --- clone ---
    let tree2 = tree.clone();
    if tree2.len() != tree.len() {
        return fail("rbtree", "clone len");
    }
    if tree2.get(&10) != Some(&"TEN") {
        return fail("rbtree", "clone get");
    }

    // --- retain ---
    let mut t = RbTree::new();
    t.insert(1, 10);
    t.insert(2, 20);
    t.insert(3, 30);
    t.retain(|_, v| *v > 15);
    if t.len() != 2 {
        return fail("rbtree", "retain len");
    }
    if t.contains_key(&1) {
        return fail("rbtree", "retain removed 1");
    }

    // --- 空树 ---
    let empty: RbTree<i32, i32> = RbTree::new();
    if empty.first_key_value().is_some() {
        return fail("rbtree", "empty first");
    }
    if !empty.is_empty() {
        return fail("rbtree", "empty is_empty");
    }

    pass("rbtree");
}
