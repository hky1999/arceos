#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

#[macro_use]
#[cfg(feature = "axstd")]
extern crate axstd as std;

use std::thread;
use std::time::{Duration, Instant};


#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    println!("Hello, main task!");

    // let now = Instant::now();

    // let elapsed = now.elapsed();
    // println!("main task sleep for {:?}", elapsed);

    loop {
        thread::sleep(Duration::from_secs(1));
        println!("main task tick()");
    }
}
