fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(texgo_desktop::automation::cli(&args));
}
