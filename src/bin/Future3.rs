// 小白提示：这段代码演示【Waker合约】。先看类型/函数签名，再看 .await、poll、spawn 等关键调用怎样推动异步任务。
// 引入 Future trait，用来手动实现自定义 Future
use std::future::Future;
// Pin 用来保证 Future 在被 poll 时不会被随意移动
use std::pin::Pin;
// Arc 负责跨线程共享所有权，Mutex 负责安全地修改共享状态
use std::sync::{Arc, Mutex};
// 引入任务轮询相关类型：Context 提供 Waker，Poll 表示完成或等待
use std::task::{Context, Poll, Waker};
// 用标准库线程模拟后台异步任务
use std::thread;
// 表示延迟等待的时间长度
use std::time::Duration;

/// 延迟后完成的Future（玩具实现）
struct Delay {
    // 标记延迟任务是否已经完成，后台线程会在计时结束后把它设为 true
    completed: Arc<Mutex<bool>>,
    // 保存执行器传入的 Waker，后台线程完成后用它唤醒任务再次被 poll
    waker_stored: Arc<Mutex<Option<Waker>>>,
    // 需要等待的时间长度
    duration: Duration,
    // 标记后台计时器线程是否已经启动，避免重复创建线程
    started: bool,
}

impl Delay {
    // 创建一个新的 Delay Future，初始状态为未完成、未启动
    fn new(duration: Duration) -> Self {
        Delay {
            // 用 Arc<Mutex<_>> 让主 Future 和后台线程共享完成状态
            completed: Arc::new(Mutex::new(false)),
            // 一开始还没有 Waker，等第一次 poll 时再保存执行器传入的 Waker
            waker_stored: Arc::new(Mutex::new(None)),
            // 保存调用者指定的延迟时长
            duration,
            // 后台线程尚未启动
            started: false,
        }
    }
}

impl Future for Delay {
    type Output = ();

    // 执行器会反复调用 poll 来推动 Future 前进
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // 如果后台线程已经把 completed 设为 true，说明 Future 已完成
        if *self.completed.lock().unwrap() {
            return Poll::Ready(());
        }

        // 保存当前任务的 Waker，方便后台线程完成后通知执行器再次 poll
        *self.waker_stored.lock().unwrap() = Some(cx.waker().clone());

        // 第一次被 poll 时启动一个后台线程来模拟异步等待
        if !self.started {
            self.started = true;
            // 克隆共享状态，移动到后台线程中使用
            let completed = Arc::clone(&self.completed);
            let waker = Arc::clone(&self.waker_stored);
            let duration = self.duration;

            thread::spawn(move || {
                // 阻塞后台线程，模拟耗时操作；不会阻塞异步执行器线程
                thread::sleep(duration);
                // 标记 Future 已完成
                *completed.lock().unwrap() = true;

                // 关键：唤醒执行器，让它再次轮询我们
                if let Some(w) = waker.lock().unwrap().take() {
                    w.wake(); // “嘿执行器，我准备好了——再次轮询我!”
                }
            });
        }

        // 再检查一次完成状态，处理保存 Waker 和启动线程之间可能出现的竞争情况
        if *self.completed.lock().unwrap() {
            return Poll::Ready(());
        }

        Poll::Pending // 还没有完成，告诉执行器稍后再来 poll
    }
}

// tokio::main 会创建 Tokio 异步运行时，让 main 函数里可以使用 .await
#[tokio::main]
async fn main() {
    println!("开始等待...");
    // 这里的 20 表示等待 20 秒；from_secs 会把数字转换成 Duration 时间长度
    Delay::new(Duration::from_secs(20)).await;


    println!("等待结束！");
}
