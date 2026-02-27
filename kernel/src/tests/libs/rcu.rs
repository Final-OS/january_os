use super::{fail, pass};
use crate::kprintln;

pub(super) fn run() {
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
        if *guard != 42 {
            return fail("rcu/basic", "initial read != 42");
        }
    }

    kprintln!("    [RCU/basic] Peeking value...");
    let ptr = rcu.peek();
    unsafe {
        if *ptr != 42 {
            return fail("rcu/basic", "peek != 42");
        }
    }

    kprintln!("    [RCU/basic] Checking reader count...");
    if rcu.active_readers() != 0 {
        return fail("rcu/basic", "should have 0 readers");
    }
    if !rcu.is_quiescent() {
        return fail("rcu/basic", "should be quiescent");
    }

    {
        let _guard = rcu.read();
        if rcu.active_readers() != 1 {
            return fail("rcu/basic", "should have 1 reader");
        }
        if rcu.is_quiescent() {
            return fail("rcu/basic", "should not be quiescent");
        }
    }

    if rcu.active_readers() != 0 {
        return fail("rcu/basic", "readers should be 0 after drop");
    }
    kprintln!("    [RCU/basic] PASSED");
}

fn test_rcu_updates() {
    use crate::libs::rcu::Rcu;

    kprintln!("    [RCU/update] Testing update...");
    let rcu = Rcu::new(10u32);

    let old = rcu.update(20);
    if old != 10 {
        return fail("rcu/update", "update old != 10");
    }
    {
        let guard = rcu.read();
        if *guard != 20 {
            return fail("rcu/update", "read after update != 20");
        }
    }

    kprintln!("    [RCU/update] Testing update_with...");
    let old = rcu.update_with(|val| *val * 2);
    if old != 20 {
        return fail("rcu/update", "update_with old != 20");
    }
    {
        let guard = rcu.read();
        if *guard != 40 {
            return fail("rcu/update", "read after update_with != 40");
        }
    }

    kprintln!("    [RCU/update] Testing try_update (no readers)...");
    let result = rcu.try_update(50);
    if result.is_err() {
        return fail("rcu/update", "try_update should succeed");
    }
    {
        let guard = rcu.read();
        if *guard != 50 {
            return fail("rcu/update", "read after try_update != 50");
        }
    }

    kprintln!("    [RCU/update] Testing try_update behavior...");
    // 注意：在单线程环境中，不能在持有读守卫时调用 try_update，
    // 因为 try_update 会调用 synchronize_rcu 等待读者完成，导致死锁。
    // 这里只测试 try_update 的基本功能。
    let result = rcu.try_update(60);
    if result.is_err() {
        return fail("rcu/update", "try_update should succeed");
    }

    kprintln!("    [RCU/update] Creating new RCU for update_async test...");
    // 使用新的 RCU 实例避免状态问题
    let rcu2 = Rcu::new(100u32);

    kprintln!("    [RCU/update] Testing update_async...");
    kprintln!(
        "    [RCU/update] Active readers before update_async: {}",
        rcu2.active_readers()
    );

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

    if rcu.active_readers() != 3 {
        return fail("rcu/readers", "should have 3 readers");
    }
    if *guard1 != 100 {
        return fail("rcu/readers", "guard1 != 100");
    }
    if *guard2 != 100 {
        return fail("rcu/readers", "guard2 != 100");
    }
    if *guard3 != 100 {
        return fail("rcu/readers", "guard3 != 100");
    }

    drop(guard1);
    if rcu.active_readers() != 2 {
        return fail("rcu/readers", "should have 2 readers after drop");
    }

    drop(guard2);
    drop(guard3);
    if rcu.active_readers() != 0 {
        return fail("rcu/readers", "should have 0 readers");
    }

    kprintln!("    [RCU/readers] Testing try_read...");
    let guard = rcu.try_read();
    if guard.is_none() {
        return fail("rcu/readers", "try_read should succeed");
    }
    drop(guard);

    kprintln!("    [RCU/readers] Testing reader isolation...");
    let guard = rcu.read();
    let old_val = *guard;
    // 单线程环境下，持有读守卫时调用 update() 会在 synchronize_rcu() 自旋等待自己释放读者，
    // 导致必然死锁。这里改用 update_async 验证“旧读者视图不变，新读者可见新值”。
    if rcu.update_async(200).is_err() {
        return fail("rcu/readers", "update_async should succeed");
    }
    if *guard != old_val {
        return fail("rcu/readers", "reader should see old value during update");
    }
    drop(guard);
    rcu.synchronize_rcu();

    let guard = rcu.read();
    if *guard != 200 {
        return fail("rcu/readers", "new reader should see new value");
    }

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
        if *guard != 100 {
            return fail("rcu/ownership", "get_mut modification failed");
        }
    }

    kprintln!("    [RCU/ownership] Testing into_inner...");
    let value = rcu.into_inner();
    if value != 100 {
        return fail("rcu/ownership", "into_inner != 100");
    }

    kprintln!("    [RCU/ownership] PASSED");
}

fn test_rcu_edge_cases() {
    use crate::libs::rcu::Rcu;

    kprintln!("    [RCU/edge] Testing ZST...");
    let rcu = Rcu::new(());
    {
        let guard = rcu.read();
        if *guard != () {
            return fail("rcu/edge", "ZST read failed");
        }
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
        if guard.data[0] != 42 {
            return fail("rcu/edge", "large struct read failed");
        }
    }

    kprintln!("    [RCU/edge] Testing consecutive updates...");
    let rcu = Rcu::new(0u32);
    for i in 1..=10 {
        rcu.update(i);
    }
    {
        let guard = rcu.read();
        if *guard != 10 {
            return fail("rcu/edge", "final value != 10");
        }
    }

    kprintln!("    [RCU/edge] Testing reader during updates...");
    let rcu = Rcu::new(0u32);
    let guard = rcu.read();
    let initial = *guard;
    if rcu.update_async(1).is_err() {
        return fail("rcu/edge", "update_async(1) should succeed");
    }
    if rcu.update_async(2).is_err() {
        return fail("rcu/edge", "update_async(2) should succeed");
    }
    if rcu.update_async(3).is_err() {
        return fail("rcu/edge", "update_async(3) should succeed");
    }
    if *guard != initial {
        return fail("rcu/edge", "reader should see initial value");
    }
    drop(guard);
    rcu.synchronize_rcu();

    let guard = rcu.read();
    if *guard != 3 {
        return fail("rcu/edge", "new reader should see latest value");
    }

    kprintln!("    [RCU/edge] PASSED");
}
