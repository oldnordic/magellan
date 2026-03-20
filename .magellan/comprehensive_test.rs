pub struct MyStruct {
    field1: i32,
    field2: String,
}

impl MyStruct {
    pub fn new() -> Self {
        Self {
            field1: 0,
            field2: String::new(),
        }
    }
    
    pub fn get_value(&self) -> i32 {
        self.field1
    }
}

pub enum MyEnum {
    Variant1,
    Variant2(i32),
}

pub trait MyTrait {
    fn do_something(&self);
}

impl MyTrait for MyStruct {
    fn do_something(&self) {
        println!("Doing something");
    }
}

pub fn calculate(x: i32, y: i32) -> i32 {
    let result = x + y;
    result
}

pub fn main() {
    let s = MyStruct::new();
    let val = s.get_value();
    let sum = calculate(val, 10);
    println!("Sum: {}", sum);
}
