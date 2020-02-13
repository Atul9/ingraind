#![no_std]
#![no_main]

use cty::*;

use redbpf_probes::bindings::*;
use redbpf_probes::helpers::*;
use redbpf_probes::kprobe::Registers;
use redbpf_probes::maps::*;
use redbpf_macros::{map, program, kprobe};
use ingraind_probes::syscalls::SyscallTracepoint;

program!(0xFFFFFFFE, "GPL");

#[map("syscall_tp_trigger")]
static mut syscall_event: PerfMap<SyscallTracepoint> = PerfMap::with_max_entries(1024);

#[map("host_pid")]
static mut host_pid: HashMap<u8, u64> = HashMap::with_max_entries(1024);

#[kprobe("__x64_sys_clone")]
pub fn syscall_enter(regs: Registers) -> i32 {
  let ignore_pid = unsafe { host_pid.get(1u8) };
  let pid_tgid = bpf_get_current_pid_tgid();
  if let Some(pid) = ignore_pid {
    if *pid == pid_tgid >> 32 {
      return 0;
    }
  }
  #[cfg(target_arch = "x86_64")]
  let syscall_nr = unsafe { (*(regs.ctx as *const pt_regs)).ax };
  #[cfg(target_arch = "aarch64")]
  let syscall_nr = unsafe { (*(regs.ctx as *const user_pt_regs)).regs[1] };

  let data = SyscallTracepoint {
    id: pid_tgid >> 32,
    syscall_nr,
    comm: bpf_get_current_comm(),
  };
  unsafe { syscall_event.insert(regs.ctx, data) };

  return 0;
}
