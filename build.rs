fn main() {
    #[cfg(target_os = "windows")]
    compile_error!("This project is not supported on Windows.");
}
