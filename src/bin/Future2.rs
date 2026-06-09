use std::pin::Pin;
use std::task::{Context, Poll};

struct Ready42;

impl Future for Ready42{
    type Output = i32;
    fn  poll(self:Pin<&mut Self>,_cx:&mut Context<'_>)->Poll<i32>{
        Poll::Ready(42)
    }
}

#[tokio::main]
async fn main() {
    let future = Ready42;
    let result = future.await;
    println!("Hello, world! {}",result);
}
