# Embedded config

A simple crate to read a toml files with values you wish to embed into your
application. The main use-case is for embedded projects, but can be used outside
of this realm too.

## Usage

### Setting it up

Create a toml file. The file can be pretty free-form and doesn't require a 
certain structure.

For the macro to find the config file, you can either set the environment
variable called `EMBEDDED_CONFIG_PATH` or set metadata in your Cargo.toml:

```toml
[package]
...

[package.metadata.embedded-config]
path = "embedded_config.toml"

...
```

### Embedding values

In your rust project, after a `use embedded_config::prelude::*;` the macro
`embed_config_value!(...)` can be used to embed a value out of your toml file.

The macro takes a simple string argument, denoting the field path in your toml.

Assuming the following toml is your custom config:

```toml
[wifi]
ssid = "foobar"

[display]
refresh_every_secs = 10
```

You could then embed those values like so:

```rust
const WIFI_SSID: &str = embed_config_value!("wifi.ssid");
```

Or directly in the code:

```rust
...
loop {
    ...
    display.update(...).await;
    Timer::after_secs(embed_config_value!("display.refresh_every_secs")).await;
}
...
```