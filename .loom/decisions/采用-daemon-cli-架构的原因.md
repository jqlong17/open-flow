---
created: 2026-03-19T02:07:27.451Z
updated: 2026-03-19T02:07:27.451Z
tags: adr, architecture, daemon, cli, cross-platform
category: decisions
status: active
---

# 采用 Daemon + CLI 架构的原因

# 采用 Daemon + CLI 架构的原因

## 状态
Accepted

## 背景
Open Flow 的核心交互是全局热键触发语音输入，要求：

- 后台常驻，随时可响应热键
- 尽量低延迟（录音/转写/粘贴链路短）
- 跨平台支持（macOS/Windows/Linux）
- 便于运维和调试（日志、状态、模型管理）

若仅做单进程前台应用，难以兼顾“常驻监听 + 可脚本化运维 + 易调试”。

## 决策
采用 `Daemon + CLI` 双层架构：

- **Daemon**：负责常驻能力（热键监听、录音、ASR 推理、文本注入、托盘状态流转）。
- **CLI**：负责控制面（start/stop/status/setup/model/config/transcribe/test）。

其中平台差异在 daemon 边界模块内用 `#[cfg(target_os = "...")]` 做隔离，核心状态机与 ASR 流程尽量复用。

## 备选方案与取舍
### 方案 A：仅 GUI/托盘应用
- 优点：用户感知统一
- 缺点：脚本化能力弱，自动化测试和远程排障不便；CI/无桌面环境集成困难

### 方案 B：每次命令临时启动进程（无常驻）
- 优点：实现简单
- 缺点：热键场景不成立；初始化成本高，响应不稳定

### 方案 C：Daemon + CLI（采用）
- 优点：
  - 常驻能力与控制面解耦
  - 便于提供统一命令接口与自动化测试
  - 跨平台实现差异可在边界模块收敛
- 成本：
  - 生命周期管理更复杂（PID、前后台、退出时机）
  - 需要处理托盘事件循环与 daemon 协调

## 后果
### 正向
- 支持后台常驻低延迟热键体验
- CLI 可直接支持运维/测试/排障流程
- 架构清晰，便于平台能力渐进扩展

### 负向
- 多进程/多线程协作复杂度提升
- 平台事件循环（NSRunLoop/Win32/glib）需要长期维护

## 实施与边界
- Daemon 主编排：`src/daemon/mod.rs`
- CLI 生命周期管理：`src/cli/daemon.rs`、`src/main.rs`
- 平台差异入口：`src/hotkey/mod.rs`、`src/tray/mod.rs`、`src/text_injection/mod.rs`

## 复审触发条件
当出现以下情况时，需复审该决策：

- 目标平台或交互方式发生重大变化（例如移动端、浏览器端）
- 守护进程模式在某平台受系统策略长期限制
- 需要引入更重的 UI/配置中心并改变控制面职责

