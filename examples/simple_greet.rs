use embedded_config::prelude::*;

const NAME: &str = embed_config_value!("hello_world_example.name");
const OPT_CUSTOM_GREET: Option<&str> = embed_config_value_opt!("hello_world_example.custom_greet");

fn main() {
    println!("{}, {NAME}", OPT_CUSTOM_GREET.unwrap_or("Hello"));
}
