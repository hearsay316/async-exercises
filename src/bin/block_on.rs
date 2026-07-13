use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

/// 阻塞当前线程，直到传入的 Future 执行完成。
///
/// API 用法：
/// ```ignore
/// let result = block_on(async {
///     42
/// });
/// ```
///
/// 参数说明：
/// - `F`: 泛型参数，表示任意实现了 `Future` trait 的类型。
/// - `future`: 要执行的异步任务，可以是 `async` 块、`async fn` 的返回值，或者手写的 Future。
///
/// 返回值：
/// - `F::Output`: Future 完成后产出的值，也就是 `Poll::Ready(value)` 里的 `value`。
fn block_on<F: Future>(mut future: F) -> F::Output {
    // Pin::new_unchecked(pointer)
    // - `pointer`: 要固定住的指针，这里是 `&mut future`。
    // 返回值是 `Pin<&mut F>`，表示这个 Future 在 poll 期间不会再被移动。
    let mut future = unsafe { Pin::new_unchecked(&mut future) };

    // 构造一个什么都不做的 RawWaker，用来创建 Context。
    fn noop_raw_waker() -> RawWaker {
        // RawWaker 回调函数的参数：
        // - `data: *const ()`: RawWaker::new 传进来的任务数据指针。
        // 真实执行器通常会把任务指针、Arc<Task> 之类的数据放在这里。
        // 这里没有真实任务数据，所以忽略这个参数。
        fn no_op(_: *const ()) {}

        // clone(data)
        // - `data`: RawWaker::new 传进来的任务数据指针。
        // 返回值是新的 RawWaker，用于支持 Waker 的 clone 操作。
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }

        // RawWakerVTable::new(clone, wake, wake_by_ref, drop)
        // - `clone`: 复制 Waker 时调用，签名是 `unsafe fn(*const ()) -> RawWaker`。
        // - `wake`: 消费 Waker 并唤醒任务时调用，签名是 `unsafe fn(*const ())`。
        // - `wake_by_ref`: 不消费 Waker，只通过引用唤醒任务时调用，签名是 `unsafe fn(*const ())`。
        // - `drop`: Waker 被丢弃时调用，签名是 `unsafe fn(*const ())`。
        // 返回值是 RawWakerVTable，表示 RawWaker 背后的操作函数表。
        let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);

        // RawWaker::new(data, vtable)
        // - `data: *const ()`: 执行器自定义的任务数据指针。
        //   当 Waker 被 clone、wake、wake_by_ref、drop 时，这个指针会原样传给 vtable 里的回调函数。
        // - `vtable: &'static RawWakerVTable`: RawWaker 的函数表，定义如何 clone、wake 和 drop。
        // 返回值是 RawWaker。RawWaker 本身只是底层表示，通常还要交给 Waker::from_raw 变成安全的 Waker。
        RawWaker::new(std::ptr::null(), vtable)
    }

    // Waker::from_raw(waker)
    // - `waker`: RawWaker::new 创建出来的底层 waker。
    // 返回值是 Waker，可以放进 Context 里传给 Future::poll。
    // 这是 unsafe API，因为调用者必须保证 RawWaker 的 data 指针和 vtable 回调函数是有效且匹配的。
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };

    // Context::from_waker(waker)
    // - `waker: &Waker`: 当前任务的唤醒器。
    // 返回值是 Context，Future::poll 通过它拿到 Waker，在 Pending 时注册唤醒逻辑。
    let mut cx = Context::from_waker(&waker);

    loop {
        // Future::poll(cx)
        // - `cx: &mut Context`: 当前任务的上下文，里面包含 Waker。
        // 返回值是 Poll::Ready(value) 或 Poll::Pending。
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                // 这个教学版执行器没有真正的唤醒机制，只是让出 CPU 后继续轮询。
                std::thread::yield_now();
            }
        }
    }
}

fn main() {
    let num = block_on(async {
        println!("测试环境");
        42
    });

    println!("结果是{num}");
}
