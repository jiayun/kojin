use kojin_macros::task;
use kojin_core::{TaskContext, TaskResult};

#[task(queue = "emails")]
async fn send_notification(ctx: &TaskContext) -> TaskResult<()> {
    let _ = ctx;
    Ok(())
}

fn main() {
    let _ = SendNotification::new();
}
