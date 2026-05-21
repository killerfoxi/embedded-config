use embedded_config::prelude::embed_config_value;

fn main() {
    const ARRAY: &[&str] = &embed_config_value!("test.array");
    println!("Array: {:?}", ARRAY);
}
