mod bootstrap;
mod coreutils;
mod sh;

fn should_boot_into_kernel_shell(init_cmd: &str) -> bool {
    let cmd = init_cmd.trim();
    matches!(cmd, "ksh")
}

pub fn run(init_cmd: &str) -> ! {
    if should_boot_into_kernel_shell(init_cmd) {
        sh::run();
    }
    bootstrap::run_boot_user_init(init_cmd);
}

pub fn execute_kernel_command(line: &str) {
    sh::execute_kernel_command(line);
}
