.global syscall_break
.global syscall_debug_output
.global syscall_create_port
.global syscall_connect_to_port
.global syscall_exit_process
.global syscall_close_handle
.global syscall_ipc_request
.global syscall_ipc_reply
.global syscall_ipc_receive
.global syscall_ipc_accept
.global syscall_get_process_id
.global syscall_map_memory
.global syscall_sleep_ns
.global syscall_bodge
.global syscall_get_thread_id
.global syscall_create_thread
.global syscall_futex_wait
.global syscall_futex_wake
.global syscall_map_device_memory
.global syscall_get_system_tick

.section .text

// sysv abi register order: rdi, rsi, rdx, rcx, r8, r9
// sysv abi callee save: rbx, rsp, rbp, r12, r13, r14, and r15;
// rcx is clobbered by syscall, so move it into r10

// The sysv abi returns 128 bit values in rax:rdx.

syscall_break:
mov eax, 0x00
syscall
ret

syscall_debug_output:
mov eax, 0x01
syscall
ret

syscall_create_port:
push rbx
mov eax, 0x02
mov rbx, rsi
syscall
mov [rbx], edx
pop rbx
ret

syscall_connect_to_named_port:
push rbx
mov eax, 0x03
mov rbx, rsi
syscall
mov [rbx], edx
pop rbx
ret

syscall_exit_process:
mov eax, 0x04
syscall
ret

syscall_close_handle:
mov eax, 0x05
syscall
ret

syscall_ipc_request:
mov eax, 0x06
syscall
ret

syscall_ipc_reply:
mov eax, 0x07
syscall
ret

syscall_ipc_receive:
push rbx
mov eax, 0x08
mov rbx, rcx
syscall
mov [rbx], rdx
pop rbx
ret

syscall_ipc_accept:
push rbx
mov eax, 0x09
mov rbx, rsi
syscall
mov [rbx], edx
pop rbx
ret

syscall_get_process_id:
mov eax, 0x0a
syscall
ret

syscall_connect_to_port_handle:
push rbx
mov eax, 0x0b
mov rbx, rsi
syscall
mov [rbx], edx
pop rbx
ret

syscall_map_memory:
push rbx
mov eax, 0x0c
mov rbx, rcx
syscall
mov [rbx], rdx
pop rbx
ret

syscall_sleep_ns:
mov eax, 0x0d
syscall
ret

syscall_bodge:
mov eax, 0x0e
syscall
ret

syscall_get_thread_id:
mov eax, 0x0f
syscall
ret

syscall_create_thread:
push rbx
mov eax, 0x10
mov rbx, rdx
syscall
mov [rbx], rdx
pop rbx
ret

syscall_futex_wait:
mov eax, 0x11
syscall
ret

syscall_futex_wake:
mov eax, 0x12
syscall
ret

syscall_map_device_memory:
push rbx
mov eax, 0x13
mov rbx, r8
syscall
mov [rbx], rdx
pop rbx
ret

syscall_get_system_tick:
mov eax, 0x15
syscall
ret