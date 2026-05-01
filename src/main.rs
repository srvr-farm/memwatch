fn main() {
    if let Err(error) = memwatch::run() {
        eprintln!("{error:?}");
        std::process::exit(1);
    }
}
