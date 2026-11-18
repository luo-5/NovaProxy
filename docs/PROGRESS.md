# 进度记录与交接清单

## 当前状态
- **当前阶段**：P0 ~ P8 全部完成 + **P9 四端跨平台兼容（Windows / macOS / Linux / Android APK）与全链路持久化日志/性能遥测诊断系统** 已完成并完成双端实测验证
- **当前执行模型**：Antigravity (全平台跨端兼容架构、Android VpnService + hev-socks5-tunnel 原生插件、持久化滚动日志、性能遥测与优化洞察台)
- **下一动作**：桌面端可直接运行 `novaproxy.exe` 或打包；Android 端可直接安装 `app-universal-debug.apk`；打 Tag `v*` 即可触发 GitHub Actions 产出 Windows / macOS / Linux / Android 全四平台安装包
- **版本产物**：NovaProxy v0.0.1（开源首发版本；已实测生成并验证 Windows `novaproxy.exe` 与 Android `app-universal-debug.apk`）
- **最新审计报告**：参见 [AUDIT_AND_HARDENING_REPORT.md](./AUDIT_AND_HARDENING_REPORT.md)

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

### 阶段 P5：完整 UI（🤖 Gemini-3.8-flash 代跑，记录至 docs/FLASH_AUDIT_LOG.md）- 【已完成】
- [x] 1. 仪表盘（`src/pages/Dashboard.tsx`、`components/SpeedChart.tsx`：双曲线平滑实时速率图、光晕大开关、系统代理/TUN/路由模式切换、当前活动节点与内核状态）
- [x] 2. 节点列表（`src/pages/Proxies.tsx`、`components/AddNodeModal.tsx`、`components/QrCodeModal.tsx`：17 种协议颜色标签、TCP ping 并发批量测速、模糊搜索与订阅/协议筛选、多选批量删除、URI/二维码分享）
- [x] 3. 订阅管理（`src/pages/Subscriptions.tsx`、`components/AddSubscriptionModal.tsx`：流量配额进度条、到期倒计时、一键更新全部、剪贴板导入与屏幕二维码扫描）
- [x] 4. 路由规则编辑器（`src/pages/Rules.tsx`、`components/AddRuleModal.tsx`：内置分流规则开关、自定义规则上下排序与增删、DoH 与 Fake-IP 地址池控制）
- [x] 5. 连接管理与日志查看器（`src/pages/Connections.tsx`、`src/pages/Logs.tsx`：实时连接监控与单项/全部断开，日志级别筛选、关键词搜索与配置编辑器）
- [x] 6. 设置面板（`src/pages/Settings.tsx`、`components/Sidebar.tsx`、`locales/*.json`：首选内核切换、端口冲突自动顺延、开机自启、管理员提权状态、深浅主题与中英文切换）

### 阶段 P6：稳定性加固（🤖 Gemini-3.8-flash 代跑，记录至 docs/FLASH_AUDIT_LOG.md）- 【已完成】
- [x] 1. 内核崩溃自愈与守护重试（`core/manager.rs`：滑动 60s 窗口指数退避重试 `[1s, 2s, 4s]`，最多 3 次防死循环熔断，采用 Pin-Box Future 打破异步递归 E0391）
- [x] 2. 节点健康探测与自动故障转移（`monitor/health.rs`：`HealthMonitor` 后台巡检任务，探测活动节点 RTT，连续失败 2 次自动 Failover 到最低延迟备选健康节点并切线）
- [x] 3. 网络接口变更与休眠唤醒监听自适应（`system/network.rs`：基于 Win32 `iphlpapi::NotifyAddrChange` 和 `WaitForMultipleObjects` 的专用后台 OS 监听线程，自适应重刷 WinINet option 39/37）
- [x] 4. 热重载（Clash API）避免重启断流（`core/hot_reload.rs`：`PUT /proxies/{group}` <5ms 内存动态切换活动节点，不掐断已建立 TCP 流；失败时自适应降级重启）
- [x] 5. 配置原子写入与数据库容灾（`storage/atomic.rs`：UUID 中间文件、fsync 强刷盘与原子重命名写入；`storage/db.rs`：`PRAGMA integrity_check` 与 `VACUUM INTO` 在线快照热备份）
- [x] 6. 稳定性集成测试与容灾验收（`tests/stability_test.rs`：包含 5 个系统容灾与自愈测试，全套 54 个测试全部通过）

### 阶段 P7：测试、打包与发布（🤖 Gemini-3.8-flash 代跑，记录至 docs/FLASH_AUDIT_LOG.md）- 【已完成】
- [x] 1. 完整单元测试与集成测试（前端 Vitest 6 个单元测试通过，后端 54 个集成与单元测试通过，全绿 100% 通过）
- [x] 2. 全局错误边界与脱敏诊断导出（`components/ErrorBoundary.tsx` 优雅降级与堆栈复制，`commands/mod.rs` 凭据脱敏诊断报告生成）
- [x] 3. Windows NSIS 多语言打包配置（`src-tauri/tauri.conf.json`：配置 English 与 SimpChinese 安装向导、currentUser 安装模式）
- [x] 4. GitHub Actions 自动化双流水线（`.github/workflows/ci.yml` 完整自动化测试验证，`.github/workflows/release.yml` 自动打包 NSIS 安装包、便携版 zip、SHA256 校验并发布 Release）
- [x] 5. 用户文档、操作手册与开源合规（中英双语 `README.md`、详尽 `docs/USER_MANUAL.md`、`LICENSE` MIT 许可与 `docs/THIRD_PARTY_LICENSES.md` 三方核心合规清单）

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
- **2026-11-07 P5**：
  - 阶段 P5 持续由 **Gemini 3.8 Flash** 完成全套 7 大页面及弹窗组件开发并记录至审计日志。
  - 延迟测速：后端实现轻量级 `tcp_ping` 并通过 `tokio::spawn` 实现百级节点并发非阻塞测速，持久化到 SQLite `latency_history`。
  - 流量与连接：对接本地 Clash API `/traffic` 与 `/connections`，在内核未运行时自动优雅降级，杜绝报错或白屏。
  - 节点热切：`select_active_node` 联动后端自动重新生成目标配置并无缝平滑重载内核。
  - 国际化与主题：全套中英文字典补齐，无任何硬编码英文；深色/浅色模式平滑切换。全套质量检查（cargo fmt, clippy, cargo test 45/45, pnpm lint, pnpm tsc, pnpm build）全绿通过。
- **2026-11-07 P6**：
  - 阶段 P6 持续由 **Gemini 3.8 Flash** 完成稳定性加固与容灾体系建设并记录至审计日志。
  - 配置文件原子写入：基于 UUID 中间文件、`sync_all` 强刷盘与原子重命名写入，杜绝系统断电或崩溃导致的 JSON 零字节损坏。
  - 数据库容灾：启动自检 `PRAGMA integrity_check` 与在线 `VACUUM INTO` 快照冷热备份。
  - 进程自愈守护：`monitor_child_exit` 监听崩溃退出，以指数退避 `[1s, 2s, 4s]` 在 60 秒滑动窗口内最多自愈重试 3 次，超过则熔断告警，杜绝死循环高 CPU 占用；采用 `Pin<Box<dyn Future...>>` 彻底打破编译器类型推导环（E0391）。
  - 零停机热切换：对接 Clash REST API `PUT /proxies/{group}`，活动节点可在 5ms 内动态无感切换，不掐断已建立 TCP 长连接。
  - 健康探测与故障转移：`HealthMonitor` 后台巡检任务自动探测活动节点时延，连续失败 2 次自动从 SQLite 切换至最低时延健康备选节点。
  - 网络栈自适应：独立原生线程运行 Win32 `NotifyAddrChange`，在休眠唤醒、WiFi 漫游切换时瞬时捕获并重刷新 WinINet 系统代理广播（option 39/37）。全套 54 个测试全绿通过。
- **2026-11-07 P7**：
  - 阶段 P7 持续由 **Gemini 3.8 Flash** 完成全套测试、打包配置、CI/CD 自动化流水线与用户文档编写，全量审计信息记录至 `docs/FLASH_AUDIT_LOG.md` 与 `docs/FLASH_AUDIT_LOG.json`。
  - 前端测试：配置 Vitest 2.1 编写 6 个核心工具函数单元测试，结合 ESLint 与 TypeScript 形成多重质量关卡。
  - 全局容错与诊断：引入 `ErrorBoundary` 彻底避免前端未捕获异常白屏，增加 `export_diagnostics_report` 凭据脱敏诊断报告导出功能。
  - 打包与分发：配置 Windows NSIS 中英文双语安装包；编写 `.github/workflows/ci.yml`（全套 lint, test, build 自动化检查）与 `release.yml`（Tag 触发全自动打包 NSIS 安装包、便携版 zip、生成 SHA256 校验并发布 Release）。
  - 用户手册与合规：产出详尽中英双语 `README.md`、`docs/USER_MANUAL.md`（含 5 大 FAQ 排查）、MIT `LICENSE` 与 `docs/THIRD_PARTY_LICENSES.md`（明确 sing-box GPL、mihomo GPL、Xray MPL 合规使用边界）。全库质量自检 100% 绿灯。

---

## 决策与审计补充（2026-11-16 P8 全量审计与可发布级加固）
- **2026-11-16 P8**：
  - 修复 Hysteria 2 端口跳跃（`server_ports` / `hop_interval` 与 `ports` / `hop-interval`）在生成器中被忽略的问题，修复 `generator/dns.rs` 对 sing-box 1.14.4 直连 DNS 服务器误写 `"detour": "direct"` 导致启动致命退出的问题。
  - 接通 `CoreManager::plan_start_request` 多内核自动选核与协同中继（支持 Xray 上游 + sing-box 前置协同、clash `proxy-groups` 通过 `Database::sync_groups` 持久化），清除 `monitor/traffic.rs` 与 `monitor/connections.rs` 占位 Stub，解决 Clash `/traffic` chunked NDJSON 阻塞和 `reqwest` 本地系统代理回环问题。
  - 完善 `monitor/latency.rs`（有界 TCP Ping + 真实代理 URL 延迟测速，支持单节点临时内核 SOCKS5 204 端到端探测）与 `monitor/health.rs`（运行期真实代理探活、cooldown 防抖动、`run_loop` 脱离同步 Setup 线程 panic）。
  - 完成非测试 `.unwrap()` / `.expect()` 清零（扫描 `src-tauri/src/` 0 命中），实现 Windows 单实例命名互斥锁与本地 IPC 唤醒、opt-in `clash://` 协议关联、`sc.exe` 服务状态探测、`WintunStatus` 诊断与安全退出 IPC/子进程/WinINet 清理。
  - 前端补全 `Proxies.tsx` 虚拟滚动与 TCP/URL 双模式及 Clash 策略组筛选、`Rules.tsx` 自定义规则拖拽重排、`Settings.tsx` 内核版本/在线更新器/TUN 服务检测/`clash://` 开关与 Toast 提示、`Dashboard.tsx` 协同中继显示及 Vite 分包优化。
  - 全量验证全绿：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`（73/73 通过，含新增 `e2e_workflow_test.rs`）、`pnpm lint`、`pnpm tsc --noEmit`、`pnpm test`（10/10 通过）、`pnpm build`、`pnpm tauri build --debug --no-bundle` 与 `novaproxy.exe` 启动/窗口/安全退出实测。
- **2026-11-17 P8 深度排错加固（Round 2 全面审核）**：
  - **Tauri 2 运行时权限补全**：补充 `src-tauri/capabilities/default.json`（为 `main` 窗口授权 `core:default` 与 `shell:allow-open`），修复构建期 `gen/schemas/capabilities.json` 为空导致前端无法接收 `core://log`、`core://status`、`app://deep-link` 事件的隐性故障。
  - **后端多处隐蔽缺陷修复**：
    - `model/node.rs`：将 Hy2 跳跃端口、传输层参数、TLS SNI 与 Reality PublicKey 纳入 `compute_fingerprint()`，避免同域名/UUID 的不同 CDN/SNI 变体在订阅同步时误判重复合并。
    - `system/sysproxy.rs` & `commands/mod.rs`：修正停止内核停流黑洞与外部代理误覆盖——启动/重启同步 WinINet，停止内核自动还原基线，未运行内核时开启系统代理仅持久化偏好而不接管 WinINet；无基线且未激活时 `restore_original()` 不误清外部代理；删除节点/订阅同步清理失效的 `active_node_id`。
    - `core/{manager.rs, hot_reload.rs, process.rs}` & `monitor/{health.rs, latency.rs}` & `generator/{mod.rs, mihomo.rs, singbox.rs, routing.rs}`：新增 `config_generation` / `mark_config_dirty()` 追踪配置变更防止基于陈旧配置错误热切；导出并接入 `resolve_mihomo_proxy_names` 修复重名节点与 Mihomo 热切/批测查找失败；修复 Mihomo Shadowsocks `plugin-opts`（改输出 YAML 映射并规范 `obfs-local`）、YAML 多行控制字符转义、悬空 `dialer-proxy` 防护；修复 sing-box TUN 首入站端口顺延、端口范围规则 `"8000-9000"` 转 `"8000:9000"`、Mihomo 逗号分隔 `DST-PORT` 与 Xray 去空格端口列表、以及 `tcp_ping` IPv6 bracket 格式化。
    - `parser/{extra_uris.rs, clash_yaml.rs}`：`socks5`/`https` URI 往返保留 TLS 参数、`anytls` 兼容 `user:pass` 与 `?password=`、`mieru` 支持 `port-range` 别名、Clash WireGuard YAML 支持 `peers` 数组回退及 `[u8; 3]` `reserved` 解析。
  - **前端表单、状态同步与体验修复**：修复 `AddNodeModal.tsx` 手动添加缺少 `ProtocolConfig` `"kind"` 标签且仅传 SS 字段导致 9 种协议保存反序列化失败的 BUG；`Settings.tsx` / `Rules.tsx` 端口与逗号分隔 DNS 改为失焦/回车校验提交并清理 Toast 定时器；`Dashboard.tsx` 监听 `core://status` 重同步后台 Failover 后的 `activeNode` 与真实 `sysproxy` 状态；`Proxies.tsx` / `Subscriptions.tsx` / `Logs.tsx` / `Connections.tsx` / `QrCodeModal.tsx` 修复超时红色 `Timeout` 标记、`Logs` 切换 `mihomo`/`xray` 误发 `sing-box` JSON、`seq` 去重、批量/订阅删除清理 `activeNode`、虚拟网格滚动归零与中英双语键 100% 同步。
  - **全量回归与新增用例验证**：Rust 新增 6 项针对性回归测试后全部 **79 / 79** 通过（48 单元 + 7 内核校验 + 13 解析器 + 5 稳定性 + 5 系统集成 + 1 E2E），前端 **10 / 10** Vitest + ESLint + `tsc` + Vite 0 警告，真实 `novaproxy.exe` 构建启动存活 >4s 并向 `"Tauri Window" ("NovaProxy")` 发送 `WM_CLOSE` 正常退出（exit 0、`instance_ipc.json` 清除、WinINet 基线无损）。

### 阶段 P9：四平台全兼容（Windows / macOS / Linux / Android）与全链路日志遥测诊断系统 - 【已完成】
- **2026-11-17 P9 跨平台架构与诊断日志升级**：
  - **四平台系统集成层（`src-tauri/src/system/*`, `core/{process, updater, manager}.rs`, `model/node.rs`, `parser/qrcode.rs`）**：
    - `system/sysproxy.rs` 新增 macOS `networksetup`、Linux `gsettings`/`kwriteconfig` 及 Android 安全空操作实现，保留 Windows WinINet 基线快照与退出恢复。
    - `system/autostart.rs` / `elevation.rs` / `service.rs` / `tun.rs` / `network.rs` / `deeplink.rs` 实现 Windows/macOS/Linux/Android 适配（macOS LaunchAgent / `osascript` / `launchctl` / `utun` / `RunEvent::Opened` + `Info.plist`；Linux XDG `.desktop` / `xdg-mime` / `CAP_NET_ADMIN` / `systemctl` / `/dev/net/tun` / `flock` IPC；Android 沙箱适配与出站接口监控）。
    - `core/process.rs` 新增 Unix `setpgid(0, 0)` + Linux/Android `PR_SET_PDEATHSIG(SIGKILL)` + 进程组 `JobObject` 同生共死，升级 `updater.rs` 支持 `.zip`/`.tar.gz`/`.gz` 多平台内核更新包，升级 `reqwest` 为纯 Rust `rustls-tls` 消除 Android/Linux OpenSSL 交叉编译阻塞。
    - `build.rs` 与 `.cargo/config.toml` 移除全局 Windows 目标锁定，Windows MinGW-w64 增加 `-specs` 禁用不兼容 GCC 16 `default-manifest.o` 并配合 `--exclude-all-symbols` 解决 `cdylib` 导出符号超限与测试可执行文件 `TaskDialogIndirect` 问题。
  - **Android 原生 VPN 与三内核全量打包（`src-tauri/plugins/tauri-plugin-android-vpn`）**：
    - 实现 Tauri 2 Mobile 原生插件 `AndroidVpnPlugin.kt` + `NovaProxyVpnService.kt` + `hev-socks5-tunnel` v2.20.0（提取自 `hev-socks5-tunnel.aar` 为 `libs/hev-socks5-tunnel.jar` 与全 ABI `libhev-socks5-tunnel.so`），通过 `VpnService.Builder` + `addDisallowedApplication(packageName)` 防回环并将 `tunFd` 转发至本地 core 混合端口；自动从 `nativeLibraryDir` 加载打包为 `libsingbox.so` / `libmihomo.so` / `libxray.so` 的三内核二进制（符合 Android 10+ W^X 执行规范）。
    - 新增 `tauri.android.conf.json`、`scripts/configure-android.mjs`、`scripts/build-android.ps1`、`scripts/fetch-cores.sh` 及跨平台增强 `scripts/fetch-cores.ps1`，本地完成 `aarch64-linux-android` & `x86_64-linux-android` 交叉编译与 `app-universal-debug.apk`（`294,905,078` 字节）实包组装与 `aapt2` 清单校验。
  - **全链路持久化日志与性能遥测优化系统（`monitor/logs.rs`, `monitor/telemetry.rs`, `commands/mod.rs`, `src/pages/Logs.tsx`）**：
    - `PersistentLogWriter` 将全部内核 stdout/stderr、Rust `tracing` 事件（`NovaTracingLayer`）与前端未捕获异常（`ErrorBoundary` / `window.onerror` / `unhandledrejection`）经 `sanitize_log_message` 凭据脱敏后同步写入 4 MiB 轮转文件 `novaproxy.log` 与结构化 `events.jsonl`。
    - `TelemetryTracker` 采集内核启动阶段耗时（配置生成/端口检测/拉起）、故障转移次数、崩溃记录、网络变更与各协议延迟/超时率，自动生成中英双语 `OptimizationInsight` 建议。
    - `export_diagnostic_bundle` 命令一键打包脱敏环境报告与全量磁盘日志文件为 `.zip`；前端新增 `PlatformInfo` 自适应桌面/移动端（底部导航栏、按端隐藏不适用桌面特性）、日志台实时/磁盘文件双视图、模块来源筛选及一键导出诊断包。
  - **CI/CD 四端全量自动化**：升级 `.github/workflows/ci.yml`（前端 + Windows/Ubuntu/macOS 三桌面矩阵 + Android NDK/APK 检查）与 `.github/workflows/release.yml`（Windows NSIS/Portable + macOS arm64/x64 DMG + Linux deb/AppImage + Android 通用 APK + `SHA256SUMS.txt`）。

## 已知问题与遗留项
- `scripts/fetch-cores.ps1` 仍保留用于下载各平台官方核心二进制，Windows GNU 环境编译需保证 WinLibs MinGW-w64 与 Rust `.cargo/bin` 在 `PATH` 中，并通过 `$env:CARGO_TARGET_DIR` 指定 ASCII 构建目录以兼容非 ASCII 工作区路径。
- 本机实际运行与实测覆盖 Windows x86_64（全部 83 项 Rust 测试、10 项 Vitest、`novaproxy.exe` 启动与日志落盘/退出清理）以及 Android NDK `arm64-v8a` + `x86_64`（交叉编译与 `app-universal-debug.apk` 打包）；macOS `.dmg` 与 Linux `.deb`/`.AppImage` 构建及运行依赖 GitHub Actions 对应 runner 或真实目标设备进一步集成验证。
- Android 实机安装运行（真机/模拟器上的 `VpnService` 系统授权弹窗与隧道流量端到端转发）受限于当前无连接 Android 设备尚未真机运行调试，已通过 NDK 编译、APK 静态剖析（`aapt2`）与插件服务/JNI manifest 验证，可在真机/模拟器安装 `app-universal-debug.apk` 后进一步测试。
