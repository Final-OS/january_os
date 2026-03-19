use super::*;

fn restore_previous_exec_state(
    previous_mappings: Vec<task::ExecMappedPage>,
    vma_restore: Option<task::ExecVmaRestorePoint>,
) -> Result<(), i32> {
    if let Some(vma_restore) = vma_restore {
        task::restore_current_exec_vmas(vma_restore);
    }

    task::remap_exec_mappings(&previous_mappings)?;
    if task::set_current_exec_mappings(previous_mappings).is_none() {
        return Err(ESRCH);
    }
    Ok(())
}

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
    let previous_mappings = match task::take_current_exec_mappings() {
        Some(mappings) => mappings,
        None => return err(ESRCH),
    };
    task::unmap_exec_mappings(&previous_mappings);

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
            if let Err(restore_errno) = restore_previous_exec_state(previous_mappings, None) {
                return err(restore_errno);
            }
            return err(errno);
        }
    };

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
    let vma_restore = match task::install_current_exec_vmas(&load_plan) {
        Ok(restore) => restore,
        Err(errno) => {
            task::rollback_exec_mappings(&staged_mappings);
            if let Err(restore_errno) = restore_previous_exec_state(previous_mappings, None) {
                return err(restore_errno);
            }
            return err(errno);
        }
    };
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
            if let Err(restore_errno) = restore_previous_exec_state(previous_mappings, Some(vma_restore))
            {
                return err(restore_errno);
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
    let auxv = task::proc::exec::minimal_auxv(&load_plan);

    let user_rsp = match task::setup_initial_user_stack(
        load_plan.stack_top,
        load_plan.stack_pages,
        argv_refs.as_slice(),
        envp_refs.as_slice(),
        auxv.as_slice(),
    ) {
        Ok(rsp) => rsp,
        Err(errno) => {
            let installed_mappings = task::take_current_exec_mappings().unwrap_or_default();
            task::rollback_exec_mappings(&installed_mappings);
            if let Err(restore_errno) =
                restore_previous_exec_state(previous_mappings, Some(vma_restore))
            {
                return err(restore_errno);
            }
            return err(errno);
        }
    };

    if task::record_current_exec_request(path.as_str(), argv.len(), envp.len()).is_none() {
        let installed_mappings = task::take_current_exec_mappings().unwrap_or_default();
        task::rollback_exec_mappings(&installed_mappings);
        if let Err(restore_errno) = restore_previous_exec_state(previous_mappings, Some(vma_restore))
        {
            return err(restore_errno);
        }
        return err(ESRCH);
    }
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
