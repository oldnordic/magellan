fn main() {
    helper_a();
    helper_b();
}

fn helper_a() {
    nested_helper();
}

fn helper_b() {
    another_nested();
}

fn nested_helper() {
    println!("nested");
}

fn another_nested() {
    deep_call();
}

fn deep_call() {
    println!("deep");
}

// Mutual recursion
fn mutual_a() {
    mutual_b();
}

fn mutual_b() {
    mutual_a();
}
