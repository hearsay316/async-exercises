use std::pin::Pin;
use std::task::{Context, Poll};

struct CountdownFuture{
    count:u32,
}

impl CountdownFuture {
    fn new(count:u32)->Self{
        CountdownFuture{
            count,
        }
    }
}
impl Future for CountdownFuture {
    type Output = &'static str;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {

        if self.count ==0{
            Poll::Ready("完成")
        }else {
            self.count-=1;
            println!("等待中...{}",self.count);
            cx.waker().wake_by_ref();
            Poll::Pending
        }

    }
}
// tokio::main 会创建 Tokio 异步运行时，让 main 函数里可以使用 .await
#[tokio::main]
async fn main() {
    println!("开始等待...");
    // 这里的 20 表示等待 20 秒；from_secs 会把数字转换成 Duration 时间长度
    // Delay::new(Duration::from_secs(20)).await;

    let future = CountdownFuture::new(10).await;
    println!("等待结束！{}",future);
}
