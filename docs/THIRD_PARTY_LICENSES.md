# 第三方开源许可证与合规清单 (Third-Party Licenses)

NovaProxy 在开发与构建过程中集成、依赖或调用了以下开源项目。本项目严格遵守各上游项目的开源协议规范。

---

## 1. 代理与 VPN 核心引擎 (Proxy Cores)

NovaProxy 通过子进程隔离调度外部编译好的二进制程序，不与 GPL 代码进行静态或动态链接，遵循松散耦合进程间通信规范：

| 核心名称 | 项目仓库 | 开源许可证 | 使用形式 | 许可说明 |
|---|---|---|---|---|
| **sing-box** | [SagerNet/sing-box](https://github.com/SagerNet/sing-box) | **GPL-3.0-or-later** | 独立可执行文件 (`sing-box.exe`) | 经进程命令行 `run -c` 启动，通过标准管道和 Clash API 通信 |
| **mihomo** | [MetaCubeX/mihomo](https://github.com/MetaCubeX/mihomo) | **GPL-3.0-or-later** | 独立可执行文件 (`mihomo.exe`) | 经进程命令行 `-d` 启动，通过 RESTful Clash API 通信 |
| **Xray-core** | [XTLS/Xray-core](https://github.com/XTLS/Xray-core) | **MPL-2.0** | 独立可执行文件 (`xray.exe`) | 经进程命令行 `run -c` 启动，负责特定协议本地中继 |
| **Wintun** | [WireGuard/wintun](https://git.zx2c4.com/wintun/) | **MIT / BSD** | 动态链接库 (`wintun.dll`) | Windows TUN 虚拟网卡底层驱动 |
| **hev-socks5-tunnel** | [heiher/hev-socks5-tunnel](https://github.com/heiher/hev-socks5-tunnel) | **MIT** | JNI 共享库 (`libhev-socks5-tunnel.so`) | Android `VpnService` tun2socks 底层隧道转发库 |

---

## 2. 客户端框架与基础组件 (Frameworks & Libraries)

| 组件 / 库 | 所属机构 / 作者 | 开源许可证 | 用途 |
|---|---|---|---|
| **Tauri** | Tauri Programme / Commons Clause | **MIT OR Apache-2.0** | 跨平台桌面应用外壳与 Rust IPC 桥接 |
| **React** | Meta Platforms, Inc. | **MIT** | 前端声明式 UI 渲染引擎 |
| **Tailwind CSS** | Tailwind Labs Inc. | **MIT** | 前端实用优先样式系统 |
| **Zustand** | Paul Henschel & contributors | **MIT** | 轻量级响应式前端状态管理 |
| **Lucide Icons** | Lucide Project | **ISC** | 矢量 UI 图标集 |
| **Recharts** | Recharts Group | **MIT** | 实时网络速率可视化图表 |
| **Rusqlite** | The Rusqlite Developers | **MIT** | 嵌入式 SQLite 驱动 |
| **Tokio** | Tokio Contributors | **MIT** | 异步网络运行时与后台调度任务 |
| **Reqwest** | Sean McArthur | **MIT OR Apache-2.0** | HTTP 客户端 (订阅拉取与 Clash API) |
| **Windows API** | Microsoft / win32 | **MIT / Apache-2.0** | 注册表、WinINet 系统代理与网络事件监听 |

---

## 3. 合规声明

- 本客户端**不提供任何代理服务器节点或科学上网服务**。
- 用户必须自行提供合法有效的订阅或自建节点配置。
- 使用本项目及相关核心必须严格遵守所在国家或地区的法律法规。
