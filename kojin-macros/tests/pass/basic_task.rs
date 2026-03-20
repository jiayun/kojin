use kojin_macros::task;
use kojin_core::{TaskContext, TaskResult};

#[task]
async fn my_task(ctx: &TaskContext) -> TaskResult<()> {
    let _ = ctx;
    Ok(())
}

fn main() {
    let _ = MyTask::new();
}
