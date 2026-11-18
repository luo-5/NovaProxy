# NovaProxy 全量审计、稳定性加固与端到端优化报告

> 审计范围：NovaProxy 仓库根目录（对照 `docs/PLAN.md`、`docs/PROGRESS.md`、`docs/FLASH_AUDIT_LOG.md` 全仓核查并修复 P3–P7 遗留缺陷与断点）  
> 内核基线：`sing-box 1.14.4`、`Mihomo Meta v1.19.33`、`Xray-core 26.x`

---

## 一、本轮发现并修复的关键问题清单

### 1. 协议解析、三内核生成与真实运行时校验
- **Hysteria 2 端口跳跃（Port Hopping）字段修复**
  - **问题**：解析器已提取 `mport` / `ports` / `hop_interval`，但在 `generator/singbox.rs`（`server_ports` / `hop_interval`）与 `generator/mihomo.rs`（`ports` / `hop-interval`）生成时被忽略。
  - **修复**：完整映射 `server_ports`（转换为 `sing-box` `start:end` 范围列表）、`hop_interval`（秒级格式）及 Mihomo YAML `ports` / `hop-interval`，并在 `tests/parser_fixtures_test.rs`、`tests/generator_check_test.rs` 与 `tests/e2e_workflow_test.rs` 通过真实二进制校验。
- **`sing-box 1.14.4` 直连 DNS 启动致命报错（实测捕获）**
  - **问题**：`generator/dns.rs` 为直连 DNS 服务器生成 `"detour": "direct"`。`sing-box check` 允许通过，但真实运行 `sing-box.exe` 启动即失败：`FATAL: start dns/https[dns-direct]: detour to an empty direct outbound makes no sense`。
  - **修复**：改造 `compile_singbox_dns_server` 接收 `detour: Option<&str>`，在 `None` 或 `"direct"` 时省略 `"detour"` 字段；已在双核协同实测与端到端启动切换测试中验证通过。
- **Clash 策略分组持久化闭环**
  - **问题**：Clash YAML 解析出的 `url-test` / `fallback` / `selector` / `load-balance` 分组未稳定映射入库与清理。
  - **修复**：在 `storage/db.rs` 实现 `Database::sync_groups`（按节点指纹重映射持久节点 ID，按 `sub:{id}:...` 作用域维护并在 `delete_subscription` 时级联清理），新增 `commands::list_groups` 暴露给前端。

### 2. 多内核自动路由、协同中继与防回退保护
- **多内核选核与协同调度闭环**
  - **修复**：在 `core/manager.rs` 实现 `CoreManager::plan_start_request`，结合 `core/capabilities.rs` 自动按节点与 TUN 状态调度：
    - SSR / Snell / Mieru 自动切换至支持的内核（Mihomo）；
    - XHTTP / mKCP 等 Xray 独占传输层（或显式选 `sing-box` 搭配 Mihomo 独占协议）生成双核协同中继（上游 SOCKS5 + `sing-box` 主路由/TUN），通过 `core/process.rs` 的 `patch_relay_outbound_port` 对齐端口冲突顺延，统一托管于 Windows Job Object 并同步合并日志与停止清理，避免静默回退为 `direct`。

### 3. 真实可用性测速、流式监控与健康故障转移
- **消除单行 Stub 与流式阻塞（`monitor/traffic.rs` / `monitor/connections.rs`）**
  - **修复**：在 `monitor/traffic.rs` 实现 `TrafficSnapshot` 与 `fetch_clash_traffic`，在 `monitor/connections.rs` 实现 `fetch_clash_connections` / `close_clash_connection` 并于 `commands/mod.rs` 解耦调用。
  - **关键修复**：`sing-box` / `mihomo` 的 `/traffic` 接口是无限分块 NDJSON 流，直接 `.json()` 会阻塞超时；改为按 newline 分块读取首条完整 JSON 后立即返回，同时全部本地 Clash API Reqwest 客户端强制 `.no_proxy()` 防止走系统代理回环。
- **TCP Ping + 真实代理 URL 延迟测速（`monitor/latency.rs`）**
  - **修复**：支持 `LatencyTestMode::Tcp`（有界并发 `Semaphore(32)`）与 `LatencyTestMode::Url`。
  - **实测坑点修复**：排查发现 `sing-box 1.14.4` 的 Clash API `/proxies/{tag}/delay` 对自定义/回环 URL 仍会拨号 `www.gstatic.com:443`；因此对单节点隔离临时内核探测（`test_single_node_latency` fallback 路径）统一在等待 `mixed_port` 与 `clash_api_port` 双就绪后通过 `probe_url_via_socks_proxy(proc.mixed_port, ...)` 执行真实 SOCKS5 -> 远端/本地 204 请求，兼容所有三内核与 UDP/QUIC 协议（Hy2 / TUIC / WireGuard / XHTTP）。
- **真实代理探活与防抖动（`monitor/health.rs`）**
  - **修复**：运行期优先通过代理真实探活（支持 UDP 协议）替换裸 TCP 探测，增加 `failover_cooldown` 防止频繁切线震荡，通过 `plan_start_request` 计算并切换备用低延迟节点。

### 4. 生产级稳定性、系统接管与桌面二进制启动 Bug 修复
- **非测试 Panic 清零**：通过脚本扫描全量 `src-tauri/src/` 非 `#[cfg(test)]` 代码，`.unwrap()` 与 `.expect()` 全部清理（命中数 **0**）；修复 `system/network.rs` `start()` 为可恢复 `Result` 与 `NotifyAddrChange` 并发防重入，升级 `lib.rs` 异常退出与 Panic 恢复 WinINet 钩子。
- **桌面实测捕获异步反应器 Panic (`lib.rs` / `monitor/health.rs`)**
  - **问题**：构建真实 `novaproxy.exe` 启动后立即抛出 `thread 'main' panicked at src\monitor\health.rs:332:9: there is no reactor running`。
  - **修复**：在 `monitor/health.rs` 抽出 `async fn run_loop`，在 `lib.rs` 的 `.setup()` 中通过 `tauri::async_runtime::spawn` 启动；同时在 `perform_graceful_shutdown` 中显式调用 `handles._single_instance.shutdown()`，保证 Tauri 退出时即时删除 `instance_ipc.json`、停止双核子进程并完整还原系统代理。
- **系统集成补齐（`system/service.rs` / `system/tun.rs` / `system/deeplink.rs` / `core/updater.rs`）**
  - `system/service.rs` 改为真实调用 `sc.exe query NovaProxyHelper`（带 `CREATE_NO_WINDOW`）并解析 `ServiceState`。
  - `system/tun.rs` 新增 `check_wintun_status` 检查 `wintun.dll` 与权限状态。
  - `system/deeplink.rs` 实现 Win32 命名 Mutex 单实例检测 + token 校验本地回环 IPC 转发，`AppSettings.register_clash_scheme` 默认关闭并支持设置页显式接管/取消 `clash://`。
  - `core/updater.rs` 支持带代理下载候选包、SHA-256 校验、运行中安全停核->替换->版本自检->失败自动回滚并持久化 `cores.lock.json` 到本地用户数据目录。

### 5. 前端 UI 补齐与 Vite 构建分包
- `vite.config.ts` 增加函数式 `manualChunks` 拆分（`charts`、`react-vendor`、`index`、`i18n`、`icons`、`tauri`），消除 >500 kB 大包与循环分包警告。
- `src/App.tsx` 与 `src/pages/Subscriptions.tsx` 接入 `app://deep-link` 自动刷新与悬浮 Toast 提醒。
- `src/pages/Dashboard.tsx` 新增双核协同中继（`upstream_core` / `upstream_pid` / `relay_port`）与实际 Clash API 端口显示。
- `src/pages/Proxies.tsx` 基于 `src/lib/listHelpers.ts`（`computeVirtualGridWindow`）实现虚拟网格滚动支持千级以上节点、新增 `TCP / URL (204)` 测速模式切换、Clash 策略分组过滤栏、Hysteria2 端口跳跃与多内核徽标。
- `src/pages/Rules.tsx` 补齐自定义规则 HTML5 拖拽排序与失败回滚。
- `src/pages/Settings.tsx` 消除所有 `alert()` 调用，接入内核版本与在线更新、TUN/Wintun 与服务就绪状态、`clash://` 协议关联开关；`zh-CN.json` 与 `en-US.json` 完成 100% 键一致测试。

---

## 二、第二轮深度全面审核与 Bug 清零（2026-11-17）

在 P8 基线全量通过之后，针对跨端联动、真实复杂订阅边界及 Tauri 运行时机制开展了第二轮逐模块排错，发现并根除以下隐蔽缺陷：

1. **Tauri 2 运行时权限清单缺失（`src-tauri/capabilities/default.json`）**
   - **根因**：此前工程未配置 Capabilities 文件，`src-tauri/gen/schemas/capabilities.json` 为 `{}`，运行时会拦截 `@tauri-apps/api/event` 的 `listen()` 订阅（`core://log`、`core://status`、`app://deep-link`）。
   - **修复**：新增 `src-tauri/capabilities/default.json`，为 `windows: ["main"]` 授予 `core:default` 与 `shell:allow-open` 权限，重新构建后验证 `gen/schemas/capabilities.json` 已生效。
2. **订阅指纹、解析器与 Mihomo/sing-box 配置生成边界 Bug**
   - `model/node.rs`：`compute_fingerprint()` 纳入 Hy2 跳跃端口、完整传输层参数及 TLS SNI/Reality 公钥，防止同机多 CDN 变体误合并。
   - `generator/mihomo.rs` & `core/hot_reload.rs`：新增 `resolve_mihomo_proxy_names()` 统一重名消歧映射并透传到 `select_active_node`、`HealthMonitor` 与 `batch_probe_with_clash_api`；Shadowsocks `plugin-opts` 改用 YAML Map（映射 `obfs-local` -> `obfs`）并通过 `mihomo -t` 实测；转义 `\n`/`\r`/`\t` 等多行控制字符防止 YAML 注入；剔除自环与悬空 `dialer-proxy` / `detour`。
   - `generator/routing.rs`：测试真实 `mihomo -t` 捕获并修复 `DST-PORT,8000-9000, 10000,PROXY` 逗号分裂导致动作解析报错问题，在 Mihomo 拆分多条规则、在 `sing-box` 将 `"-"` 端口范围标准化为 `":"`（如 `8000:9000`）、在 `xray` 去除逗号空格。
   - `core/process.rs`：修复 `tun_mode == true` 时 `sing-box` 首入站为无端口 `tun-in` 导致端口冲突顺延跳过 `mixed-in` 的缺陷；精确匹配 Mihomo Relay 出站块并避免双核 `upstream_settings.socks_port` 端口撞车。
   - `parser/{extra_uris.rs, clash_yaml.rs}`：补全 `socks5`/`https` TLS 查询参数往返、`anytls` 的 `user:pass` / `?password=` 解析、`mieru` `port-range` 别名、Clash WireGuard `peers:` 回退及 `reserved: [u8; 3]` 解析。
3. **系统代理防黑洞与配置脏标记状态机**
   - `system/sysproxy.rs` & `commands/mod.rs`：`restore_original()` 在未激活且无基线时直接返回 `Ok(())` 不误伤外部代理；`core_start`/`core_restart`/`core_stop` 自动联动系统代理开关；运行前未启动内核勾选系统代理仅落盘不接管 WinINet 造成断网；为增删改节点/订阅/规则/DNS/设置接入 `mark_config_dirty()` 与 `is_config_dirty()`，防止热切绕过结构性变更。
   - `monitor/latency.rs`：`tcp_ping` 使用 `format_host_for_uri` 正确格式化 IPv6；`test_nodes_batch` 尊重 `node.preferred_core` 并隔离清洗失效 `dialer_proxy`。
4. **前端表单协议栈、输入提交与状态重同步修复**
   - `AddNodeModal.tsx`：依据 `bindings.ts` Tagged Union 修复手动添加界面，覆盖 9 种可选协议的完整 `"kind"` 字段与表单校验。
   - `Settings.tsx` & `Rules.tsx`：端口与逗号分隔 DNS 改为 `onBlur`/回车校验提交（防止逐字符保存合法性破坏及难以输入 `", "`），统一清理 Toast 定时器。
   - `Dashboard.tsx` & `Proxies.tsx` & `Subscriptions.tsx` & `Logs.tsx` & `Connections.tsx` & `QrCodeModal.tsx`：处理 `core://status` 自动重拉节点/代理状态、失败测速红色 `Timeout` 状态区分、切换核心不误带默认 `sing-box` JSON、日志 `seq` 去重、删除节点/订阅联动清理 `activeNode` 与虚拟滚动计算，以及中英双语键 100% 校验。

---

## 三、最终全量验证结果（末次编辑后实测）

| 验证项 | 命令 / 路径 | 实测结果 |
| --- | --- | --- |
| 非测试 `.unwrap()`/`.expect()` 审计 | Python 扫描 `src-tauri/src/**/*.rs` 首个 `#[cfg(test)]` 之前全部代码 | **`NON_TEST_UNWRAP_HITS=0`** |
| Rust 代码格式 | `cargo fmt --check` | 通过（exit 0） |
| Rust Clippy 静态检查 | `cargo clippy --all-targets -- -D warnings` | 通过（0 Rust/Clippy 警告） |
| Rust 单元测试 (`--lib`) | `cargo test -- --test-threads=4` | **48 / 48 通过**（0 失败，0 跳过） |
| 端到端工作流测试 (`e2e_workflow_test`) | `cargo test -- --test-threads=4` | **1 / 1 通过** |
| 三内核配置静态校验 (`generator_check_test`) | `cargo test -- --test-threads=4` | **7 / 7 通过**（含 `SS obfs` Map、`8000:9000`、`mihomo -t`、`sing-box check`、`xray run -test`） |
| 订阅与协议 Fixture 测试 (`parser_fixtures_test`) | `cargo test -- --test-threads=4` | **13 / 13 通过**（含 CDN 指纹、WG `peers`/`reserved`、SOCKS5/HTTP TLS/AnyTLS/Mieru） |
| 稳定性与容灾集成测试 (`stability_test`) | `cargo test -- --test-threads=4` | **5 / 5 通过** |
| 系统集成测试 (`system_test`) | `cargo test -- --test-threads=4` | **5 / 5 通过** |
| 前端代码规范与类型检查 | `pnpm lint` & `pnpm tsc --noEmit` | **0 错误，0 警告** |
| 前端单元测试 (`Vitest`) | `pnpm test` | **2 文件 10 / 10 通过**（含中英双语 100% 键一致性） |
| 前端生产构建 (`Vite`) | `pnpm build` | 通过（最大分包 `charts` 388.84 kB） |
| 真实桌面二进制构建与运行实测 | `pnpm tauri build --debug --no-bundle` -> 启动 `novaproxy.exe` | 主窗口 `"NovaProxy"`（HWND `4785976` / `"Tauri Window"`）存活 >4s，`WM_CLOSE` 安全退出 `EXIT_CODE=0`，`instance_ipc.json` 清除且 WinINet 基线（`ProxyEnable=0`）无损 |

---

## 五、第三轮升级：四端跨平台兼容（Windows/macOS/Linux/Android）与持久化日志遥测诊断系统（2026-11-17）

### 1. 跨平台后端、打包与工具链改造
- **`Cargo.toml` / `build.rs` / `.cargo/config.toml`**：支持 `["staticlib", "cdylib", "rlib"]` 输出；将 `reqwest` 切换为纯 Rust `rustls-tls` 消除 Android/Linux OpenSSL 交叉编译障碍；`tray-icon` 限定仅桌面目标启用；新增 `flate2`、`tar`、`libc` 与 `tracing-subscriber`；修复 Windows MinGW GCC 16 `default-manifest.o` 与 `cdylib` 导出符号超限冲突。
- **全平台系统抽象（`src-tauri/src/system/`）**：为 macOS（`networksetup` / LaunchAgents / `osascript` / `launchctl` / `utun` / `Info.plist` + `RunEvent::Opened`）、Linux（`gsettings` + `kwriteconfig` / XDG autostart + `xdg-mime` / `pkexec` + `systemctl` / `/dev/net/tun` + `flock` IPC）及 Android（安全空操作与原生 `VpnService` 桥接）提供完整实现，`get_platform_info` 向前端报告实时能力矩阵。
- **进程与更新器跨平台（`core/{process, updater, manager}.rs`, `model/node.rs`, `parser/qrcode.rs`）**：Android 核心命名适配 `lib{singbox,mihomo,xray}.so`（加载自只读可执行 `nativeLibraryDir`），Unix 子进程使用 `setpgid` + `PR_SET_PDEATHSIG(SIGKILL)` + 进程组回收，更新器适配 `.zip`/`.tar.gz`/`.gz` 并显式拦截 Android 运行期 W^X 原地替换。

### 2. Android 端原生 VPN 插件与 APK 打包（`src-tauri/plugins/tauri-plugin-android-vpn`）
- 实现 `NovaProxyVpnService` + `hev-socks5-tunnel` v2.20.0 非 Root 全局 TUN 接管服务（`addDisallowedApplication(packageName)` 防回环，IPv4/IPv6 + UDP 转发至运行期 `mixed_port`，验证 DNS 原始 IP 传入），并在 `commands/mod.rs` (`core_start`/`core_restart`/`core_stop`) 自动联动授权与启停。
- 新增 `tauri.android.conf.json`、`scripts/configure-android.mjs`、`scripts/build-android.ps1`、`scripts/fetch-cores.sh` 及增强 `scripts/fetch-cores.ps1`，本地成功打包通用 APK（`arm64-v8a` + `x86_64`，`294,905,078` 字节）并通过 `aapt2` 验证全部 `.so`、服务声明及深层链接协议。

### 3. 全链路持久化日志与性能遥测优化系统
- **滚动落盘与脱敏（`monitor/logs.rs`）**：引入 `PersistentLogWriter`（4 MiB 自动轮转 `novaproxy.log` 与 JSON 行格式 `events.jsonl`）、`NovaTracingLayer`（捕获 Rust 子系统事件）与前端异常通道（`log_frontend_event`），统一经过 `sanitize_log_message` 掩码凭据后保存与推送。
- **性能遥测与优化洞察（`monitor/telemetry.rs`）**：记录内核启动拆解耗时、协议测速成功率/超时统计、崩溃退出、Auto Failover 与网络变更次数，自动生成中英双语 `OptimizationInsight` 排障建议。
- **前端工作台与诊断包导出（`Logs.tsx` / `Settings.tsx` / `Dashboard.tsx` / `Sidebar.tsx`）**：新增移动端底部导航栏、按平台自适应隐藏桌面设置项、Android VPN 权限状态卡、日志模块筛选与磁盘日志浏览视图，以及 `export_diagnostic_bundle` 一键导出脱敏 `.zip` 诊断压缩包。

### 4. 最新全量验证结果（末次代码编辑后）
| 验证项 | 执行命令 / 目标 | 最新实测结果 |
| --- | --- | --- |
| 非测试 `.unwrap()`/`.expect()` 审计 | 扫描 `src-tauri/src` 与 `plugins/tauri-plugin-android-vpn/src` | **`NON_TEST_UNWRAP_HITS=0`** |
| Rust 格式化与 Clippy | `cargo fmt --check` & `cargo clippy --all-targets -- -D warnings` | **0 错误，0 Rust 警告** |
| Rust 全部单元测试与集成测试 | `cargo test -- --test-threads=4`（含新增日志/遥测测试用例） | **83 / 83 通过**（52 unit + 1 e2e + 7 generator + 13 parser + 5 stability + 5 system） |
| Android NDK 交叉检查 | `cargo clippy --target aarch64-linux-android --lib -- -D warnings` & `cargo check --target x86_64-linux-android --lib` | **0 错误，0 警告** |
| Android 通用 APK 构建与剖析 | `scripts/build-android.ps1` + `aapt2 dump` | `app-universal-debug.apk`（`294,905,078` 字节）通过，含 `arm64-v8a` + `x86_64` 全三核 `.so` + `libhev-socks5-tunnel.so` + `libnovaproxy_lib.so` |
| 前端质量与构建 | `pnpm lint` & `pnpm exec tsc --noEmit` & `pnpm test` & `pnpm build` | ESLint 0 警告，`tsc` 0 错误，Vitest **10 / 10** 通过，Vite 构建通过 |
| 桌面端构建与日志/退出实测 | `pnpm tauri build --debug --no-bundle` -> 启动 `novaproxy.exe` | 窗口 `"NovaProxy"` 存活 >5s，生成 `%LOCALAPPDATA%\NovaProxy\logs\{novaproxy.log, events.jsonl}`（6 条初始化事件），`WM_CLOSE` 返回 `EXIT_CODE=0`，`instance_ipc.json` 已清除，WinINet 基线无损 |
| macOS / Linux 范围说明 | `.github/workflows/{ci,release}.yml` | 已接入 CI/Release 矩阵构建配置；当前宿主为 Windows x86_64，macOS 与 Linux 二进制运行实测待 CI runner 或对应宿主验证 |

---

## 六、已知限制与说明
1. **TUN 模式与管理员服务 (`NovaProxyHelper`)**：当前工程已打通真实的 Windows 服务状态查询与前端 Wintun/权限就绪度反馈，由于 NSIS 配置为 `currentUser` 免提权用户级安装模式，尚未随包内置单独的本地系统提权服务 EXE，因此使用 TUN 模式时需通过设置页“以管理员身份重新启动”完成 UAC 提权。
2. **远程外网节点与在线升级网络验证范围**：本轮端到端实测涵盖本地回环 SOCKS5 + HTTP 204 真实隧道探活、三内核静态校验与真实双核/热切生命周期、模拟 Clash REST API 及损坏/哈希失配自动回滚；未在测试环境内硬编码外部 VPS 真实密码/UUID 或向 GitHub Release 发起真实外部文件替换。
3. **MinGW GNU 工具链链接提示**：Windows `x86_64-pc-windows-gnu` 下同时存在 `tauri-build` 与 `build.rs` 清单资源会打印 1 条 `.rsrc merge failure` 链接器提示（不影响二进制运行与 `-D warnings` Rust 检查）。
