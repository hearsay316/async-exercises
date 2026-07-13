# async-exercises

用于学习 Rust 异步编程的练习项目，同时附带一个 Word 加载项（Office Web Add-in）安装工具。

项目使用 Rust 2024 edition，通过 `cargo run --bin <名称>` 运行各个示例。

## 目录说明

仓库里混了两类内容：

| 类别 | 文件 | 说明 |
| --- | --- | --- |
| 异步练习 | `src/bin/Future.rs` | 手写 `Future` / `Poll` trait，理解异步基础概念 |
| 异步练习 | `src/bin/Future2.rs` | 最简单的「立即就绪」Future（`Ready42`） |
| 异步练习 | `src/bin/Future3.rs` | `Delay`：后台线程 + Waker 实现延迟完成的 Future |
| 异步练习 | `src/bin/Future4.rs` | `CountdownFuture`：递减计数器，演示自我唤醒 |
| 异步练习 | `src/bin/block_on.rs` | 手写一个最简阻塞执行器 `block_on`（`RawWaker` + `Waker`） |
| 异步练习 | `src/bin/block_on_1.rs` | 「虚假唤醒安全」练习：`FlagFuture`，每次 `poll` 都重新检查状态 |
| 工具 | `src/main.rs` | 小绿鲸 Word 加载项安装器：下载 manifest、写注册表 Developer 键、生成 sidload docx |
| 工具 | `src/bin/main_2.rs` | 上述工具的「共享文件夹目录」版本：创建 Windows 共享并注册 Trusted Catalog |

## 异步练习主题

这些例子按由浅入深排列，建议按下面的顺序阅读：

1. **`Future.rs`** —— 从零定义 `Future` 和 `Poll`，看清 `poll(self: Pin<&mut Self>, cx: &mut Context)` 这个签名的来历。
2. **`Future2.rs`** —— `Ready42`：`poll` 直接返回 `Poll::Ready(42)`，说明 `async fn` / `async {}` 最朴素的本质。
3. **`Future3.rs`** —— `Delay`：在 `poll` 里启动后台线程模拟异步等待，完成后用 `Waker::wake()` 通知执行器再 `poll` 一次，演示完整的 **Future ↔ Waker ↔ 执行器** 三方合约。
4. **`Future4.rs`** —— `CountdownFuture`：在 `Pending` 分支里调用 `cx.waker().wake_by_ref()` 主动唤醒自己，配合 `#[tokio::main]` 看效果。
5. **`block_on.rs`** —— 自己实现 `block_on`：用 `RawWakerVTable` 拼出一个「什么都不做」的 Waker，循环 `poll` 直到 `Ready`。
6. **`block_on_1.rs`** —— 虚假唤醒（spurious wakeup）安全练习：`poll` 里**永远先检查真实状态**，绝不能被唤醒后就假设任务已完成。

## 运行方式

每个文件都是独立的可执行 binary：

```bash
# 运行某个异步练习
cargo run --bin Future
cargo run --bin Future2
cargo run --bin Future3
cargo run --bin Future4
cargo run --bin block_on
cargo run --bin block_on_1

# 运行默认的 main（src/main.rs，Word 加载项安装器）
cargo run

# 运行「共享目录」版安装器
cargo run --bin main_2
```

## 依赖

- `tokio` —— 异步运行时（仅部分示例使用 `#[tokio::main]`）
- `anyhow` —— 错误处理
- `quick-xml` —— 解析 Office 加载项 manifest XML
- `reqwest`（`blocking`）—— 下载 manifest
- `winreg` —— 读写 Windows 注册表
- `dirs` —— 获取 `LocalAppData` 等目录
- `zip` —— 拼装 sideload 用的 `.docx`（docx 本质是 zip 包）

## 关于 Word 加载项工具

`src/main.rs` / `src/bin/main_2.rs` 是配套的实用工具，工作流程大致是：

1. 从 `https://www.xljsci.com/LTSCOfficeV2/manifest.xml` 下载加载项清单；
2. 解析其中的 `<Id>` / `<Version>` / `<Host>`（仅支持 `Host = Document` 即 Word）；
3. 写入 Windows 注册表，把加载项注册到 Word：
   - `main.rs`：走 `Developer` sideload 路径，并生成一个内嵌 webextension 的 `.docx`，打开即用；
   - `main_2.rs`：走「共享文件夹」可信目录路径，需要**管理员权限**（程序会自动请求 UAC 提权重启）创建 `\\<计算机名>\XljOfficeAddinCatalog` 共享，并写入 `TrustedCatalogs` 注册表项；
4. 启动 Word。

> 注意：这部分代码只面向 **Windows**，依赖注册表、`net share`、UAC 等 Windows 机制。

## 备注

- 仓库里的 `target/` 是编译产物，`.idea/` 是 RustRover 配置，均已通过 `.gitignore` 忽略。
- 异步练习里的中文注释偏「教学向」，适合刚接触 Rust 异步、想搞懂 `Future` 底层机制的同学。
