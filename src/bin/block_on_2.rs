// ===========================================================================
// 核心概念：Join<A, B> 是一个 Future 组合器——本身是 Future，内部包装了
// 两个子 Future。每次 poll() 同时推进两个子 Future，直到两者都完成。
//
// 设计理由：
// 1. MaybeDone 枚举追踪每个子 Future 的状态（Pending/Done/Taken）
// 2. 手动实现 Unpin——因为我们只和 Unpin 子 Future 配合使用，
//    且不会将 Pin 投影到字段上，这样做是安全的
// 3. get_mut() 依赖 Unpin 实现——如果 Self: Unpin，则 Pin<&mut Self>
//    可以直接安全地通过 get_mut() 拿到 &mut Self
// 4. Taken 状态用于在匹配时安全地取走输出值（mem::replace 而非 move）
// ===========================================================================

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// 并发轮询两个 Future，在两者都完成时返回它们的输出元组
pub struct Join<A, B>
where
    A: Future,
    B: Future,
{
    a: MaybeDone<A>,   // → 第一个子 Future 的状态追踪器
    b: MaybeDone<B>,   // → 第二个子 Future 的状态追踪器
}

/// 追踪单个子 Future 的三态状态
enum MaybeDone<F: Future> {
    Pending(F),          // → 仍在执行中，持有 Future 本体
    Done(F::Output),     // → 已完成，持有输出值
    Taken,               // → 输出已被取走（用于 mem::replace 的中间状态）
}

// → 手动为 Join 实现 Unpin。由于我们只和 Unpin 子 Future 配合，
// 且不会把 Pin 投射到字段上，这是安全的。
// 有了 Unpin，poll() 中就可以安全地使用 self.get_mut()。
impl<A: Future + Unpin, B: Future + Unpin> Unpin for Join<A, B> {}

impl<A, B> Join<A, B>
where
    A: Future,
    B: Future,
{
    pub fn new(a: A, b: B) -> Self {
        Join {
            a: MaybeDone::Pending(a), // → 初始：两者都处于 Pending 状态
            b: MaybeDone::Pending(b),
        }
    }
}

impl<A, B> Future for Join<A, B>
where
    A: Future + Unpin,
    B: Future + Unpin,
{
    type Output = (A::Output, B::Output);

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        //      ^^^^^^^ 由于 Self: Unpin，可以安全获取 &mut Self
        //              无需 unsafe 的 Pin 投影

        // → 如果 A 尚未完成，轮询 A
        if let MaybeDone::Pending(ref mut fut) = this.a {
            //  ^^^^^^^^^^^^^^^ 模式匹配：仅在 Pending 状态下才匹配
            //                  ref mut fut：获取对内部 Future 的可变引用
            if let Poll::Ready(val) = Pin::new(fut).poll(cx) {
                //  ^^^^^^^^^^^^^^^^^ Pin::new()：为子 Future 创建 Pin 包装
                //  注意：这里要求子 Future: Unpin，所以 Pin::new() 安全
                this.a = MaybeDone::Done(val);
                // → A 完成：将状态从 Pending 切换为 Done，保存结果
            }
        }

        // → 如果 B 尚未完成，轮询 B（逻辑与 A 相同）
        if let MaybeDone::Pending(ref mut fut) = this.b {
            if let Poll::Ready(val) = Pin::new(fut).poll(cx) {
                this.b = MaybeDone::Done(val);
                // → B 完成：切换到 Done 状态
            }
        }

        // → 检查两者是否都已完成
        match (&this.a, &this.b) {
            (MaybeDone::Done(_), MaybeDone::Done(_)) => {
                // → 两者都完成，安全地取出输出值
                // 使用 mem::replace 而非直接 move：因为 match 中只能通过引用访问
                let a_val = match std::mem::replace(&mut this.a, MaybeDone::Taken) {
                    //              ^^^^^^^^^^^^^^^^^ 将 this.a 替换为 Taken，
                    //              同时返回原来的值（所有权的转移）
                    MaybeDone::Done(v) => v,   // → 取出 A 的结果
                    _ => unreachable!(),       // → 已知是 Done，不可能走这里
                };
                let b_val = match std::mem::replace(&mut this.b, MaybeDone::Taken) {
                    MaybeDone::Done(v) => v,   // → 取出 B 的结果
                    _ => unreachable!(),
                };
                Poll::Ready((a_val, b_val))
                // → 返回两个结果的元组
            }
            _ => Poll::Pending, // → 至少一个仍在 Pending，继续等待
        }
    }
}
// ===========================================================================
// 一个最简单的"忙等"执行器：循环 poll Future，Pending 时让出 CPU 再重试。
// 注意：它的 Waker 是 noop（什么都不做），所以靠忙轮询推进，仅供教学。
// ===========================================================================
fn block_on<F: Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn noop_raw_waker() -> RawWaker {
        fn no_op(_: *const ()) {}
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);
        RawWaker::new(std::ptr::null(), vtable)
    }

    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);

    // 用 Box::pin 固定 future，因为 async 块是 !Unpin
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

// ===========================================================================
// 一个简单的"异步任务"：用 poll_fn 手写，每次 poll 打印一次进度，
// 第 N 次才返回 Ready。这样能在输出里清楚地看到两个任务交错执行。
// ===========================================================================
fn countdown(name: &'static str, from: u32) -> Pin<Box<dyn Future<Output = u32>>> {
    let mut left = from;
    Box::pin(std::future::poll_fn(move |_cx| {
        if left == 0 {
            println!("  [{name}] 完成!");
            Poll::Ready(from)
        } else {
            println!("  [{name}] 剩余 {left}");
            left -= 1;
            Poll::Pending
        }
    }))
}

fn main() {
    println!("=== Join: 两个 Future 并发轮询演示 ===\n");

    // async 块是 !Unpin，用 Box::pin 包裹后变成 Pin<Box<F>>，它是 Unpin 的
    // → 这样才能满足 Join<A: Unpin, B: Unpin> 的约束
    let fut_a = countdown("A", 6);
    let fut_b = countdown("B", 2);

    // Join::new 同时轮询 A 和 B，直到两者都完成
    let join = Join::new(fut_a, fut_b);

    // 用我们的忙等执行器驱动 Join
    let (result_a, result_b) = block_on(join);

    println!("\n=== 结果: A={result_a}, B={result_b} ===");
    println!("注意输出里 A 和 B 的进度是交错的——这就是单线程并发!");
}

// 用法（async 块是 !Unpin，所以用 Box::pin 包裹它们以满足 Unpin 约束）：
// let (page1, page2) = Join::new(
//     Box::pin(http_get("https://example.com/a")), // → 堆分配 + 固定
//     Box::pin(http_get("https://example.com/b")), // → 堆分配 + 固定
// ).await;
// → 两个请求同时在同一个线程上交错执行！
