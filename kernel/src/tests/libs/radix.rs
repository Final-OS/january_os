use super::{fail, pass};
use alloc::vec;
use alloc::vec::Vec;

pub(super) fn run() {
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

    if rt.len() != 3 {
        return fail("rdtree/basic", "len != 3");
    }
    if rt.get(50) != Some(&"fifty") {
        return fail("rdtree/basic", "get(50) failed");
    }
    if rt.contains(100) != true {
        return fail("rdtree/basic", "contains(100)");
    }
    if rt.contains(99) != false {
        return fail("rdtree/basic", "contains(99)");
    }

    // get_mut
    if let Some(val) = rt.get_mut(50) {
        *val = "FIFTY";
    }
    if rt.get(50) != Some(&"FIFTY") {
        return fail("rdtree/basic", "get_mut");
    }

    // get_key_value
    if rt.get_key_value(100) != Some((100, &"hundred")) {
        return fail("rdtree/basic", "get_key_value");
    }

    // remove
    rt.remove(50);
    if rt.contains(50) {
        return fail("rdtree/basic", "remove failed");
    }
    if rt.len() != 2 {
        return fail("rdtree/basic", "len after remove");
    }

    // remove_entry
    if rt.remove_entry(100) != Some((100, "hundred")) {
        return fail("rdtree/basic", "remove_entry");
    }

    // clear
    rt.clear();
    if !rt.is_empty() {
        return fail("rdtree/basic", "clear");
    }
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
    if rt.first() != Some((10, &"ten")) {
        return fail("rdtree/ops", "first");
    }
    if rt.last() != Some((30, &"thirty")) {
        return fail("rdtree/ops", "last");
    }

    // pop_first/pop_last
    let first = rt.pop_first();
    if first != Some((10, "ten")) {
        return fail("rdtree/ops", "pop_first");
    }
    if rt.len() != 2 {
        return fail("rdtree/ops", "len after pop_first");
    }

    let last = rt.pop_last();
    if last != Some((30, "thirty")) {
        return fail("rdtree/ops", "pop_last");
    }
    if rt.len() != 1 {
        return fail("rdtree/ops", "len after pop_last");
    }

    // get_or_insert
    let val = rt.get_or_insert(40, "forty");
    *val = "FORTY";
    if rt.get(40) != Some(&"FORTY") {
        return fail("rdtree/ops", "get_or_insert");
    }

    // get_or_insert_with
    let val = rt.get_or_insert_with(50, || "fifty");
    if *val != "fifty" {
        return fail("rdtree/ops", "get_or_insert_with");
    }

    // get_or_default
    let val = rt.get_or_default(60);
    if *val != "" {
        return fail("rdtree/ops", "get_or_default");
    }
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
    if items.len() != 2 {
        return fail("rdtree/range", "range len");
    }
    if items[0] != (20, &"twenty") {
        return fail("rdtree/range", "range[0]");
    }

    // remove_range
    let removed = rt.remove_range(15, 35);
    if removed.len() != 2 {
        return fail("rdtree/range", "remove_range len");
    }
    if rt.len() != 2 {
        return fail("rdtree/range", "len after remove_range");
    }

    // split_off
    rt.clear();
    rt.insert(1, "a");
    rt.insert(2, "b");
    rt.insert(3, "c");
    rt.insert(4, "d");

    let mut rt2 = rt.split_off(3);
    if rt.len() != 2 {
        return fail("rdtree/range", "split_off left len");
    }
    if rt2.len() != 2 {
        return fail("rdtree/range", "split_off right len");
    }
    if !rt.contains(1) || !rt.contains(2) {
        return fail("rdtree/range", "split_off left content");
    }
    if !rt2.contains(3) || !rt2.contains(4) {
        return fail("rdtree/range", "split_off right content");
    }

    // append
    rt.append(&mut rt2);
    if rt.len() != 4 {
        return fail("rdtree/range", "append len");
    }
    if rt2.len() != 0 {
        return fail("rdtree/range", "append other len");
    }

    // retain
    rt.retain(|k, _| k % 2 == 0);
    if rt.len() != 2 {
        return fail("rdtree/range", "retain len");
    }
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
    if items.len() != 3 {
        return fail("rdtree/iter", "iter len");
    }
    if items[0] != (1, &"a") {
        return fail("rdtree/iter", "iter[0]");
    }

    // keys
    let keys: Vec<_> = rt.keys().collect();
    if keys != vec![1, 2, 3] {
        return fail("rdtree/iter", "keys");
    }

    // values
    let values: Vec<_> = rt.values().copied().collect();
    if values != vec!["a", "b", "c"] {
        return fail("rdtree/iter", "values");
    }

    // values_mut
    for val in rt.values_mut() {
        *val = "x";
    }
    if rt.get(1) != Some(&"x") {
        return fail("rdtree/iter", "values_mut");
    }

    // iter_mut
    for (k, v) in rt.iter_mut() {
        if k == 2 {
            *v = "y";
        }
    }
    if rt.get(2) != Some(&"y") {
        return fail("rdtree/iter", "iter_mut");
    }

    // IntoIterator for &RadixTree
    let mut count = 0;
    for (k, v) in &rt {
        count += 1;
        if k == 1 && v != &"x" {
            return fail("rdtree/iter", "into_iter ref");
        }
    }
    if count != 3 {
        return fail("rdtree/iter", "into_iter ref count");
    }
}

fn test_radix_edge_cases() {
    use crate::libs::rdtree::RadixTree;

    // 空树
    let rt: RadixTree<i32> = RadixTree::new();
    if !rt.is_empty() {
        return fail("rdtree/edge", "empty is_empty");
    }
    if rt.first().is_some() {
        return fail("rdtree/edge", "empty first");
    }
    if rt.last().is_some() {
        return fail("rdtree/edge", "empty last");
    }
    if rt.find_first_gap_from(0) != Some(0) {
        return fail("rdtree/edge", "empty gap");
    }

    // 单元素
    let mut rt = RadixTree::new();
    rt.insert(42, "answer");
    if rt.len() != 1 {
        return fail("rdtree/edge", "single len");
    }
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
    if rt.len() != 2 {
        return fail("rdtree/edge", "large key len");
    }
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
    if rt2.len() != 3 {
        return fail("rdtree/edge", "from_iter len");
    }

    // Extend
    let mut rt3 = RadixTree::new();
    rt3.extend([(1, "a"), (2, "b")]);
    if rt3.len() != 2 {
        return fail("rdtree/edge", "extend len");
    }

    // Clone
    let rt4 = rt3.clone();
    if rt4.len() != rt3.len() {
        return fail("rdtree/edge", "clone len");
    }
    if rt4.get(1) != Some(&"a") {
        return fail("rdtree/edge", "clone content");
    }
}
