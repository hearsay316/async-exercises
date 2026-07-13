// 小白提示：这是“虚假唤醒安全”练习的答案。重点看 poll 每次都重新检查 flag，而不是被唤醒后就假设一定完成。
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Waker};

struct FlagFuture {
    flag: Arc<AtomicBool>,
    waker_slot: Arc<Mutex<Option<Waker>>>,
}

impl FlagFuture {
    fn new(flag: Arc<AtomicBool>, waker_slot: Arc<Mutex<Option<Waker>>>) -> Self {
        FlagFuture { flag, waker_slot }
    }
}

impl Future for FlagFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // 始终重新检查实际情况——永远不要只相信唤醒
        if self.flag.load(Ordering::Acquire) {
            return Poll::Ready(());
        }

        // 存储/更新Waker以便我们收到通知
        let mut slot = self.waker_slot.lock().unwrap();
        *slot = Some(cx.waker().clone());

        // 存放Waker后重新检查以避免竞争：
        // 该标志可能已在我们第一次检查之间设置
        // 并存放Waker
        if self.flag.load(Ordering::Acquire) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

// 设置方（例如另一个线程或任务）：
fn set_flag(flag: &AtomicBool, waker_slot: &Mutex<Option<Waker>>) {
    flag.store(true, Ordering::Release);
    if let Some(waker) = waker_slot.lock().unwrap().take() {
        waker.wake();
    }
}

// 等价于使用 poll_fn：
// async fn wait_for_flag(flag: Arc<AtomicBool>, waker_slot: Arc<Mutex<Option<Waker>>>) {
//     std::future::poll_fn(|cx| {
//         if flag.load(Ordering::Acquire) {
//             return Poll::Ready(());
//         }
//         *waker_slot.lock().unwrap() = Some(cx.waker().clone());
//         if flag.load(Ordering::Acquire) { Poll::Ready(()) } else { Poll::Pending }
//     }).await
// }

#[tokio::main]
async fn main() {
    let flag = Arc::new(AtomicBool::new(false));
    let waker_slot = Arc::new(Mutex::new(None));

    let flag2 = Arc::clone(&flag);
    let waker_slot2 = Arc::clone(&waker_slot);
    println!("设置标志1");
  let td =   std::thread::spawn(move || {
        println!("设置标志2");
        std::thread::sleep(std::time::Duration::from_secs(10));
        set_flag(&flag2, &waker_slot2);
        println!("设置标志3");
    });
    println!("设置标志4");
    FlagFuture::new(flag, waker_slot).await;
    td.join().unwrap();
    println!("FlagFuture 执行完成");
}