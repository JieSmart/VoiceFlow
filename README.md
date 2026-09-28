# 语流

<p align="center">
  <img src="src-tauri/icons/app-icon.svg" width="96" height="96" alt="语流">
</p>

<p align="center">
  <strong>说了就上屏。</strong><br>
  Windows 桌面语音输入。全局热键唤醒，识别完成后写入刚才的输入框。
</p>

<p align="center">
  <a href="https://github.com/JieSmart/VoiceFlow/releases/latest"><img src="https://img.shields.io/github/v/release/JieSmart/VoiceFlow?label=release" alt="release"></a>
  <a href="https://github.com/JieSmart/VoiceFlow/actions/workflows/release.yml"><img src="https://github.com/JieSmart/VoiceFlow/actions/workflows/release.yml/badge.svg" alt="build"></a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-blue" alt="Windows">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green" alt="MIT"></a>
</p>

VoiceFlow is a local Windows speech-to-text input tool. Speak, and the text appears in the app you were using.

## 安装

1. 打开 [Releases](https://github.com/JieSmart/VoiceFlow/releases/latest)
2. 下载 NSIS 安装包（`*_x64-setup.exe`）
3. 安装到当前用户目录 `%LOCALAPPDATA%\Programs\VoiceFlow`

安装包未代码签名，SmartScreen 可能提示「未知应用」，选择仍要运行即可。这版只提供 NSIS，不提供 MSI。

登录后可选择开机自启动：默认只留托盘图标。从开始菜单或桌面打开仍会显示主窗口。

## 第一次使用

1. 允许麦克风权限。
2. 打开 **设置 → 语音识别**，选一个引擎并保存密钥（云识别）或下载本地模型。
3. 点到要输入的窗口，按 `Ctrl+Shift+Space` 开始说话，再按一次结束。
4. 默认在识别完成后写入刚才的输入框。也可以在设置里改为先审阅再上屏。
5. 需要整理措辞时，在设置里配置大模型；未配置或失败时使用原文。

| 快捷键 | 作用 |
|---|---|
| `Ctrl+Shift+Space` | 开始 / 结束录音 |
| `Ctrl+Shift+V` | 显示或隐藏主窗口 |
| `Ctrl+Alt+G` | 审阅时用 AI 整理 |
| `Ctrl+Shift+M` | 开关热词 |

可改为按住说话：按住唤醒键录音，松开结束。按住太短会取消。

## 它做什么

- 说话时气泡显示识别中的文字，不抢原窗口焦点。
- 停录后写入原输入框；流式引擎边说边出字，Qwen3-ASR / Whisper 是整段离线识别。
- 热词做识别后的替换，可按当时前台窗口标题决定是否生效。
- 可选 AI 整理（OpenAI 或 Claude）。
- 热词库、输入记录、本地模型下载、数据目录迁移都在主窗口里。

识别引擎：OpenAI Realtime、讯飞、腾讯云、本地 Sherpa Zipformer，以及 Qwen3-ASR / faster-whisper。

## 隐私

- 本机运行，无账号、无云同步。
- 原始录音不落盘，识别在内存中完成。
- API Key 只存在本机 SQLite，不进设置文件、日志或 git。
- 选用云引擎时，音频或文本会发到对应厂商；本地 Sherpa / Qwen3-ASR / Whisper 可离线。

## 环境

- Windows 10 / 11（x64）
- 从源码构建还需要 [Node.js](https://nodejs.org/) 20+、[Rust](https://rustup.rs/) stable、[Visual Studio C++ 生成工具](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- 本地 GPU 加速可选 NVIDIA + CUDA 12

## 从源码运行

```powershell
npm install
npm run tauri dev
```

```powershell
npm test
npx tsc -p tsconfig.app.json --noEmit
cd src-tauri
cargo test --lib
cd ..
npm run tauri build
```

推送 `v*` 标签会触发 [GitHub Actions](https://github.com/JieSmart/VoiceFlow/actions)：先跑测试，再发布 Windows x64 安装包。

浏览器只能预览界面，没有全局热键、托盘和上屏：

```powershell
npm run dev -- --host 127.0.0.1 --port 4173
```

## 限制

- 不做账号、云同步、在线更新。
- 热词不是命令，不会打开外部应用。
- 未列出的 ASR / LLM 厂商尚未接入。

## 反馈

请到 [Issues](https://github.com/JieSmart/VoiceFlow/issues) 说明 Windows 版本、识别引擎（云或本地）、以及能否复现。不要在 issue 里粘贴 API Key。

## 协议

[MIT](LICENSE)
