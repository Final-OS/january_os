use super::{fail, pass};
use alloc::vec;
use alloc::vec::Vec;

pub(super) fn run() {
    test_maple_basic();
    test_maple_operations();
    test_maple_gap_search();
    test_maple_advanced();
    test_maple_edge_cases();
    pass("mptree");
}

fn test_maple_basic() {
    use crate::libs::mptree::{MapleInsertError, MapleTree};

    // 基本插入和查找
    let mut mt = MapleTree::new();
    if mt.insert(0, 100, "a").is_err() {
        return fail("mptree/basic", "insert a");
    }
    if mt.insert(200, 300, "b").is_err() {
        return fail("mptree/basic", "insert b");
    }

    if mt.len() != 2 {
        return fail("mptree/basic", "len != 2");
    }
    if mt.is_empty() {
        return fail("mptree/basic", "should not be empty");
    }

    // find
    if mt.find(50) != Some((0, 100, &"a")) {
        return fail("mptree/basic", "find(50)");
    }
    if mt.find(150).is_some() {
        return fail("mptree/basic", "find(150) should be None");
    }
    if mt.find(250) != Some((200, 300, &"b")) {
        return fail("mptree/basic", "find(250)");
    }

    // contains
    if !mt.contains(50) {
        return fail("mptree/basic", "contains(50)");
    }
    if mt.contains(150) {
        return fail("mptree/basic", "contains(150)");
    }

    // contains_start
    if !mt.contains_start(0) {
        return fail("mptree/basic", "contains_start(0)");
    }
    if mt.contains_start(50) {
        return fail("mptree/basic", "contains_start(50)");
    }

    // 重叠检测
    if mt.insert(50, 150, "c") != Err(MapleInsertError::Overlap) {
        return fail("mptree/basic", "overlap not detected");
    }
    if mt.insert(300, 200, "d") != Err(MapleInsertError::InvalidRange) {
        return fail("mptree/basic", "invalid range not detected");
    }

    // remove
    if mt.remove(0) != Some((100, "a")) {
        return fail("mptree/basic", "remove(0)");
    }
    if mt.len() != 1 {
        return fail("mptree/basic", "len after remove");
    }
    if mt.find(50).is_some() {
        return fail("mptree/basic", "find after remove");
    }

    // clear
    mt.clear();
    if !mt.is_empty() {
        return fail("mptree/basic", "clear");
    }
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
    if mt.find(50) != Some((0, 100, &"A")) {
        return fail("mptree/ops", "get_mut");
    }

    // get_mut_at
    if let Some((end, val)) = mt.get_mut_at(0) {
        if end != 100 {
            return fail("mptree/ops", "get_mut_at end");
        }
        *val = "AA";
    }
    if mt.find(50) != Some((0, 100, &"AA")) {
        return fail("mptree/ops", "get_mut_at");
    }

    // update_end
    if mt.update_end(200, 350).is_err() {
        return fail("mptree/ops", "update_end");
    }
    if mt.find(320) != Some((200, 350, &"b")) {
        return fail("mptree/ops", "find after update_end");
    }
    mt.update_end(200, 300).unwrap(); // 恢复

    // replace
    let old = mt.replace(0, 120, "a2");
    if old != Ok(Some((100, "AA"))) {
        return fail("mptree/ops", "replace");
    }
    if mt.find(110) != Some((0, 120, &"a2")) {
        return fail("mptree/ops", "find after replace");
    }

    // insert_overwrite
    let removed = mt.insert_overwrite(50, 250, "ow");
    match removed {
        Ok(v) => {
            if v.len() != 2 {
                return fail("mptree/ops", "overwrite removed count");
            }
        }
        Err(_) => return fail("mptree/ops", "insert_overwrite failed"),
    }
    if mt.find(100) != Some((50, 250, &"ow")) {
        return fail("mptree/ops", "find after overwrite");
    }

    // split_at
    mt.clear();
    mt.insert(0, 100, "s").unwrap();
    if !mt.split_at(50) {
        return fail("mptree/ops", "split_at");
    }
    if mt.len() != 2 {
        return fail("mptree/ops", "len after split");
    }
    if mt.find(25) != Some((0, 50, &"s")) {
        return fail("mptree/ops", "left half");
    }
    if mt.find(75) != Some((50, 100, &"s")) {
        return fail("mptree/ops", "right half");
    }

    // merge_adjacent_equal
    mt.clear();
    mt.insert(0, 10, "m").unwrap();
    mt.insert(10, 20, "m").unwrap();
    mt.insert(20, 30, "m").unwrap();
    let merges = mt.merge_adjacent_equal();
    if merges != 2 {
        return fail("mptree/ops", "merge count");
    }
    if mt.len() != 1 {
        return fail("mptree/ops", "len after merge");
    }
    if mt.find(15) != Some((0, 30, &"m")) {
        return fail("mptree/ops", "merged interval");
    }
}

fn test_maple_gap_search() {
    use crate::libs::mptree::MapleTree;

    let mut mt = MapleTree::new();
    mt.insert(0, 100, "a").unwrap();
    mt.insert(200, 300, "b").unwrap();

    // find_gap 正向
    if let Some(start) = mt.find_gap(100, 0, usize::MAX) {
        if start != 100 {
            return fail("mptree/gap", "gap start != 100");
        }
    } else {
        return fail("mptree/gap", "find_gap failed");
    }

    // find_gap 反向
    if let Some(start) = mt.find_gap_reverse(100, 400, 0) {
        if start != 300 {
            return fail("mptree/gap", "gap_rev start != 300");
        }
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
    if mt.first() != Some((0, 100, &"a")) {
        return fail("mptree/adv", "first");
    }
    if mt.last() != Some((400, 500, &"c")) {
        return fail("mptree/adv", "last");
    }

    // pop_first/pop_last
    let first = mt.pop_first();
    if first != Some((0, 100, "a")) {
        return fail("mptree/adv", "pop_first");
    }
    if mt.len() != 2 {
        return fail("mptree/adv", "len after pop_first");
    }

    let last = mt.pop_last();
    if last != Some((400, 500, "c")) {
        return fail("mptree/adv", "pop_last");
    }
    if mt.len() != 1 {
        return fail("mptree/adv", "len after pop_last");
    }

    // starts
    mt.clear();
    mt.insert(10, 20, "x").unwrap();
    mt.insert(30, 40, "y").unwrap();
    let starts: Vec<_> = mt.starts().collect();
    if starts != vec![10, 30] {
        return fail("mptree/adv", "starts");
    }

    // values
    let values: Vec<_> = mt.values().copied().collect();
    if values != vec!["x", "y"] {
        return fail("mptree/adv", "values");
    }

    // values_mut
    for val in mt.values_mut() {
        *val = "z";
    }
    if mt.find(15) != Some((10, 20, &"z")) {
        return fail("mptree/adv", "values_mut");
    }

    // iter
    let items: Vec<_> = mt.iter().collect();
    if items.len() != 2 {
        return fail("mptree/adv", "iter len");
    }

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
    if intersecting.len() != 2 {
        return fail("mptree/adv", "intersecting len");
    }

    // has_intersection
    if !mt.has_intersection(50, 250) {
        return fail("mptree/adv", "has_intersection true");
    }
    if mt.has_intersection(100, 200) {
        return fail("mptree/adv", "has_intersection false");
    }

    // remove_intersecting
    let removed = mt.remove_intersecting(50, 250);
    if removed.len() != 2 {
        return fail("mptree/adv", "remove_intersecting len");
    }
    if mt.len() != 1 {
        return fail("mptree/adv", "len after remove_intersecting");
    }

    // total_length
    mt.clear();
    mt.insert(0, 100, "a").unwrap();
    mt.insert(200, 300, "b").unwrap();
    if mt.total_length() != 200 {
        return fail("mptree/adv", "total_length");
    }

    // retain
    mt.insert(400, 500, "c").unwrap();
    mt.retain(|s, e, _| e - s >= 100);
    if mt.len() != 3 {
        return fail("mptree/adv", "retain len");
    }

    mt.retain(|s, _, _| s < 300);
    if mt.len() != 2 {
        return fail("mptree/adv", "retain len 2");
    }
}

fn test_maple_edge_cases() {
    use crate::libs::mptree::MapleTree;

    // 空树
    let mt: MapleTree<i32> = MapleTree::new();
    if !mt.is_empty() {
        return fail("mptree/edge", "empty is_empty");
    }
    if mt.find(0).is_some() {
        return fail("mptree/edge", "empty find");
    }
    if mt.first().is_some() {
        return fail("mptree/edge", "empty first");
    }
    if mt.last().is_some() {
        return fail("mptree/edge", "empty last");
    }

    // 单区间
    let mut mt = MapleTree::new();
    mt.insert(10, 20, "single").unwrap();
    if mt.len() != 1 {
        return fail("mptree/edge", "single len");
    }
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
    if mt.len() != 3 {
        return fail("mptree/edge", "adjacent len");
    }
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
    if mt2.len() != mt.len() {
        return fail("mptree/edge", "clone len");
    }
    if mt2.find(1000) != Some((0, usize::MAX / 2, &"large")) {
        return fail("mptree/edge", "clone content");
    }

    // Default
    let mt3: MapleTree<i32> = MapleTree::default();
    if !mt3.is_empty() {
        return fail("mptree/edge", "default");
    }
}
