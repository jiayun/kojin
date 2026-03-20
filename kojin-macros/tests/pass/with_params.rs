use kojin_macros::task;
use kojin_core::{TaskContext, TaskResult};

#[task]
async fn send_email(ctx: &TaskContext, to: String, subject: String) -> TaskResult<()> {
    let _ = (ctx, &to, &subject);
    Ok(())
}

fn main() {
    let _ = SendEmail::new("a@b.com".into(), "hello".into());
}
