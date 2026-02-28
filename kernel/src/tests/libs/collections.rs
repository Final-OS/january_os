use super::{fail, pass};
use crate::hlist_for_each_entry;

pub(super) fn run_ring_buffer() {
    test_ring_buffer();
}

pub(super) fn run_kfifo() {
    test_kfifo();
}

pub(super) fn run_bitmap() {
    test_bitmap();
}

pub(super) fn run_hlist() {
    test_hlist();
}

pub(super) fn run_wait_queue() {
    test_wait_queue();
}

pub(super) fn run_id_allocator() {
    test_id_allocator();
}

pub(super) fn run_sync_once() {
    test_sync_once();
}

pub(super) fn run_sync_blocking() {
    test_sync_blocking();
}

fn test_ring_buffer() {
    use crate::libs::ring_buffer::RingBuffer;

    let mut ring = RingBuffer::with_capacity(4).expect("ring init");
    if !ring.is_empty() {
        return fail("ring_buffer", "should be empty");
    }
    if ring.capacity() != 4 {
        return fail("ring_buffer", "capacity != 4");
    }

    if ring.push(1).is_err() {
        return fail("ring_buffer", "push 1");
    }
    if ring.push(2).is_err() {
        return fail("ring_buffer", "push 2");
    }
    if ring.push(3).is_err() {
        return fail("ring_buffer", "push 3");
    }
    if ring.push(4).is_err() {
        return fail("ring_buffer", "push 4");
    }
    if !ring.is_full() {
        return fail("ring_buffer", "should be full");
    }
    if ring.push(5).is_ok() {
        return fail("ring_buffer", "push on full should fail");
    }

    if ring.peek() != Some(&1) {
        return fail("ring_buffer", "peek");
    }
    if ring.pop() != Some(1) {
        return fail("ring_buffer", "pop 1");
    }
    if ring.pop_back() != Some(4) {
        return fail("ring_buffer", "pop_back 4");
    }

    let dropped = ring.push_overwrite(9);
    if dropped.is_some() {
        return fail("ring_buffer", "overwrite should not drop when not full");
    }

    let mut out = [0; 4];
    let read = ring.read_slice(&mut out);
    if read != 3 {
        return fail("ring_buffer", "read_slice len");
    }
    if out[..3] != [2, 3, 9] {
        return fail("ring_buffer", "read_slice content");
    }

    let written = ring.write_slice(&[7, 8, 9, 10, 11]);
    if written != 4 {
        return fail("ring_buffer", "write_slice len");
    }
    let dropped = ring.push_overwrite(12);
    if dropped != Some(7) {
        return fail("ring_buffer", "push_overwrite dropped");
    }

    pass("ring_buffer");
}

fn test_kfifo() {
    use crate::libs::kfifo::KFifo;

    let mut fifo = KFifo::with_capacity(3).expect("kfifo init");
    if fifo.in_slice(&[1, 2]) != 2 {
        return fail("kfifo", "in_slice 2");
    }
    if fifo.avail() != 2 {
        return fail("kfifo", "avail 2");
    }

    if fifo.push(3).is_err() {
        return fail("kfifo", "push 3");
    }
    if !fifo.is_full() {
        return fail("kfifo", "full");
    }
    if fifo.push(4).is_ok() {
        return fail("kfifo", "push full should fail");
    }

    let mut out = [0; 4];
    let got = fifo.out_slice(&mut out);
    if got != 3 {
        return fail("kfifo", "out_slice len");
    }
    if out[..3] != [1, 2, 3] {
        return fail("kfifo", "out_slice content");
    }

    if fifo.push_overwrite(10).is_some() {
        return fail("kfifo", "overwrite no drop");
    }
    if fifo.push(11).is_err() {
        return fail("kfifo", "push 11");
    }
    if fifo.push(12).is_err() {
        return fail("kfifo", "push 12");
    }
    if fifo.push_overwrite(13) != Some(10) {
        return fail("kfifo", "overwrite drop 10");
    }
    if fifo.pop_back() != Some(13) {
        return fail("kfifo", "pop_back 13");
    }

    fifo.reset();
    if !fifo.is_empty() {
        return fail("kfifo", "reset empty");
    }

    pass("kfifo");
}

fn test_bitmap() {
    use crate::libs::bitmap::Bitmap;

    let mut bm = Bitmap::new(16);
    if bm.count_ones() != 0 {
        return fail("bitmap", "count_ones init");
    }
    if bm.count_zeros() != 16 {
        return fail("bitmap", "count_zeros init");
    }

    if !bm.set(2) || !bm.set(3) {
        return fail("bitmap", "set bits");
    }
    if !bm.test(2) || !bm.test(3) {
        return fail("bitmap", "test set bits");
    }
    if bm.find_first_set() != Some(2) {
        return fail("bitmap", "first set");
    }
    if bm.find_first_zero() != Some(0) {
        return fail("bitmap", "first zero");
    }

    if !bm.set_range(8, 4) {
        return fail("bitmap", "set_range");
    }
    if !bm.test_all_set(8, 4) {
        return fail("bitmap", "test_all_set");
    }
    if bm.find_contiguous_zeros(2) != Some(0) {
        return fail("bitmap", "contiguous from start");
    }
    if bm.find_next_set(4) != Some(8) {
        return fail("bitmap", "find_next_set");
    }

    if !bm.clear_range(8, 4) {
        return fail("bitmap", "clear_range");
    }
    if !bm.test_all_clear(8, 4) {
        return fail("bitmap", "test_all_clear");
    }

    bm.set_all();
    if bm.count_ones() != 16 {
        return fail("bitmap", "set_all count");
    }
    bm.clear_all();
    if bm.count_ones() != 0 {
        return fail("bitmap", "clear_all count");
    }

    pass("bitmap");
}

fn test_hlist() {
    use crate::libs::hlist::{HListHead, HListNode};

    #[repr(C)]
    struct Item {
        id: usize,
        node: HListNode,
    }

    let mut head = HListHead::new();
    head.init();

    let mut a = Item {
        id: 1,
        node: HListNode::new(),
    };
    let mut b = Item {
        id: 2,
        node: HListNode::new(),
    };
    let mut c = Item {
        id: 3,
        node: HListNode::new(),
    };

    unsafe {
        head.add_head(&mut a.node);
        head.add_head(&mut b.node);
        HListNode::add_after(&mut c.node, &mut b.node);
    }

    if head.is_empty() {
        return fail("hlist", "should not be empty");
    }
    if !a.node.is_hashed() || !b.node.is_hashed() || !c.node.is_hashed() {
        return fail("hlist", "hashed state");
    }

    let mut seen = [0usize; 3];
    let mut idx = 0usize;
    hlist_for_each_entry!(&mut head, item, Item, node, {
        if idx < 3 {
            seen[idx] = unsafe { (*item).id };
            idx += 1;
        }
    });
    if seen != [2, 3, 1] {
        return fail("hlist", "iteration order");
    }

    unsafe {
        if !c.node.del() {
            return fail("hlist", "del c");
        }
    }
    if c.node.is_hashed() {
        return fail("hlist", "c should be unhashed");
    }

    unsafe {
        let first = head.pop_front();
        if first.is_null() {
            return fail("hlist", "pop_front null");
        }
    }

    let mut moved = HListHead::new();
    moved.init();
    unsafe {
        moved.move_list(&mut head);
    }
    if !head.is_empty() {
        return fail("hlist", "old head should be empty");
    }

    pass("hlist");
}

fn test_wait_queue() {
    use crate::libs::wait_queue::{WaitMode, WaitQueue, WaitState};

    let mut wq = WaitQueue::new();
    wq.enqueue_mode(10, WaitMode::Exclusive);
    wq.enqueue(20);
    wq.enqueue(30);
    wq.enqueue(20); // duplicate ignored

    if wq.len() != 3 {
        return fail("wait_queue", "len after enqueue");
    }
    if wq.sleeping_count() != 3 {
        return fail("wait_queue", "sleeping count");
    }

    let woke = wq.wake_one().expect("wake one");
    if woke.token != 10 || woke.state != WaitState::Woken {
        return fail("wait_queue", "wake_one result");
    }

    if !wq.mark_interrupted(20) {
        return fail("wait_queue", "interrupt 20");
    }
    let woke = wq.wake_all_if(|entry| entry.token >= 30);
    if woke.len() != 1 || woke[0].token != 30 || woke[0].state != WaitState::Woken {
        return fail("wait_queue", "wake_all_if should wake 30 only");
    }

    let removed = wq.dequeue(20).expect("dequeue interrupted");
    if removed.state != WaitState::Interrupted {
        return fail("wait_queue", "dequeue interrupted state");
    }

    if !wq.is_empty() {
        return fail("wait_queue", "should be empty");
    }

    pass("wait_queue");
}

fn test_id_allocator() {
    use crate::libs::id_allocator::{IdAllocError, IdAllocator};

    let mut ida = IdAllocator::new(100, 8).expect("id alloc init");
    let id0 = ida.alloc().expect("alloc 100");
    let id1 = ida.alloc().expect("alloc 101");
    if (id0, id1) != (100, 101) {
        return fail("id_allocator", "sequential alloc");
    }

    if ida.alloc_specific(104).is_err() {
        return fail("id_allocator", "alloc_specific 104");
    }
    if ida.alloc_specific(104) != Err(IdAllocError::AlreadyAllocated) {
        return fail("id_allocator", "alloc_specific duplicate");
    }

    let range = ida.alloc_range(2).expect("alloc_range 2");
    if range != 102 {
        return fail("id_allocator", "alloc_range start");
    }

    if ida.is_allocated(106) {
        return fail("id_allocator", "106 should be free");
    }
    if ida.first_free() != Some(105) {
        return fail("id_allocator", "first_free");
    }
    if ida.first_allocated() != Some(100) {
        return fail("id_allocator", "first_allocated");
    }

    if ida.free(101).is_err() {
        return fail("id_allocator", "free 101");
    }
    if ida.free(101) != Err(IdAllocError::NotAllocated) {
        return fail("id_allocator", "free unallocated");
    }

    if ida.free_range(102, 2).is_err() {
        return fail("id_allocator", "free_range 102..104");
    }
    if ida.allocated() != 2 {
        return fail("id_allocator", "allocated count after free range");
    }

    ida.clear();
    if ida.allocated() != 0 || ida.available() != 8 {
        return fail("id_allocator", "clear stats");
    }

    pass("id_allocator");
}

fn test_sync_once() {
    use crate::sync::{Once, OnceCell};

    let once = Once::new();
    let mut once_attempts = 0usize;

    if once
        .call_once_try(|| -> Result<(), i32> {
            once_attempts += 1;
            Err(7)
        })
        .is_ok()
    {
        return fail("sync_once", "call_once_try error path");
    }
    if once.is_completed() {
        return fail("sync_once", "once should remain incomplete after error");
    }

    if once
        .call_once_try(|| -> Result<(), i32> {
            once_attempts += 1;
            Ok(())
        })
        .is_err()
    {
        return fail("sync_once", "call_once_try success path");
    }
    if !once.is_completed() {
        return fail("sync_once", "once should be completed after success");
    }
    if once
        .call_once_try(|| -> Result<(), i32> {
            once_attempts += 100;
            Ok(())
        })
        .is_err()
    {
        return fail("sync_once", "call_once_try completed fast path");
    }
    if once_attempts != 2 {
        return fail("sync_once", "unexpected once attempt count");
    }

    let cell: OnceCell<u32> = OnceCell::new();
    let mut cell_attempts = 0usize;

    let first = cell.get_or_try_init(|| -> Result<u32, i32> {
        cell_attempts += 1;
        Err(22)
    });
    if first != Err(22) {
        return fail("sync_once", "once_cell first try should fail");
    }
    if cell.get().is_some() {
        return fail("sync_once", "once_cell should stay uninitialized after failure");
    }

    let second = cell.get_or_try_init(|| -> Result<u32, i32> {
        cell_attempts += 1;
        Ok(42)
    });
    match second {
        Ok(v) if *v == 42 => {}
        _ => return fail("sync_once", "once_cell second try should succeed"),
    }

    let third = cell.get_or_try_init(|| -> Result<u32, i32> {
        cell_attempts += 100;
        Ok(777)
    });
    match third {
        Ok(v) if *v == 42 => {}
        _ => return fail("sync_once", "once_cell completed fast path"),
    }

    if cell_attempts != 2 {
        return fail("sync_once", "unexpected once_cell attempt count");
    }

    pass("sync_once");
}

fn test_sync_blocking() {
    use crate::sync::{CondVar, Mutex, Semaphore};

    let m = Mutex::new(7usize);
    {
        let mut g = m.lock_blocking();
        *g = g.saturating_add(1);
    }
    {
        let g = m.lock_blocking();
        if *g != 8 {
            return fail("sync_blocking", "mutex lock_blocking unexpected value");
        }
    }

    let sem = Semaphore::new(1);
    sem.acquire_blocking();
    sem.release();
    sem.acquire_many_blocking(1);
    sem.release_many(1);

    let cv = CondVar::new();
    cv.notify_one();
    cv.notify_all();

    let guard = m.lock_blocking();
    let guard = cv.wait_while(&m, guard, |_v| false);
    if *guard != 8 {
        return fail("sync_blocking", "condvar wait_while changed guarded value");
    }

    pass("sync_blocking");
}
