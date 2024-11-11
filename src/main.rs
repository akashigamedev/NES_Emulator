use std::io::{self, Read};
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;
pub mod cpu;
pub mod opcodes;
fn main() {
    let mut counter = 0;
    let mut buffer: [u8; 100] = [0; 100];
    loop {
        io::stdin().read(&mut buffer).unwrap();
        counter += 1;
        println!("counter: {}", counter);
    }
}
