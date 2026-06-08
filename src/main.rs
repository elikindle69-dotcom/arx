fn main() {
    if let Err(error) = arx::cli::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
