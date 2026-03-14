//! 自动化热键测试：模拟按右侧 Command，循环「开始录音 → 停止转写 → 再按」并打印日志。
//! 需在另一终端先运行 `RUST_LOG=info open-flow start`，本命令只负责模拟按键。

use anyhow::Result;
use std::time::{Duration, Instant};

/// 模拟一次「按下并松开」热键（Windows/Linux: 右侧 Alt；macOS: 右侧 Command）
fn simulate_hotkey() -> Result<()> {
    use rdev::{simulate, EventType, Key};
    let delay = Duration::from_millis(25);
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let key = Key::AltGr;
    #[cfg(target_os = "macos")]
    let key = Key::MetaRight;
    simulate(&EventType::KeyPress(key)).map_err(|e| anyhow::anyhow!("模拟按下失败: {:?}", e))?;
    std::thread::sleep(delay);
    simulate(&EventType::KeyRelease(key)).map_err(|e| anyhow::anyhow!("模拟松开失败: {:?}", e))?;
    std::thread::sleep(delay);
    Ok(())
}

/// 运行热键模拟循环：先等 daemon 就绪，再循环「按(开始) → 等(录音) → 按(停止) → 等(转写)」
pub async fn run_test_hotkey(
    cycles: u32,
    record_secs: u64,
    transcribe_wait_secs: u64,
    ready_wait_secs: u64,
) -> Result<()> {
    println!("⌨️  热键自动化测试（模拟热键：Windows/Linux 右侧 Alt，macOS 右 Command）");
    println!("   请先在另一终端运行: RUST_LOG=info open-flow start");
    println!();
    println!("   参数: {} 轮, 每轮录音约 {}s, 转写等待 {}s", cycles, record_secs, transcribe_wait_secs);
    println!("   启动后等待 {}s 再开始模拟（给 daemon 就绪时间）", ready_wait_secs);
    println!();

    std::thread::sleep(Duration::from_secs(ready_wait_secs));

    for i in 1..=cycles {
        let t0 = Instant::now();
        println!("[TestHotkey] 轮次 {} — 模拟按键: 开始录音", i);
        simulate_hotkey()?;
        std::thread::sleep(Duration::from_secs(record_secs));

        println!("[TestHotkey] 轮次 {} — 模拟按键: 停止并转写 (已录音 ~{}s)", i, record_secs);
        simulate_hotkey()?;
        std::thread::sleep(Duration::from_secs(transcribe_wait_secs));

        let elapsed = t0.elapsed().as_secs();
        println!("[TestHotkey] 轮次 {} 结束 (本轮耗时 {}s)，下一轮...", i, elapsed);
        println!();
    }

    println!("[TestHotkey] 全部 {} 轮完成。请查看 open-flow start 终端的 [Hotkey] 日志核对行为。", cycles);
    Ok(())
}
