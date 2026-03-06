use super::*;

pub(crate) fn sys_execve(args: &SyscallArgs) -> SyscallRet {
    let path_ptr = args.arg0;
    let argv_ptr = args.arg1;
    let envp_ptr = args.arg2;

    let (path, argv, envp) = match parse_execve_payload(path_ptr, argv_ptr, envp_ptr) {
        Ok(payload) => payload,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] parse failed errno={} path_ptr={:#x} argv_ptr={:#x} envp_ptr={:#x}",
                    errno,
                    path_ptr,
                    argv_ptr,
                    envp_ptr
                );
            }
            return err(errno);
        }
    };

    let pid = match task::current_pid() {
        Some(pid) => pid.0,
        None => return err(ESRCH),
    };

    let image = match fs::runtime::read_all_for_pid(pid, path.as_str()) {
        Ok(image) => image,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] executable image not found path={}",
                    path
                );
            }
            return err(errno);
        }
    };

    let load_plan = match task::build_elf_load_plan(image.as_slice()) {
        Ok(plan) => plan,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] invalid elf path={} errno={} image_len={}",
                    path,
                    errno,
                    image.len()
                );
            }
            return err(errno);
        }
    };

    let map_preview = task::preview_pt_load_mapping(&load_plan);

    let staged_mappings = match task::stage_pt_load_mappings(image.as_slice(), &load_plan) {
        Ok(mapped) => mapped,
        Err(errno) => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] stage PT_LOAD failed path={} errno={} segs={} pages={}",
                    path,
                    errno,
                    map_preview.segment_count,
                    map_preview.total_pages,
                );
            }
            return err(errno);
        }
    };

    if task::record_current_exec_request(path.as_str(), argv.len(), envp.len()).is_none() {
        if crate::config::DEBUG_VERBOSE {
            crate::kprintln!(
                "\x1b[90m[diag]\x1b[0m[execve] current process missing while path={} argc={} envc={}",
                path,
                argv.len(),
                envp.len()
            );
        }
        task::rollback_exec_mappings(&staged_mappings);
        return err(ESRCH);
    }

    let argv0 = argv.first().map(|arg| arg.as_str()).unwrap_or("");
    let mapped_segment_pages = staged_mappings
        .iter()
        .filter(|page| page.kind == task::ExecMappedPageKind::Segment)
        .count();
    let mapped_stack_pages = staged_mappings
        .iter()
        .filter(|page| page.kind == task::ExecMappedPageKind::Stack)
        .count();

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] accepted request path={} argc={} envc={} argv0={}",
            path,
            argv.len(),
            envp.len(),
            argv0
        );
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] elf plan path={} image_len={} entry={:#x} segs={} seg_pages={} stack_pages={} total_pages={}",
            path,
            load_plan.image_len,
            load_plan.entry,
            map_preview.segment_count,
            map_preview.segment_pages,
            map_preview.stack_pages,
            map_preview.total_pages,
        );
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] stage mapping done path={} mapped_segment_pages={} mapped_stack_pages={} first_virt={:#x}",
            path,
            mapped_segment_pages,
            mapped_stack_pages,
            staged_mappings.first().map(|page| page.virt).unwrap_or(0),
        );
    }

    let staged_count = staged_mappings.len();
    if let Err(errno) = task::install_current_exec_vmas(&load_plan) {
        task::rollback_exec_mappings(&staged_mappings);
        return err(errno);
    }
    let replaced_pages = match task::set_current_exec_mappings(staged_mappings) {
        Some(replaced) => replaced,
        None => {
            if crate::config::DEBUG_VERBOSE {
                crate::kprintln!(
                    "\x1b[90m[diag]\x1b[0m[execve] install mappings failed path={} staged_pages={}",
                    path,
                    staged_count,
                );
            }
            return err(ESRCH);
        }
    };

    let mut argv_refs: Vec<&str> = if argv.is_empty() {
        vec![path.as_str()]
    } else {
        argv.iter().map(|arg| arg.as_str()).collect()
    };
    let envp_refs: Vec<&str> = envp.iter().map(|arg| arg.as_str()).collect();

    let user_rsp = match task::setup_initial_user_stack(
        load_plan.stack_top,
        load_plan.stack_pages,
        argv_refs.as_slice(),
        envp_refs.as_slice(),
    ) {
        Ok(rsp) => rsp,
        Err(errno) => return err(errno),
    };
    let user_frame = task::arch::build_user_enter_frame(load_plan.entry, user_rsp);

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] user frame rip={:#x} rsp={:#x} cs={:#x} ss={:#x} rflags={:#x}",
            user_frame.rip,
            user_frame.rsp,
            user_frame.cs,
            user_frame.ss,
            user_frame.rflags
        );
    }

    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] mappings installed path={} staged_pages={} replaced_pages={}",
            path,
            staged_count,
            replaced_pages,
        );
    }
    if crate::config::DEBUG_VERBOSE {
        crate::kprintln!(
            "\x1b[90m[diag]\x1b[0m[execve] enter ring3 path={} rip={:#x} rsp={:#x}",
            path,
            user_frame.rip,
            user_frame.rsp,
        );
    }

    unsafe {
        task::arch::enter_user_mode_iret(&user_frame);
    }
}
