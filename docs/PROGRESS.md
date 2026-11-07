# 进度记录与交接清单

## 当前状态
- **当前阶段**：P0 完成 -> 准备进入 P1
- **当前执行模型**：Gemini-3.8-flash
- **下一阶段**：P1（核心数据模型 + 内核管理器）
- **下一执行模型**：Gemini-4-Argon

---

## 阶段任务清单

### 阶段 P0：环境与脚手架（🤖 Gemini-3.8-flash）- 【已完成】
- [x] 1. 安装 Rust 工具链（rustup, stable-x86_64-pc-windows-gnu 搭配 MinGW-w64 UCRT，规避 VS BuildTools UAC 依赖），配置 WebView2 与 pnpm
- [x] 2. 创建 Tauri 2 + React + TS + Vite 项目（NovaProxy），配置 Tailwind CSS、shadcn/ui 变量、Zustand、TanStack Query、i18next、cargo clippy/rustfmt
- [x] 3. 按架构规范建立完整目录结构骨架（`model`, `parser`, `generator`, `core`, `system`, `monitor`, `storage`, `commands` 及其对应子模块）
- [x] 4. 编写 `scripts/fetch-cores.ps1`，成功下载 sing-box (v1.14.4)、mihomo (v1.19.33)、Xray-core (v26.10.31) 二进制，记录校验和至 `cores.lock.json`
- [x] 5. 配置 Tauri `externalBin` / resources 资源打包规则，并在 `src-tauri/.cargo/config.toml` 配置默认构建目标与 ASCII target-dir
- [x] 6. 初始化 Git 仓库，完善 `.gitignore`（严格忽略内核二进制与构建产物，保留 .gitkeep）
- [x] 7. 确立 `docs/PLAN.md` 与 `docs/PROGRESS.md`，验收标准检查全绿（`cargo fmt`、`cargo clippy`、`cargo test`、`pnpm build` 全部通过）

### 阶段 P1：核心数据模型 + 内核管理器（🧠 Gemini-4-Argon）
- [ ] 1. 定义统一中间模型 `ProxyNode`、`Subscription`、`Settings`、`RoutingMode` 并支持 Serde/TS 类型导出
- [ ] 2. SQLite 存储层实现（schema、migrations、CRUD）
- [ ] 3. 内核能力与选核矩阵 `core/capabilities.rs`
- [ ] 4. 内核生命周期管理 `core/process.rs`、`manager.rs`（Job Object、状态机、日志收集、端口冲突检测）
- [ ] 5. 内核自动更新逻辑 `core/updater.rs`
- [ ] 6. Tauri 基础命令导出与简单调试

### 阶段 P2：分享链接与订阅解析（🧠 Gemini-4-Argon）
- [ ] 1. 各协议单链接解析器与生成器（vmess, vless, trojan, ss, ssr, hy2, tuic 等）
- [ ] 2. 订阅格式自动识别与解析（Base64, Clash YAML, sing-box JSON, SIP008）
- [ ] 3. 订阅网络下载与流量信息解析
- [ ] 4. 节点去重与增量更新逻辑
- [ ] 5. 剪贴板导入与二维码解析
- [ ] 6. 测试用例与 Fixture 验证

### 阶段 P3：配置生成器、路由与 DNS（🧠 Gemini-4-Argon）
- [ ] 1. `ProxyNode` -> sing-box / mihomo / Xray 配置生成器
- [ ] 2. 入站、出站组、分流模式定义
- [ ] 3. 路由规则集（geosite, geoip, 自定义规则）
- [ ] 4. DNS 分流与防泄露机制
- [ ] 5. 跨内核协同与链式代理方案
- [ ] 6. 快照测试与内核配置有效性 check

### 阶段 P4：系统代理与 TUN 模式（🧠 Gemini-4-Argon）
- [ ] 1. WinINet 系统代理设置与异常恢复
- [ ] 2. TUN 虚拟网卡配置与提权/后台服务方案
- [ ] 3. 开机自启、系统托盘菜单
- [ ] 4. 单实例互斥与 URL Scheme 协议注册

### 阶段 P5：完整 UI（🤖 Gemini-3.8-flash）
- [ ] 1. 仪表盘（速度图表、开关、当前节点）
- [ ] 2. 节点列表（多选、排序、延迟测速、详情弹窗）
- [ ] 3. 订阅管理
- [ ] 4. 路由规则编辑器
- [ ] 5. 连接管理与日志查看器
- [ ] 6. 设置面板（DNS、TUN、内核、主题、多语言、备份）

### 阶段 P6：稳定性加固（🧠 Gemini-4-Argon）
- [ ] 1. 内核崩溃自愈与守护重试
- [ ] 2. 节点健康探测与自动故障转移
- [ ] 3. 网络接口变更与休眠唤醒监听自适应
- [ ] 4. 热重载（Clash API）避免重启断流
- [ ] 5. 配置原子写入与数据库容灾
- [ ] 6. 24小时内存泄漏与句柄排查、错误边界兜底

### 阶段 P7：测试、打包与发布（🤖 Gemini-3.8-flash）
- [ ] 1. 完整单元测试、集成测试
- [ ] 2. GitHub Actions 构建流与 Windows NSIS/便携版打包
- [ ] 3. 自动更新与签名验证
- [ ] 4. 用户文档与中英文 README

---

## 决策记录（Architecture Decision Log）
- **2026-11-07 P0**：选定技术栈 Tauri 2 (Rust) + React 18 + TS + Tailwind + Vite。内核采用多内核架构（主 sing-box，副 mihomo 与 Xray），通过子进程 + Job Object 隔离运行。
- **2026-11-07 P0**：Windows 端默认启用 WebView2，系统代理使用 WinINet API，TUN 规划支持提权服务。

---

## 已知问题与遗留项
- 需等待 Rustup 安装完成并在当前会话加载 Rust 环境变量。
- 确认 C++ 编译器（MSVC 或 MinGW）满足 Tauri 2 编译要求。
