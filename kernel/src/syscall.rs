use crate::arch_impl::{common_per_cpu_data, common_per_cpu_data_mut};
use core::{slice, str};
use libpebble::{caps::Capability, syscall};
use log::{info, trace, warn};

/// This is the architecture-independent syscall handler. It should be called by the handler that
/// receives the syscall (each architecture is free to do this however it wishes). The only
/// parameter that is guaranteed to be valid is `number`; the meaning of the rest may be undefined
/// depending on how many parameters the specific system call takes.
///
/// It is defined as using the C ABI, so an architecture can call it stably from assembly if it
/// wants to.
#[no_mangle]
pub extern "C" fn rust_syscall_handler(
    number: usize,
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
) -> usize {
    info!("Syscall! number = {}, a = {}, b = {}, c = {}, d = {}, e = {}", number, a, b, c, d, e);

    match number {
        syscall::SYSCALL_YIELD => yield_syscall(),
        syscall::SYSCALL_EARLY_LOG => {
            /*
             * a = length of string in bytes (must be <= 1024)
             * b = pointer to string in userspace
             */
            early_log(a, b)
        }

        _ => {
            // TODO: unsupported system call number, kill process or something?
            warn!("Process made system call with invalid syscall number: {}", number);
            1
        }
    }
}

fn yield_syscall() -> usize {
    /*
     * This is a fairly unique system call in that it can return into a different context than the
     * one that called it. We ask the scheduler to move us to the next task, then return to the new
     * userspace context.
     */
    info!("Process yielded!");
    unsafe {
        common_per_cpu_data_mut().scheduler.switch_to_next();
    }

    0
}

fn early_log(str_length: usize, str_address: usize) -> usize {
    /*
     * Returns:
     *      0 => message was successfully logged
     *      1 => message was too long
     *      2 => string was not valid UTF-8
     *      3 => task doesn't have `EarlyLogging` capability
     *
     * TODO: check that b is a valid userspace pointer and that it's mapped to physical
     * memory
     * TODO: log the process ID / name to help identify stuff
     */

    // Check the current task has the `EarlyLogging` capability
    if !unsafe { common_per_cpu_data() }
        .running_task()
        .object
        .task()
        .unwrap()
        .read()
        .capabilities
        .contains(&Capability::EarlyLogging)
    {
        return 3;
    }

    // Check if the message is too long
    if str_length > 1024 {
        return 1;
    }

    // Check the message is valid UTF-8
    let message = match str::from_utf8(unsafe { slice::from_raw_parts(str_address as *const u8, str_length) })
    {
        Ok(message) => message,
        Err(_) => return 2,
    };

    trace!("Userspace task early log message: {}", message);
    0
}
