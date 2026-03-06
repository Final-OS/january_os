use super::*;

pub(crate) fn read_tty_byte() -> Option<u8> {
    interrupt::read_char()
        .or_else(serial_read_char)
        .or_else(keyboard::read_char)
        .map(|b| if b == b'\r' { b'\n' } else { b })
}

#[inline]
pub(crate) fn stdin_has_pending_input() -> bool {
    drivers::input::has_char() || crate::drivers::tty::serial_has_input()
}

#[inline]
pub(crate) fn enqueue_current_stdin_waiter() -> bool {
    let Some(task_ref) = task::current_task() else {
        return false;
    };

    let tid = {
        let task = task_ref.lock();
        task.id
    };

    {
        let mut waiters = STDIN_WAITERS.lock();
        waiters.enqueue_mode(tid.0, WaitMode::Exclusive);
    }

    let mut task = task_ref.lock();
    if task.status == task::TaskStatus::Exited {
        let mut waiters = STDIN_WAITERS.lock();
        let _ = waiters.dequeue(tid.0);
        return false;
    }
    task.status = task::TaskStatus::Blocked;
    true
}

#[inline]
pub(crate) fn dequeue_current_stdin_waiter() {
    if let Some(tid) = task::current_tid() {
        let mut waiters = STDIN_WAITERS.lock();
        let _ = waiters.dequeue(tid.0);
    }
}

pub(crate) fn wake_stdin_waiters_if_ready() -> usize {
    if !stdin_has_pending_input() {
        return 0;
    }

    let token = {
        let mut waiters = STDIN_WAITERS.lock();
        waiters.wake_one().map(|entry| entry.token)
    };

    let Some(token) = token else {
        return 0;
    };

    let tid = task::TaskId(token);
    let Some(task_ref) = task::find_task_by_tid(tid) else {
        return 0;
    };

    let mut should_enqueue = false;
    {
        let mut task = task_ref.lock();
        if task.status == task::TaskStatus::Blocked {
            task.status = task::TaskStatus::Ready;
            should_enqueue = true;
        }
    }

    if should_enqueue {
        task::sched::SCHEDULER.add_task(task_ref);
        1
    } else {
        0
    }
}
