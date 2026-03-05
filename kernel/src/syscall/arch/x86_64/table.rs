//! x86_64 Linux ABI 系统调用表

use crate::syscall::SyscallDef;

pub const SYSCALL_TABLE: &[SyscallDef] = &[
    SyscallDef {
        nr: 0,
        name: "read",
    },
    SyscallDef {
        nr: 1,
        name: "write",
    },
    SyscallDef {
        nr: 2,
        name: "open",
    },
    SyscallDef {
        nr: 3,
        name: "close",
    },
    SyscallDef {
        nr: 4,
        name: "stat",
    },
    SyscallDef {
        nr: 5,
        name: "fstat",
    },
    SyscallDef {
        nr: 6,
        name: "lstat",
    },
    SyscallDef {
        nr: 7,
        name: "poll",
    },
    SyscallDef {
        nr: 8,
        name: "lseek",
    },
    SyscallDef {
        nr: 9,
        name: "mmap",
    },
    SyscallDef {
        nr: 10,
        name: "mprotect",
    },
    SyscallDef {
        nr: 11,
        name: "munmap",
    },
    SyscallDef {
        nr: 12,
        name: "brk",
    },
    SyscallDef {
        nr: 13,
        name: "rt_sigaction",
    },
    SyscallDef {
        nr: 14,
        name: "rt_sigprocmask",
    },
    SyscallDef {
        nr: 15,
        name: "rt_sigreturn",
    },
    SyscallDef {
        nr: 16,
        name: "ioctl",
    },
    SyscallDef {
        nr: 17,
        name: "pread64",
    },
    SyscallDef {
        nr: 18,
        name: "pwrite64",
    },
    SyscallDef {
        nr: 19,
        name: "readv",
    },
    SyscallDef {
        nr: 20,
        name: "writev",
    },
    SyscallDef {
        nr: 21,
        name: "access",
    },
    SyscallDef {
        nr: 22,
        name: "pipe",
    },
    SyscallDef {
        nr: 23,
        name: "select",
    },
    SyscallDef {
        nr: 24,
        name: "sched_yield",
    },
    SyscallDef {
        nr: 25,
        name: "mremap",
    },
    SyscallDef {
        nr: 26,
        name: "msync",
    },
    SyscallDef {
        nr: 27,
        name: "mincore",
    },
    SyscallDef {
        nr: 28,
        name: "madvise",
    },
    SyscallDef {
        nr: 29,
        name: "shmget",
    },
    SyscallDef {
        nr: 30,
        name: "shmat",
    },
    SyscallDef {
        nr: 31,
        name: "shmctl",
    },
    SyscallDef {
        nr: 32,
        name: "dup",
    },
    SyscallDef {
        nr: 33,
        name: "dup2",
    },
    SyscallDef {
        nr: 34,
        name: "pause",
    },
    SyscallDef {
        nr: 35,
        name: "nanosleep",
    },
    SyscallDef {
        nr: 36,
        name: "getitimer",
    },
    SyscallDef {
        nr: 37,
        name: "alarm",
    },
    SyscallDef {
        nr: 38,
        name: "setitimer",
    },
    SyscallDef {
        nr: 39,
        name: "getpid",
    },
    SyscallDef {
        nr: 40,
        name: "sendfile",
    },
    SyscallDef {
        nr: 41,
        name: "socket",
    },
    SyscallDef {
        nr: 42,
        name: "connect",
    },
    SyscallDef {
        nr: 43,
        name: "accept",
    },
    SyscallDef {
        nr: 44,
        name: "sendto",
    },
    SyscallDef {
        nr: 45,
        name: "recvfrom",
    },
    SyscallDef {
        nr: 46,
        name: "sendmsg",
    },
    SyscallDef {
        nr: 47,
        name: "recvmsg",
    },
    SyscallDef {
        nr: 48,
        name: "shutdown",
    },
    SyscallDef {
        nr: 49,
        name: "bind",
    },
    SyscallDef {
        nr: 50,
        name: "listen",
    },
    SyscallDef {
        nr: 51,
        name: "getsockname",
    },
    SyscallDef {
        nr: 52,
        name: "getpeername",
    },
    SyscallDef {
        nr: 53,
        name: "socketpair",
    },
    SyscallDef {
        nr: 54,
        name: "setsockopt",
    },
    SyscallDef {
        nr: 55,
        name: "getsockopt",
    },
    SyscallDef {
        nr: 56,
        name: "clone",
    },
    SyscallDef {
        nr: 57,
        name: "fork",
    },
    SyscallDef {
        nr: 58,
        name: "vfork",
    },
    SyscallDef {
        nr: 59,
        name: "execve",
    },
    SyscallDef {
        nr: 60,
        name: "exit",
    },
    SyscallDef {
        nr: 61,
        name: "wait4",
    },
    SyscallDef {
        nr: 62,
        name: "kill",
    },
    SyscallDef {
        nr: 63,
        name: "uname",
    },
    SyscallDef {
        nr: 64,
        name: "semget",
    },
    SyscallDef {
        nr: 65,
        name: "semop",
    },
    SyscallDef {
        nr: 66,
        name: "semctl",
    },
    SyscallDef {
        nr: 67,
        name: "shmdt",
    },
    SyscallDef {
        nr: 68,
        name: "msgget",
    },
    SyscallDef {
        nr: 69,
        name: "msgsnd",
    },
    SyscallDef {
        nr: 70,
        name: "msgrcv",
    },
    SyscallDef {
        nr: 71,
        name: "msgctl",
    },
    SyscallDef {
        nr: 72,
        name: "fcntl",
    },
    SyscallDef {
        nr: 73,
        name: "flock",
    },
    SyscallDef {
        nr: 74,
        name: "fsync",
    },
    SyscallDef {
        nr: 75,
        name: "fdatasync",
    },
    SyscallDef {
        nr: 76,
        name: "truncate",
    },
    SyscallDef {
        nr: 77,
        name: "ftruncate",
    },
    SyscallDef {
        nr: 78,
        name: "getdents",
    },
    SyscallDef {
        nr: 79,
        name: "getcwd",
    },
    SyscallDef {
        nr: 80,
        name: "chdir",
    },
    SyscallDef {
        nr: 81,
        name: "fchdir",
    },
    SyscallDef {
        nr: 82,
        name: "rename",
    },
    SyscallDef {
        nr: 83,
        name: "mkdir",
    },
    SyscallDef {
        nr: 84,
        name: "rmdir",
    },
    SyscallDef {
        nr: 85,
        name: "creat",
    },
    SyscallDef {
        nr: 86,
        name: "link",
    },
    SyscallDef {
        nr: 87,
        name: "unlink",
    },
    SyscallDef {
        nr: 88,
        name: "symlink",
    },
    SyscallDef {
        nr: 89,
        name: "readlink",
    },
    SyscallDef {
        nr: 90,
        name: "chmod",
    },
    SyscallDef {
        nr: 91,
        name: "fchmod",
    },
    SyscallDef {
        nr: 92,
        name: "chown",
    },
    SyscallDef {
        nr: 93,
        name: "fchown",
    },
    SyscallDef {
        nr: 94,
        name: "lchown",
    },
    SyscallDef {
        nr: 95,
        name: "umask",
    },
    SyscallDef {
        nr: 96,
        name: "gettimeofday",
    },
    SyscallDef {
        nr: 97,
        name: "getrlimit",
    },
    SyscallDef {
        nr: 98,
        name: "getrusage",
    },
    SyscallDef {
        nr: 99,
        name: "sysinfo",
    },
    SyscallDef {
        nr: 100,
        name: "times",
    },
    SyscallDef {
        nr: 101,
        name: "ptrace",
    },
    SyscallDef {
        nr: 102,
        name: "getuid",
    },
    SyscallDef {
        nr: 103,
        name: "syslog",
    },
    SyscallDef {
        nr: 104,
        name: "getgid",
    },
    SyscallDef {
        nr: 105,
        name: "setuid",
    },
    SyscallDef {
        nr: 106,
        name: "setgid",
    },
    SyscallDef {
        nr: 107,
        name: "geteuid",
    },
    SyscallDef {
        nr: 108,
        name: "getegid",
    },
    SyscallDef {
        nr: 109,
        name: "setpgid",
    },
    SyscallDef {
        nr: 110,
        name: "getppid",
    },
    SyscallDef {
        nr: 111,
        name: "getpgrp",
    },
    SyscallDef {
        nr: 112,
        name: "setsid",
    },
    SyscallDef {
        nr: 113,
        name: "setreuid",
    },
    SyscallDef {
        nr: 114,
        name: "setregid",
    },
    SyscallDef {
        nr: 115,
        name: "getgroups",
    },
    SyscallDef {
        nr: 116,
        name: "setgroups",
    },
    SyscallDef {
        nr: 117,
        name: "setresuid",
    },
    SyscallDef {
        nr: 118,
        name: "getresuid",
    },
    SyscallDef {
        nr: 119,
        name: "setresgid",
    },
    SyscallDef {
        nr: 120,
        name: "getresgid",
    },
    SyscallDef {
        nr: 121,
        name: "getpgid",
    },
    SyscallDef {
        nr: 122,
        name: "setfsuid",
    },
    SyscallDef {
        nr: 123,
        name: "setfsgid",
    },
    SyscallDef {
        nr: 124,
        name: "getsid",
    },
    SyscallDef {
        nr: 125,
        name: "capget",
    },
    SyscallDef {
        nr: 126,
        name: "capset",
    },
    SyscallDef {
        nr: 127,
        name: "rt_sigpending",
    },
    SyscallDef {
        nr: 128,
        name: "rt_sigtimedwait",
    },
    SyscallDef {
        nr: 129,
        name: "rt_sigqueueinfo",
    },
    SyscallDef {
        nr: 130,
        name: "rt_sigsuspend",
    },
    SyscallDef {
        nr: 131,
        name: "sigaltstack",
    },
    SyscallDef {
        nr: 132,
        name: "utime",
    },
    SyscallDef {
        nr: 133,
        name: "mknod",
    },
    SyscallDef {
        nr: 134,
        name: "uselib",
    },
    SyscallDef {
        nr: 135,
        name: "personality",
    },
    SyscallDef {
        nr: 136,
        name: "ustat",
    },
    SyscallDef {
        nr: 137,
        name: "statfs",
    },
    SyscallDef {
        nr: 138,
        name: "fstatfs",
    },
    SyscallDef {
        nr: 139,
        name: "sysfs",
    },
    SyscallDef {
        nr: 140,
        name: "getpriority",
    },
    SyscallDef {
        nr: 141,
        name: "setpriority",
    },
    SyscallDef {
        nr: 142,
        name: "sched_setparam",
    },
    SyscallDef {
        nr: 143,
        name: "sched_getparam",
    },
    SyscallDef {
        nr: 144,
        name: "sched_setscheduler",
    },
    SyscallDef {
        nr: 145,
        name: "sched_getscheduler",
    },
    SyscallDef {
        nr: 146,
        name: "sched_get_priority_max",
    },
    SyscallDef {
        nr: 147,
        name: "sched_get_priority_min",
    },
    SyscallDef {
        nr: 148,
        name: "sched_rr_get_interval",
    },
    SyscallDef {
        nr: 149,
        name: "mlock",
    },
    SyscallDef {
        nr: 150,
        name: "munlock",
    },
    SyscallDef {
        nr: 151,
        name: "mlockall",
    },
    SyscallDef {
        nr: 152,
        name: "munlockall",
    },
    SyscallDef {
        nr: 153,
        name: "vhangup",
    },
    SyscallDef {
        nr: 154,
        name: "modify_ldt",
    },
    SyscallDef {
        nr: 155,
        name: "pivot_root",
    },
    SyscallDef {
        nr: 156,
        name: "_sysctl",
    },
    SyscallDef {
        nr: 157,
        name: "prctl",
    },
    SyscallDef {
        nr: 158,
        name: "arch_prctl",
    },
    SyscallDef {
        nr: 159,
        name: "adjtimex",
    },
    SyscallDef {
        nr: 160,
        name: "setrlimit",
    },
    SyscallDef {
        nr: 161,
        name: "chroot",
    },
    SyscallDef {
        nr: 162,
        name: "sync",
    },
    SyscallDef {
        nr: 163,
        name: "acct",
    },
    SyscallDef {
        nr: 164,
        name: "settimeofday",
    },
    SyscallDef {
        nr: 165,
        name: "mount",
    },
    SyscallDef {
        nr: 166,
        name: "umount2",
    },
    SyscallDef {
        nr: 167,
        name: "swapon",
    },
    SyscallDef {
        nr: 168,
        name: "swapoff",
    },
    SyscallDef {
        nr: 169,
        name: "reboot",
    },
    SyscallDef {
        nr: 170,
        name: "sethostname",
    },
    SyscallDef {
        nr: 171,
        name: "setdomainname",
    },
    SyscallDef {
        nr: 172,
        name: "iopl",
    },
    SyscallDef {
        nr: 173,
        name: "ioperm",
    },
    SyscallDef {
        nr: 174,
        name: "create_module",
    },
    SyscallDef {
        nr: 175,
        name: "init_module",
    },
    SyscallDef {
        nr: 176,
        name: "delete_module",
    },
    SyscallDef {
        nr: 177,
        name: "get_kernel_syms",
    },
    SyscallDef {
        nr: 178,
        name: "query_module",
    },
    SyscallDef {
        nr: 179,
        name: "quotactl",
    },
    SyscallDef {
        nr: 180,
        name: "nfsservctl",
    },
    SyscallDef {
        nr: 181,
        name: "getpmsg",
    },
    SyscallDef {
        nr: 182,
        name: "putpmsg",
    },
    SyscallDef {
        nr: 183,
        name: "afs_syscall",
    },
    SyscallDef {
        nr: 184,
        name: "tuxcall",
    },
    SyscallDef {
        nr: 185,
        name: "security",
    },
    SyscallDef {
        nr: 186,
        name: "gettid",
    },
    SyscallDef {
        nr: 187,
        name: "readahead",
    },
    SyscallDef {
        nr: 188,
        name: "setxattr",
    },
    SyscallDef {
        nr: 189,
        name: "lsetxattr",
    },
    SyscallDef {
        nr: 190,
        name: "fsetxattr",
    },
    SyscallDef {
        nr: 191,
        name: "getxattr",
    },
    SyscallDef {
        nr: 192,
        name: "lgetxattr",
    },
    SyscallDef {
        nr: 193,
        name: "fgetxattr",
    },
    SyscallDef {
        nr: 194,
        name: "listxattr",
    },
    SyscallDef {
        nr: 195,
        name: "llistxattr",
    },
    SyscallDef {
        nr: 196,
        name: "flistxattr",
    },
    SyscallDef {
        nr: 197,
        name: "removexattr",
    },
    SyscallDef {
        nr: 198,
        name: "lremovexattr",
    },
    SyscallDef {
        nr: 199,
        name: "fremovexattr",
    },
    SyscallDef {
        nr: 200,
        name: "tkill",
    },
    SyscallDef {
        nr: 201,
        name: "time",
    },
    SyscallDef {
        nr: 202,
        name: "futex",
    },
    SyscallDef {
        nr: 203,
        name: "sched_setaffinity",
    },
    SyscallDef {
        nr: 204,
        name: "sched_getaffinity",
    },
    SyscallDef {
        nr: 205,
        name: "set_thread_area",
    },
    SyscallDef {
        nr: 206,
        name: "io_setup",
    },
    SyscallDef {
        nr: 207,
        name: "io_destroy",
    },
    SyscallDef {
        nr: 208,
        name: "io_getevents",
    },
    SyscallDef {
        nr: 209,
        name: "io_submit",
    },
    SyscallDef {
        nr: 210,
        name: "io_cancel",
    },
    SyscallDef {
        nr: 211,
        name: "get_thread_area",
    },
    SyscallDef {
        nr: 212,
        name: "lookup_dcookie",
    },
    SyscallDef {
        nr: 213,
        name: "epoll_create",
    },
    SyscallDef {
        nr: 214,
        name: "epoll_ctl_old",
    },
    SyscallDef {
        nr: 215,
        name: "epoll_wait_old",
    },
    SyscallDef {
        nr: 216,
        name: "remap_file_pages",
    },
    SyscallDef {
        nr: 217,
        name: "getdents64",
    },
    SyscallDef {
        nr: 218,
        name: "set_tid_address",
    },
    SyscallDef {
        nr: 219,
        name: "restart_syscall",
    },
    SyscallDef {
        nr: 220,
        name: "semtimedop",
    },
    SyscallDef {
        nr: 221,
        name: "fadvise64",
    },
    SyscallDef {
        nr: 222,
        name: "timer_create",
    },
    SyscallDef {
        nr: 223,
        name: "timer_settime",
    },
    SyscallDef {
        nr: 224,
        name: "timer_gettime",
    },
    SyscallDef {
        nr: 225,
        name: "timer_getoverrun",
    },
    SyscallDef {
        nr: 226,
        name: "timer_delete",
    },
    SyscallDef {
        nr: 227,
        name: "clock_settime",
    },
    SyscallDef {
        nr: 228,
        name: "clock_gettime",
    },
    SyscallDef {
        nr: 229,
        name: "clock_getres",
    },
    SyscallDef {
        nr: 230,
        name: "clock_nanosleep",
    },
    SyscallDef {
        nr: 231,
        name: "exit_group",
    },
    SyscallDef {
        nr: 232,
        name: "epoll_wait",
    },
    SyscallDef {
        nr: 233,
        name: "epoll_ctl",
    },
    SyscallDef {
        nr: 234,
        name: "tgkill",
    },
    SyscallDef {
        nr: 235,
        name: "utimes",
    },
    SyscallDef {
        nr: 236,
        name: "vserver",
    },
    SyscallDef {
        nr: 237,
        name: "mbind",
    },
    SyscallDef {
        nr: 238,
        name: "set_mempolicy",
    },
    SyscallDef {
        nr: 239,
        name: "get_mempolicy",
    },
    SyscallDef {
        nr: 240,
        name: "mq_open",
    },
    SyscallDef {
        nr: 241,
        name: "mq_unlink",
    },
    SyscallDef {
        nr: 242,
        name: "mq_timedsend",
    },
    SyscallDef {
        nr: 243,
        name: "mq_timedreceive",
    },
    SyscallDef {
        nr: 244,
        name: "mq_notify",
    },
    SyscallDef {
        nr: 245,
        name: "mq_getsetattr",
    },
    SyscallDef {
        nr: 246,
        name: "kexec_load",
    },
    SyscallDef {
        nr: 247,
        name: "waitid",
    },
    SyscallDef {
        nr: 248,
        name: "add_key",
    },
    SyscallDef {
        nr: 249,
        name: "request_key",
    },
    SyscallDef {
        nr: 250,
        name: "keyctl",
    },
    SyscallDef {
        nr: 251,
        name: "ioprio_set",
    },
    SyscallDef {
        nr: 252,
        name: "ioprio_get",
    },
    SyscallDef {
        nr: 253,
        name: "inotify_init",
    },
    SyscallDef {
        nr: 254,
        name: "inotify_add_watch",
    },
    SyscallDef {
        nr: 255,
        name: "inotify_rm_watch",
    },
    SyscallDef {
        nr: 256,
        name: "migrate_pages",
    },
    SyscallDef {
        nr: 257,
        name: "openat",
    },
    SyscallDef {
        nr: 258,
        name: "mkdirat",
    },
    SyscallDef {
        nr: 259,
        name: "mknodat",
    },
    SyscallDef {
        nr: 260,
        name: "fchownat",
    },
    SyscallDef {
        nr: 261,
        name: "futimesat",
    },
    SyscallDef {
        nr: 262,
        name: "newfstatat",
    },
    SyscallDef {
        nr: 263,
        name: "unlinkat",
    },
    SyscallDef {
        nr: 264,
        name: "renameat",
    },
    SyscallDef {
        nr: 265,
        name: "linkat",
    },
    SyscallDef {
        nr: 266,
        name: "symlinkat",
    },
    SyscallDef {
        nr: 267,
        name: "readlinkat",
    },
    SyscallDef {
        nr: 268,
        name: "fchmodat",
    },
    SyscallDef {
        nr: 269,
        name: "faccessat",
    },
    SyscallDef {
        nr: 270,
        name: "pselect6",
    },
    SyscallDef {
        nr: 271,
        name: "ppoll",
    },
    SyscallDef {
        nr: 272,
        name: "unshare",
    },
    SyscallDef {
        nr: 273,
        name: "set_robust_list",
    },
    SyscallDef {
        nr: 274,
        name: "get_robust_list",
    },
    SyscallDef {
        nr: 275,
        name: "splice",
    },
    SyscallDef {
        nr: 276,
        name: "tee",
    },
    SyscallDef {
        nr: 277,
        name: "sync_file_range",
    },
    SyscallDef {
        nr: 278,
        name: "vmsplice",
    },
    SyscallDef {
        nr: 279,
        name: "move_pages",
    },
    SyscallDef {
        nr: 280,
        name: "utimensat",
    },
    SyscallDef {
        nr: 281,
        name: "epoll_pwait",
    },
    SyscallDef {
        nr: 282,
        name: "signalfd",
    },
    SyscallDef {
        nr: 283,
        name: "timerfd_create",
    },
    SyscallDef {
        nr: 284,
        name: "eventfd",
    },
    SyscallDef {
        nr: 285,
        name: "fallocate",
    },
    SyscallDef {
        nr: 286,
        name: "timerfd_settime",
    },
    SyscallDef {
        nr: 287,
        name: "timerfd_gettime",
    },
    SyscallDef {
        nr: 288,
        name: "accept4",
    },
    SyscallDef {
        nr: 289,
        name: "signalfd4",
    },
    SyscallDef {
        nr: 290,
        name: "eventfd2",
    },
    SyscallDef {
        nr: 291,
        name: "epoll_create1",
    },
    SyscallDef {
        nr: 292,
        name: "dup3",
    },
    SyscallDef {
        nr: 293,
        name: "pipe2",
    },
    SyscallDef {
        nr: 294,
        name: "inotify_init1",
    },
    SyscallDef {
        nr: 295,
        name: "preadv",
    },
    SyscallDef {
        nr: 296,
        name: "pwritev",
    },
    SyscallDef {
        nr: 297,
        name: "rt_tgsigqueueinfo",
    },
    SyscallDef {
        nr: 298,
        name: "perf_event_open",
    },
    SyscallDef {
        nr: 299,
        name: "recvmmsg",
    },
    SyscallDef {
        nr: 300,
        name: "fanotify_init",
    },
    SyscallDef {
        nr: 301,
        name: "fanotify_mark",
    },
    SyscallDef {
        nr: 302,
        name: "prlimit64",
    },
    SyscallDef {
        nr: 303,
        name: "name_to_handle_at",
    },
    SyscallDef {
        nr: 304,
        name: "open_by_handle_at",
    },
    SyscallDef {
        nr: 305,
        name: "clock_adjtime",
    },
    SyscallDef {
        nr: 306,
        name: "syncfs",
    },
    SyscallDef {
        nr: 307,
        name: "sendmmsg",
    },
    SyscallDef {
        nr: 308,
        name: "setns",
    },
    SyscallDef {
        nr: 309,
        name: "getcpu",
    },
    SyscallDef {
        nr: 310,
        name: "process_vm_readv",
    },
    SyscallDef {
        nr: 311,
        name: "process_vm_writev",
    },
    SyscallDef {
        nr: 312,
        name: "kcmp",
    },
    SyscallDef {
        nr: 313,
        name: "finit_module",
    },
    SyscallDef {
        nr: 314,
        name: "sched_setattr",
    },
    SyscallDef {
        nr: 315,
        name: "sched_getattr",
    },
    SyscallDef {
        nr: 316,
        name: "renameat2",
    },
    SyscallDef {
        nr: 317,
        name: "seccomp",
    },
    SyscallDef {
        nr: 318,
        name: "getrandom",
    },
    SyscallDef {
        nr: 319,
        name: "memfd_create",
    },
    SyscallDef {
        nr: 320,
        name: "kexec_file_load",
    },
    SyscallDef {
        nr: 321,
        name: "bpf",
    },
    SyscallDef {
        nr: 322,
        name: "execveat",
    },
    SyscallDef {
        nr: 323,
        name: "userfaultfd",
    },
    SyscallDef {
        nr: 324,
        name: "membarrier",
    },
    SyscallDef {
        nr: 325,
        name: "mlock2",
    },
    SyscallDef {
        nr: 326,
        name: "copy_file_range",
    },
    SyscallDef {
        nr: 327,
        name: "preadv2",
    },
    SyscallDef {
        nr: 328,
        name: "pwritev2",
    },
    SyscallDef {
        nr: 329,
        name: "pkey_mprotect",
    },
    SyscallDef {
        nr: 330,
        name: "pkey_alloc",
    },
    SyscallDef {
        nr: 331,
        name: "pkey_free",
    },
    SyscallDef {
        nr: 332,
        name: "statx",
    },
    SyscallDef {
        nr: 333,
        name: "io_pgetevents",
    },
    SyscallDef {
        nr: 334,
        name: "rseq",
    },
    SyscallDef {
        nr: 424,
        name: "pidfd_send_signal",
    },
    SyscallDef {
        nr: 425,
        name: "io_uring_setup",
    },
    SyscallDef {
        nr: 426,
        name: "io_uring_enter",
    },
    SyscallDef {
        nr: 427,
        name: "io_uring_register",
    },
    SyscallDef {
        nr: 428,
        name: "open_tree",
    },
    SyscallDef {
        nr: 429,
        name: "move_mount",
    },
    SyscallDef {
        nr: 430,
        name: "fsopen",
    },
    SyscallDef {
        nr: 431,
        name: "fsconfig",
    },
    SyscallDef {
        nr: 432,
        name: "fsmount",
    },
    SyscallDef {
        nr: 433,
        name: "fspick",
    },
    SyscallDef {
        nr: 434,
        name: "pidfd_open",
    },
    SyscallDef {
        nr: 435,
        name: "clone3",
    },
    SyscallDef {
        nr: 436,
        name: "close_range",
    },
    SyscallDef {
        nr: 437,
        name: "openat2",
    },
    SyscallDef {
        nr: 438,
        name: "pidfd_getfd",
    },
    SyscallDef {
        nr: 439,
        name: "faccessat2",
    },
    SyscallDef {
        nr: 440,
        name: "process_madvise",
    },
    SyscallDef {
        nr: 441,
        name: "epoll_pwait2",
    },
    SyscallDef {
        nr: 442,
        name: "mount_setattr",
    },
    SyscallDef {
        nr: 443,
        name: "quotactl_fd",
    },
    SyscallDef {
        nr: 444,
        name: "landlock_create_ruleset",
    },
    SyscallDef {
        nr: 445,
        name: "landlock_add_rule",
    },
    SyscallDef {
        nr: 446,
        name: "landlock_restrict_self",
    },
    SyscallDef {
        nr: 447,
        name: "memfd_secret",
    },
    SyscallDef {
        nr: 448,
        name: "process_mrelease",
    },
    SyscallDef {
        nr: 449,
        name: "futex_waitv",
    },
    SyscallDef {
        nr: 450,
        name: "set_mempolicy_home_node",
    },
    SyscallDef {
        nr: 451,
        name: "cachestat",
    },
    SyscallDef {
        nr: 452,
        name: "fchmodat2",
    },
    SyscallDef {
        nr: 453,
        name: "map_shadow_stack",
    },
    SyscallDef {
        nr: 454,
        name: "futex_wake",
    },
    SyscallDef {
        nr: 455,
        name: "futex_wait",
    },
    SyscallDef {
        nr: 456,
        name: "futex_requeue",
    },
    SyscallDef {
        nr: 457,
        name: "statmount",
    },
    SyscallDef {
        nr: 458,
        name: "listmount",
    },
    SyscallDef {
        nr: 459,
        name: "lsm_get_self_attr",
    },
    SyscallDef {
        nr: 460,
        name: "lsm_set_self_attr",
    },
    SyscallDef {
        nr: 461,
        name: "lsm_list_modules",
    },
];
