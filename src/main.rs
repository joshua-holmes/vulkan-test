use std::fs;

const EXAMPLES_DIR: &str = "./examples";

fn main() {
    let example_options = fs::read_dir(EXAMPLES_DIR)
        .expect(format!("cannot find directory to read from: {}", EXAMPLES_DIR).as_str())
        .filter_map(|dir| dir.map(|d| d.file_name().into_string().unwrap()).ok());
    println!("To run examples, run:");
    println!("\tcargo run --example <example>\n");
    println!("where <example> can be one of these options:");
    for e in example_options {
        println!("\t{}", e);
    }
}
