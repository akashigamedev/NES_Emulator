use std::io::{self, Read};

mod cpu;
mod opcodes;
fn main() {
    let mut counter = 0;
    let mut buffer: [u8; 100] = [0; 100];
    loop {
        io::stdin().read(&mut buffer).unwrap();
        counter += 1;
        println!("counter: {}", counter);
    }
}
