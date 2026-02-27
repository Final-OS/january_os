use super::{fail, pass};
use alloc::vec;
use alloc::vec::Vec;

pub(super) fn run() {
    use crate::libs::btree::BTree;

    // --- 基本插入与查询 ---
    let mut bt = BTree::new();
    bt.insert(5, "five");
    bt.insert(2, "two");
    bt.insert(8, "eight");
    bt.insert(1, "one");
    bt.insert(9, "nine");

    if bt.len() != 5 {
        return fail("btree", "len != 5");
    }
    if bt.is_empty() {
        return fail("btree", "should not be empty");
    }
    if bt.get(&5) != Some(&"five") {
        return fail("btree", "get(5) failed");
    }
    if bt.get(&1) != Some(&"one") {
        return fail("btree", "get(1) failed");
    }
    if bt.get(&99).is_some() {
        return fail("btree", "get(99) should be None");
    }

    // --- 插入覆盖 ---
    let old = bt.insert(5, "FIVE");
    if old != Some("five") {
        return fail("btree", "insert overwrite");
    }
    if bt.get(&5) != Some(&"FIVE") {
        return fail("btree", "get after overwrite");
    }
    if bt.len() != 5 {
        return fail("btree", "len after overwrite");
    }

    // --- 迭代顺序 ---
    let keys: Vec<_> = bt.iter().map(|(k, _)| *k).collect();
    if keys != vec![1, 2, 5, 8, 9] {
        return fail("btree", "iteration order");
    }

    let values: Vec<_> = bt.iter().map(|(_, v)| *v).collect();
    if values != vec!["one", "two", "FIVE", "eight", "nine"] {
        return fail("btree", "iteration values");
    }

    // --- 删除 ---
    let removed = bt.remove(&2);
    if removed != Some("two") {
        return fail("btree", "remove(2)");
    }
    if bt.len() != 4 {
        return fail("btree", "len after remove");
    }
    if bt.get(&2).is_some() {
        return fail("btree", "get(2) after remove");
    }

    // 删除不存在的键
    if bt.remove(&99).is_some() {
        return fail("btree", "remove non-existent");
    }
    if bt.len() != 4 {
        return fail("btree", "len after remove non-existent");
    }

    // --- 大量插入测试（触发节点分裂）---
    let mut bt2 = BTree::new();
    for i in 0..100 {
        bt2.insert(i, i * 10);
    }
    if bt2.len() != 100 {
        return fail("btree", "bulk insert len");
    }

    // 验证所有值都能正确检索
    for i in 0..100 {
        if bt2.get(&i) != Some(&(i * 10)) {
            return fail("btree", "bulk insert get");
        }
    }

    // 验证迭代顺序
    let keys: Vec<_> = bt2.iter().map(|(k, _)| *k).collect();
    let expected: Vec<_> = (0..100).collect();
    if keys != expected {
        return fail("btree", "bulk insert iteration order");
    }

    // --- 大量删除测试 ---
    for i in (0..100).step_by(2) {
        bt2.remove(&i);
    }
    if bt2.len() != 50 {
        return fail("btree", "bulk remove len");
    }

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
    if !empty.is_empty() {
        return fail("btree", "empty is_empty");
    }
    if empty.len() != 0 {
        return fail("btree", "empty len");
    }
    if empty.get(&1).is_some() {
        return fail("btree", "empty get");
    }

    let count = empty.iter().count();
    if count != 0 {
        return fail("btree", "empty iter count");
    }

    // --- 单元素测试 ---
    let mut single = BTree::new();
    single.insert(42, "answer");
    if single.len() != 1 {
        return fail("btree", "single len");
    }
    if single.get(&42) != Some(&"answer") {
        return fail("btree", "single get");
    }

    let items: Vec<_> = single.iter().collect();
    if items.len() != 1 {
        return fail("btree", "single iter len");
    }
    if items[0] != (&42, &"answer") {
        return fail("btree", "single iter content");
    }

    single.remove(&42);
    if !single.is_empty() {
        return fail("btree", "single after remove");
    }

    // --- 逆序插入测试 ---
    let mut bt3 = BTree::new();
    for i in (0..50).rev() {
        bt3.insert(i, i);
    }
    if bt3.len() != 50 {
        return fail("btree", "reverse insert len");
    }

    let keys: Vec<_> = bt3.iter().map(|(k, _)| *k).collect();
    let expected: Vec<_> = (0..50).collect();
    if keys != expected {
        return fail("btree", "reverse insert order");
    }

    // --- 新增：contains_key 测试 ---
    if !bt3.contains_key(&25) {
        return fail("btree", "contains_key(25)");
    }
    if bt3.contains_key(&100) {
        return fail("btree", "contains_key(100) should be false");
    }

    // --- 新增：get_mut 测试 ---
    if let Some(v) = bt3.get_mut(&25) {
        *v = 250;
    }
    if bt3.get(&25) != Some(&250) {
        return fail("btree", "get_mut modification");
    }

    // --- 新增：first/last 测试 ---
    if bt3.first() != Some((&0, &0)) {
        return fail("btree", "first()");
    }
    if bt3.last() != Some((&49, &49)) {
        return fail("btree", "last()");
    }

    // --- 新增：pop_first/pop_last 测试 ---
    let mut bt4 = BTree::new();
    bt4.insert(1, "a");
    bt4.insert(2, "b");
    bt4.insert(3, "c");

    if bt4.pop_first() != Some((1, "a")) {
        return fail("btree", "pop_first");
    }
    if bt4.len() != 2 {
        return fail("btree", "len after pop_first");
    }

    if bt4.pop_last() != Some((3, "c")) {
        return fail("btree", "pop_last");
    }
    if bt4.len() != 1 {
        return fail("btree", "len after pop_last");
    }

    // --- 新增：clear 测试 ---
    bt4.clear();
    if !bt4.is_empty() {
        return fail("btree", "clear");
    }
    if bt4.len() != 0 {
        return fail("btree", "len after clear");
    }

    // --- 新增：keys/values 迭代器测试 ---
    let mut bt5 = BTree::new();
    bt5.insert(1, "one");
    bt5.insert(2, "two");
    bt5.insert(3, "three");

    let keys: Vec<_> = bt5.keys().copied().collect();
    if keys != vec![1, 2, 3] {
        return fail("btree", "keys iterator");
    }

    let values: Vec<_> = bt5.values().copied().collect();
    if values != vec!["one", "two", "three"] {
        return fail("btree", "values iterator");
    }

    // --- 新增：retain 测试 ---
    let mut bt6 = BTree::new();
    bt6.insert(1, 10);
    bt6.insert(2, 20);
    bt6.insert(3, 30);
    bt6.insert(4, 40);

    bt6.retain(|_, v| *v > 15);
    if bt6.len() != 3 {
        return fail("btree", "retain len");
    }
    if bt6.contains_key(&1) {
        return fail("btree", "retain removed key 1");
    }
    if !bt6.contains_key(&2) {
        return fail("btree", "retain kept key 2");
    }

    // --- 新增：Clone 测试 ---
    let bt7 = bt5.clone();
    if bt7.len() != bt5.len() {
        return fail("btree", "clone len");
    }
    if bt7.get(&2) != Some(&"two") {
        return fail("btree", "clone content");
    }

    // --- 新增：FromIterator 测试 ---
    let bt8: BTree<_, _> = [(1, "x"), (2, "y"), (3, "z")].into_iter().collect();
    if bt8.len() != 3 {
        return fail("btree", "from_iter len");
    }
    if bt8.get(&2) != Some(&"y") {
        return fail("btree", "from_iter content");
    }

    // --- 新增：Extend 测试 ---
    let mut bt9 = BTree::new();
    bt9.insert(1, "a");
    bt9.extend([(2, "b"), (3, "c")]);
    if bt9.len() != 3 {
        return fail("btree", "extend len");
    }

    pass("btree");
}
