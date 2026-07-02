// Sample Rust file for syntax highlighting demo

use std::collections::HashMap;

fn main() {
    let x = 42;
    let s = "hello, world!";
    let result = add(x, 21);

    println!("{} + {} = {}", x, 21, result);

    // This is a comment
    let map: HashMap<String, i32> = HashMap::new();
}

fn add(a: i32, b: i32) -> i32 {
    a + b /* inline comment */
}

pub struct Point {
    x: f64,
    y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(&self) -> f64 {
        (self.x.powi(2) + self.y.powi(2)).sqrt()
    }
}
