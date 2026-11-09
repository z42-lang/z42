pub(crate) fn current_token() -> usize {
    thread_local! { static TOKEN: u8 = const { 0 }; }
    TOKEN.with(|t| t as *const u8 as usize)
}
