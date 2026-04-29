# AGENTS.md

## Project

Windows 声明式环境管理器。单个 TOML 配置文件同步 Scoop 软件包和 Dotfiles。

## Build & Run

```bash
cargo build
cargo run -- sync
cargo run -- status
cargo run -- -c path/to/config.toml sync --dotfiles-only --backup
```

没有单元测试。验证方式：`cargo build` 无警告 + E2E 测试（见下文）。

## E2E 测试

`temps/` 目录已 gitignore，用于端到端测试。

```bash
# 首次运行（会 copy 和 merge 各格式文件到 ~/Temps/e2e-test/）
cargo run -- -c temps/config.toml sync --dotfiles-only --backup

# 幂等性验证（应全部 "Already up to date"）
cargo run -- -c temps/config.toml sync --dotfiles-only --backup

# status 验证
cargo run -- -c temps/config.toml status
```

`temps/config.toml` 覆盖了所有行为：copy、merge json/yaml/toml/gitconfig。

## Source Paths

`source` 路径相对于配置文件所在目录（非当前工作目录）。`resolve_base_dir` 先 `canonicalize` config_path 再取 parent。

## Architecture

4 个源文件，职责清晰：

- `main.rs` — CLI 解析（clap）、命令分发、`resolve_base_dir`
- `config.rs` — 配置结构体（serde 反序列化）、验证
- `dotfiles.rs` — dotfile 同步逻辑（copy、merge、冲突处理）
- `scoop.rs` — Scoop 同步（bucket/package 管理）

## Merge Behavior

`behavior = "merge"` 必须指定 `format`（json/yaml/gitconfig/toml）。

合并规则：对象/Table/Mapping 递归合并，数组和标量由源覆盖目标。

gitconfig merge 使用 `gix-config` 0.52（注意 API：`sections_by_name_and_filter` 返回 `Option<Iterator>`，`set_raw_value_by` 接受 owned 数据）。

## Conflict Resolution

copy 行为目标已存在且内容不同时：
- `--delete` 直接删除
- `--backup` 备份为 `.bak`
- 无参数则交互式提示
- `--delete` 和 `--backup` 互斥

merge 行为不触发冲突提示，直接写入合并结果。

## Conventions

- Commit: conventional commits（`feat:` / `fix:` / `refactor:` / `chore:`）
- 仅 Windows，Scoop 缺失时自动安装
- 无 symlink 支持（已移除 link 行为）
- `config.toml` 和 `dotfiles/` 已 gitignore（用户个人配置）

## Key Dependencies

- `gix-config` 0.52 — git config 解析/合并
- `serde_json` / `serde_yaml` 0.9 / `toml` 0.8 — 格式合并
- `clap` 4 — CLI
- `dirs` 5 — 用户主目录
- `colored` 2 — 终端输出
