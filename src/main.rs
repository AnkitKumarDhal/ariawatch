fn print_version(name: &str, version: &str) {
    println!("{name} v{version}");

    let stage = if version == "0.0.1" {
        "bootstrap"
    } else if version == "0.0.2" {
        "foundation"
    } else {
        "future release"
    };

    println!("stage: {stage}");
}

fn main() {
    let name = "ariawatch";
    let mut version = "0.0.1";

    print_version(name, version);

    version = "0.0.2";

    print_version(name, version);

    version = "0.0.3";

    print_version(name, version);
}
