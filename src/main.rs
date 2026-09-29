fn print_version(name: &str, version: &str) {
    println!("{name} v{version}");
}

fn main() {
    let name = "ariawatch";
    let mut version = "0.0.1";

    print_version(name, version);

    version = "0.0.2";
    print_version(name, version);
}
