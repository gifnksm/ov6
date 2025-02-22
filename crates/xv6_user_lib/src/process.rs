pub fn exit(code: i32) -> ! {
    xv6_user_syscall::exit(code);
}
