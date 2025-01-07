use embedded_config::prelude::*;

const NAME: &str = embed_config_value!("hello_world_example.name");

fn main() {
    println!("Hello, {NAME}");
}
