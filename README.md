# ESP32 Rust Hello World

This is a bare-metal Rust program for the original ESP32, using Espressif's
`esp-hal`. It writes `Hello, world!` over the serial connection.

## Set up

Install the Rust ESP toolchain and export its environment with
[espup](https://github.com/esp-rs/espup). Install `espflash` as well. The
[Rust on ESP book](https://docs.espressif.com/projects/rust/book/) has setup
instructions.

## Build and run

```sh
cargo build
cargo run
```

`cargo run` flashes the firmware and opens the serial monitor. Use
`cargo build --release` for an optimized firmware build.
