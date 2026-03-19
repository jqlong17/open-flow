---
created: 2026-03-19T00:32:45.236Z
updated: 2026-03-19T00:32:45.236Z
tags: architecture, rust, asr, cross-platform, daemon
category: concepts
status: active
---

# Open Flow 系统架构概览

# Open Flow 系统架构概览

## 产品定位
Open Flow 是面向 AI 编程场景的开源语音输入工具，核心链路为：

`全局热键 -> 录音 -> ASR 转写 -> 写入剪贴板 -> 自动粘贴（按平台能力）`

目标是“后台常驻 + 本地推理 + 快速响应 + 跨平台一致体验（以 macOS 为主）”。

## 总体架构
项目采用 `Daemon + CLI` 架构：

- `open-flow daemon` 负责常驻运行、监听热键、录音、推理和文本注入。
- `open-flow` CLI 负责启动/停止/状态查询、模型管理、配置、单次转写与测试命令。
- 平台差异通过 Rust `#[cfg(target_os = "...")]` 收敛在边界模块，核心流程尽量复用。

## 核心状态机
Daemon 状态机为：

`Idle -> Recording -> Processing -> Idle`

- 第一次按热键进入录音
- 再次按热键停止录音并进入转写
- 完成后回到空闲

## ASR 处理链路
本地 ASR 以 SenseVoiceSmall ONNX 为核心，典型流程：

1. 麦克风采集（16kHz mono f32）
2. 实时 ring buffer
3. 停止录音后写临时 wav
4. 特征预处理：fbank -> LFR -> CMVN
5. ONNX Runtime 推理
6. CTC greedy decode
7. 文本输出与注入

关键预设：
- `quantized`（默认，约 230MB）
- `fp16`（约 450MB）

## 跨平台差异（高频关注）
- **macOS**：热键为右 Command（CGEventTap）；文本注入为剪贴板 + osascript Cmd+V；有菜单栏托盘和设置界面。
- **Windows**：热键为右 Alt（rdev AltGr）；文本默认仅写剪贴板，用户手动 Ctrl+V。
- **Linux**：热键为右 Alt（rdev AltGr）；文本注入为剪贴板 + xdotool/wtype。

## 代码边界（维护入口）
- `src/daemon/mod.rs`：核心主循环（热键、录音、转写、注入编排）
- `src/asr/`：预处理、ONNX 推理、解码
- `src/hotkey/mod.rs`：平台热键监听实现
- `src/text_injection/mod.rs`：平台粘贴策略
- `src/tray/mod.rs`：多平台托盘实现与事件循环适配
- `src/cli/`：命令入口与 daemon 生命周期控制
- `src/common/config.rs`：跨平台配置路径与配置读取

## 设计取舍与约束
- 以本地模型和离线能力优先，隐私敏感场景可不依赖云服务。
- 平台适配优先保证 macOS 主路径稳定，Win/Linux 功能在模块边界隔离。
- 启动与转写响应优先于复杂 UI；CLI/托盘互补。

## 关键参考
- `README.md`
- `docs/ARCHITECTURE.md`
