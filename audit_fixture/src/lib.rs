pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn compute(x: i32) -> i32 {
    let temp = step1(x);
    step2(temp)
}

fn step1(x: i32) -> i32 {
    x * 2
}

fn step2(x: i32) -> i32 {
    x + 1
}

pub fn helper() {
    nested();
}

fn nested() {
    println!("done");
}

// Mutual recursion
pub fn is_even(n: u32) -> bool {
    if n == 0 { true } else { is_odd(n - 1) }
}

fn is_odd(n: u32) -> bool {
    if n == 0 { false } else { is_even(n - 1) }
}
