# 进度记录与交接清单

## 当前状态
- **当前阶段**：P4 完成 -> 准备进入 P5（完整 UI）
- **当前执行模型**：Gemini-3.8-flash（因 Argon token 耗尽交接，审计文档见 docs/FLASH_AUDIT_LOG.md 与 docs/FLASH_AUDIT_LOG.json）
- **下一阶段**：P5（完整 UI）
- **下一执行模型**：Gemini-3.8-flash / Gemini-4-Argon（按用户偏好启动）

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

### 阶段 P1：核心数据模型 + 内核管理器（🧠 Gemini-4-Argon）- 【已完成】
- [x] 1. 定义统一中间模型 `ProxyNode`、`Subscription`、`Settings`、`RoutingMode` 并支持 Serde/TS 类型导出（`src/types/bindings.ts`）
- [x] 2. SQLite 存储层实现（schema、`001_initial.sql` migrations、线程安全 `Database` CRUD 与级联删除）
- [x] 3. 内核能力与选核矩阵 `core/capabilities.rs`（覆盖全部 17 种协议与 8 种传输层、XHTTP/mKCP 特殊路由、热重载 JSON 能力表与自动降级选核）
- [x] 4. 内核生命周期管理 `core/process.rs`、`manager.rs`（Windows Job Object `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 防残留进程、`CREATE_NO_WINDOW`、`Stopped → Starting → Running → Stopping / Crashed` 状态机、`LogRingBuffer` 实时日志推送、端口占用检测与自动顺延重写）
- [x] 5. 内核自动更新逻辑 `core/updater.rs`（支持经当前代理 HTTP/SOCKS 下载、SHA-256 校验、版本启动自检、失败自动回滚并更新 `cores.lock.json`）
- [x] 6. Tauri 基础命令导出与简单调试（`core_start`, `core_stop`, `core_restart`, `core_status`, `core_logs`, `core_clear_logs`, `core_default_config`，并在前端 `Dashboard.tsx` 与 `Logs.tsx` 联调最小可运行 `sing-box` 配置）

### 阶段 P2：分享链接与订阅解析（🧠 Gemini-4-Argon）- 【已完成】
- [x] 1. 各协议单链接解析器与生成器（`vmess`, `vless`, `trojan`, `ss`, `ssr`, `hysteria`, `hysteria2`, `tuic`, `wireguard`, 以及 `anytls`, `shadowtls`, `naive`, `snell`, `mieru`, `ssh`, `socks5`, `http` 共 17 种协议的 `parse` 与 `to_uri` 双向实现）
- [x] 2. 订阅格式自动识别与容错解析（自动识别纯文本多行分享链接、Base64/URL-safe Base64 编码订阅、Clash/Mihomo YAML、sing-box JSON、SIP008 JSON、WireGuard `.conf` INI，自动剥离 BOM/CRLF，单行损坏记录警告并跳过而不中断整体导入）
- [x] 3. 订阅网络下载与流量信息解析（支持自定义 `User-Agent`、超时与失败指数退避重试、可选经本地混合代理端口更新、解析 `subscription-userinfo` / `profile-update-interval` / `profile-web-page-url` 响应头）
- [x] 4. 节点去重与增量更新逻辑（基于 SHA-256 节点指纹 `compute_fingerprint()` 去重，`Database::sync_subscription_nodes` 在单事务内增量合并，保留已有节点的 `id`、用户自定义名称 `name`、排序 `sort_order`、测速延迟 `latency` 与分组归属）
- [x] 5. 剪贴板批量导入与二维码编解码（`qrcode` 生成 SVG/PNG 二维码，`rqrr` + `image` 解码图片二维码，Win32 GDI 虚拟屏幕截屏识别桌面二维码，并导出对应 Tauri IPC 命令与 `src/lib/ipc.ts` 封装）
- [x] 6. 测试用例与 Fixture 验证（`src-tauri/tests/fixtures/` 覆盖各协议 >= 5 个样本，含 IPv6、中文/Emoji 名称、URL 编码、缺可选字段、Clash YAML、sing-box JSON、SIP008 JSON、WireGuard `.conf`，全部通过 `parse -> to_uri -> parse` 往返一致性测试与 HTTP 模拟订阅服务器重试测试）

### 阶段 P3：配置生成器、路由与 DNS（🤖 Gemini-3.8-flash 代跑，记录至 docs/FLASH_AUDIT_LOG.md）- 【已完成】
- [x] 1. `ProxyNode` -> sing-box / mihomo / Xray 配置生成器（`generator/singbox.rs`, `generator/mihomo.rs`, `generator/xray.rs`，覆盖 17 种协议、全部 8 种传输层、TLS/REALITY/Vision 参数）
- [x] 2. 入站、出站组、分流模式定义（mixed 端口、可选局域网共享与认证，自动测速 `urltest`、手动选择 `selector`、故障转移 `fallback`、负载均衡 `loadbalance`）
- [x] 3. 路由规则集（`generator/routing.rs`：中国大陆直连 geosite/geoip、广告拦截、私网绕行，sing-box .srs 远程规则集与 mihomo rule-providers，用户自定义分流规则拖拽排序与图形化模型）
- [x] 4. DNS 分流与防泄露机制（`generator/dns.rs`：分流 DNS 国内直连 + 远程加密 DoH、Fake-IP 地址池分配与规则劫持）
- [x] 5. 跨内核协同与链式代理方案（`generator/mod.rs`：`generate_multi_core_relay` 实现 Xray 独占协议如 XHTTP/mKCP 作为上游 SOCKS5 本地中继，sing-box 作为主内核负责路由与 TUN；支持 `dialer_proxy` 前置代理）
- [x] 6. 快照测试与内核配置有效性 check（`tests/generator_check_test.rs`：调用实际 `sing-box.exe check`、`mihomo.exe -t`、`xray.exe run -test` 对生成配置进行端到端静态校验，全部绿灯通过）

### 阶段 P4：系统代理与 TUN 模式（🤖 Gemini-3.8-flash 代跑，记录至 docs/FLASH_AUDIT_LOG.md）- 【已完成】
- [x] 1. WinINet 系统代理设置与异常恢复（`system/sysproxy.rs`：Win32 注册表 + WinINet `InternetSetOptionW` 广播刷新，基线快照持久化，应用退出/崩溃 Drop 自动还原，局域网与自定义白名单 bypass）
- [x] 2. TUN 虚拟网卡配置与提权/后台服务方案（`system/tun.rs`：`TunSettings` 适配 Wintun 与 sing-box 1.14 / mihomo 1.19 TUN 入站语法注入，`system/elevation.rs`：TokenElevation 检测与 `ShellExecuteExW` "runas" 提权请求）
- [x] 3. 开机自启、系统托盘菜单（`system/autostart.rs`：注册表 `Run` 项静默启动管理，`lib.rs`：系统托盘右键菜单显隐窗口、一键开关系统代理与安全退出）
- [x] 4. 单实例互斥与 URL Scheme 协议注册（`system/deeplink.rs`：`novaproxy://` 与 `clash://` 协议注册与参数解析，单实例快速唤醒并导入订阅）
- [x] 5. 系统级端到端测试与真实内核静态校验（`tests/system_test.rs`：验证深层链接、系统代理周期、开机自启开关、以及启用 TUN 模式下 `sing-box.exe check` 与 `mihomo.exe -t` 配置校验）

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
- **2026-11-07 P1**：
  - 使用 `ts-rs` 将 Rust 端核心数据模型自动生成到 `src/types/bindings.ts`（通过 `model::tests::test_export_typescript_bindings` 在 `cargo test` 时自动同步更新），确保前后端类型 100% 对齐。
  - 在 `core/process.rs` 中直接通过 Win32 FFI 调用 `CreateJobObjectW` + `SetInformationJobObject(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)` + `AssignProcessToJobObject`，保证主进程异常退出或被强杀时 Windows 内核自动清理所有内核子进程。
  - 在 `src-tauri/build.rs` 中为 `--lib` 单元测试可执行文件注入 `libresource.a`（包含 `Microsoft.Windows.Common-Controls v6.0.0.0` SxS Manifest），解决 Windows GNU 目标下 `comctl32.dll` 的 `TaskDialogIndirect` 符号加载问题。
- **2026-11-07 P2**：
  - 统一抽象 `parser/uri_common.rs` 处理 v2rayN / Xray / sing-box 的传输层参数（`type`, `host`, `path`, `serviceName`, `mode`, `headerType`, `seed`, `ed`, `eh`）与安全层参数（`security`, `sni`, `alpn`, `fp`, `pbk`, `sid`, `spx`, `allowInsecure`），以及 IPv6 `[...]` 规范化。
  - 屏幕二维码扫描在 `parser/qrcode.rs` 中使用纯 Win32 GDI (`GetSystemMetrics` + `BitBlt` + `GetDIBits`) 捕获多显示器虚拟桌面并交由 `rqrr` 识别，避免引入额外 C/C++ 编译依赖。
  - 订阅增量更新 (`Database::sync_subscription_nodes`) 通过 SHA-256 节点指纹匹配已有节点，在更新连接参数的同时无损保留用户自定义节点名称、自定义排序、历史延迟及分组绑定。
- **2026-11-07 P3**：
  - 因 Gemini 4 Argon token 耗尽，阶段 P3 由 **Gemini 3.8 Flash** 完成全部功能实现并整理详细交接审计文档：`docs/FLASH_AUDIT_LOG.md` 与 `docs/FLASH_AUDIT_LOG.json`。
  - sing-box 1.14 语法调整：WireGuard 按照最新标准生成顶级 `endpoints: [{"type": "wireguard", "peers": [{"allowed_ips": ["0.0.0.0/0", "::/0"], ...}]}]`，outbound/selector 直连其 tag；ShadowTLS 采用外层 shadowtls 出站 + 内层 Shadowsocks 的 `detour` 链式调用；NaiveProxy 在无 `libcronet.dll` 时自动降级为具有相同 HTTP/2 CONNECT + TLS/uTLS 特性的 `http` 出站。
  - Xray-core 私有 IP 绕行规则使用显式 CIDR 列表生成，规避内核运行目录缺少 `geoip.dat` 时的解析失败。
  - 编写 `tests/generator_check_test.rs`，端到端调用真实的 `sing-box.exe check`、`mihomo.exe -t`、`xray.exe run -test` 对生成配置进行校验，并验证多内核协同中继（Xray 上游 + sing-box 主控）。全部 35 个测试（22 单元 + 4 内核校验 + 9 订阅与链接往返）全部通过。
- **2026-11-07 P4**：
  - 阶段 P4 持续由 **Gemini 3.8 Flash** 实现并记录至 `docs/FLASH_AUDIT_LOG.md` 与 `docs/FLASH_AUDIT_LOG.json`。
  - 系统代理使用 Windows 注册表 + WinINet API（`InternetSetOptionW` 39 与 37），确保 Chrome/Edge/系统组件免重启即刻感知代理变更；引入 `baseline` 快照机制，在应用异常退出或崩溃 Drop 时无条件恢复用户初始代理状态，杜绝断网痛点。
  - TUN 模式适配 sing-box 1.14 与 mihomo 1.19 最新标准（sing-box 废弃 `inet4_address` 改用 `address: ["172.19.0.1/30"]`，废弃 inbound 级 `sniff: true` 改用 route rule 级 `action: "sniff"`）。
  - UAC 提权使用 `GetTokenInformation(TokenElevation)` 精确检测管理员权限，并实现 `ShellExecuteExW("runas")` 平滑拉起。
  - 注册表协议关联实现 `novaproxy://` 和 `clash://` 协议支持，并在单实例机制下安全传递 Deep Link URL 到已有实例。
  - 编写 `tests/system_test.rs`，全套 45 个测试全绿通过。

---

## 已知问题与遗留项
- 在 PowerShell 会话中运行 Cargo 命令时，需确保将 WinLibs MinGW-w64 (`C:\Users\31752\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin`) 与 `$env:USERPROFILE\.cargo\bin` 加入 `$env:PATH`，且 `CARGO_TARGET_DIR` 保持 ASCII 路径（已在 `src-tauri/.cargo/config.toml` 中配置）。
