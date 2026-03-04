#!/usr/bin/env sh
set -eu

VM_CFG_PATH="${VM_CFG_PATH:-vm_cfg.toml}"

if [ ! -f "$VM_CFG_PATH" ]; then
    echo "Error: vm config file not found: $VM_CFG_PATH" >&2
    exit 1
fi

usage() {
    cat >&2 <<'EOF'
Usage: vmcfg <command> [args]

Commands:
  get <key>   Get VM config value (e.g., qemu.memory)
  show        Show VM config values

Environment:
  VM_CFG_PATH Path to vm_cfg.toml (default: vm_cfg.toml)
EOF
}

default_value() {
    case "$1" in
        qemu.memory) echo "1G" ;;
        qemu.smp) echo "4" ;;
        qemu.cpu) echo "host" ;;
        qemu.kvm) echo "auto" ;;
        qemu.machine) echo "q35" ;;
        qemu.iommu) echo "true" ;;
        qemu.numa_args) echo "" ;;
        qemu.extra_args) echo "" ;;
        vmware.memory) echo "1G" ;;
        vmware.smp) echo "4" ;;
        vmware.cpu) echo "host" ;;
        vmware.firmware) echo "uefi" ;;
        *) return 1 ;;
    esac
}

read_toml_value() {
    section="$1"
    key="$2"
    awk -v sec="$section" -v key="$key" '
BEGIN { in_sec = 0 }
/^[[:space:]]*\[/ {
    in_sec = ($0 ~ ("^[[:space:]]*\\[" sec "\\][[:space:]]*$"))
    next
}
in_sec && $0 ~ ("^[[:space:]]*" key "[[:space:]]*=") {
    eq = index($0, "=")
    if (eq == 0) {
        next
    }
    val = substr($0, eq + 1)
    sub(/[[:space:]]*#.*$/, "", val)
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", val)
    gsub(/^"/, "", val)
    gsub(/"$/, "", val)
    print val
    exit
}
' "$VM_CFG_PATH"
}

get_value() {
    full_key="$1"
    case "$full_key" in
        qemu.*|vmware.*) ;;
        *)
            echo "Error: unknown key '$full_key'" >&2
            exit 1
            ;;
    esac

    section="${full_key%%.*}"
    key="${full_key#*.}"
    val="$(read_toml_value "$section" "$key")"
    if [ -n "$val" ]; then
        echo "$val"
        return 0
    fi
    if default_value "$full_key" >/dev/null 2>&1; then
        default_value "$full_key"
        return 0
    fi
    echo "Error: unknown key '$full_key'" >&2
    exit 1
}

show_all() {
    echo "[qemu]"
    echo "  memory = $(get_value qemu.memory)"
    echo "  smp = $(get_value qemu.smp)"
    echo "  cpu = $(get_value qemu.cpu)"
    echo "  kvm = $(get_value qemu.kvm)"
    echo "  machine = $(get_value qemu.machine)"
    echo "  iommu = $(get_value qemu.iommu)"
    echo "  numa_args = $(get_value qemu.numa_args)"
    echo "  extra_args = $(get_value qemu.extra_args)"
    echo
    echo "[vmware]"
    echo "  memory = $(get_value vmware.memory)"
    echo "  smp = $(get_value vmware.smp)"
    echo "  cpu = $(get_value vmware.cpu)"
    echo "  firmware = $(get_value vmware.firmware)"
}

if [ $# -lt 1 ]; then
    usage
    exit 1
fi

cmd="$1"
case "$cmd" in
    get)
        if [ $# -ne 2 ]; then
            echo "Error: missing key argument" >&2
            exit 1
        fi
        get_value "$2"
        ;;
    show)
        show_all
        ;;
    *)
        echo "Error: unknown command '$cmd'" >&2
        usage
        exit 1
        ;;
esac
