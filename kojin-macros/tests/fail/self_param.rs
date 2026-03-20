use kojin_macros::task;

struct Foo;

impl Foo {
    #[task]
    async fn my_task(&self) {}
}

fn main() {}
