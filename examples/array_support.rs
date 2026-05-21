use embedded_config::prelude::*;

const ARRAY: &[&str] = &embed_config_value!("test.array");
const IP: [u8; 4] = embed_config_value!("network.ip" as [u8; 4]);
const PORT: u16 = embed_config_value!("network.port" as u16);

fn main() {
    println!("Array: {ARRAY:?}");
    println!("IP: {IP:?}");
    println!("Port: {PORT}");
}
