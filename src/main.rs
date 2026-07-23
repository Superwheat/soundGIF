use std::process;

fn main() {
    if let Err(error) = soundgif::run_cli() {
        eprintln!("soundgif: {error}");
        process::exit(1);
    }
}
