# initramfs Template

This directory is the project initramfs template.

`make build` will package this tree into `target/initramfs.cpio` and place it at:
`EFI/january_os/initramfs.cpio`.

Notes:
- Linux-like top-level directories are pre-created (`/bin`, `/sbin`, `/etc`, `/usr`, `/proc`, `/sys`, `/dev`, `/tmp`, `/var`, `/mnt`).
- `.gitkeep` files are only for repository tracking and are skipped during packaging.
- `/bin/init`, `/bin/sh` and coreutils (`ls`, `cat`, `pwd`, `echo`) are built from `userland/` and copied in by `make build`.
- Current `/bin/init` is a minimal PID 1 user-space shell used until `fork/vfork + execve` can reliably support a supervising init process.
- `tests/` files are consumed by kernel test suites.
- `tests/task/test_user.elf` is tracked in this template and packaged as-is.
