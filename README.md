# home-sync

> Windows 声明式用户环境管理器 — 通过一个配置文件同步 Scoop 软件包和 Dotfiles。

## 为什么需要？

每次重装系统、换电脑或配置新工作站时，Windows 用户都要面对：

- 手动复制各种配置文件（`.gitconfig`、编辑器设置、终端配置等）
- 逐个回忆并安装几十个工具
- 没有"我的环境"的单一事实来源

**home-sync** 用一个声明式 TOML 配置文件解决这些问题。一条命令，恢复整个用户环境。

## 功能特性

- 📦 **Scoop 管理** — 声明 buckets 和 packages，自动安装缺失的、卸载多余的（需确认）
- 📄 **Dotfile 同步** — 复制或深度合并仓库文件到 Windows 目标路径，冲突时可选删除或备份
- ⚡ **批量对比** — 一次获取已安装列表，内存中对比，快速高效
- 🚀 **一条命令** — `home-sync sync` 完成所有同步
- 📋 **状态查看** — `home-sync status` 查看当前环境状态
- 🎯 **选择性同步** — `--scoop-only` 或 `--dotfiles-only` 仅同步指定部分
- 🪶 **轻量** — 单个二进制文件，无运行时依赖

## 快速开始

### 1. 安装

```bash
cargo install --path .
```

### 2. 初始化配置

```bash
home-sync init
```

从示例模板创建 `config.toml`。

### 3. 编辑 `config.toml`

```toml
[scoop]
buckets = [
    "main",
    "extras",
    { name = "my-bucket", source = "https://github.com/user/my-bucket" },
]

packages = [
    "git",
    "7zip",
    "ripgrep",
    "bat",
    "neovim",
    # 指定 bucket 来源
    { name = "zig", bucket = "main" },
]

[[dotfiles]]
source = "dotfiles/.gitconfig"
target = "~/.gitconfig"

[[dotfiles]]
source = "dotfiles/settings.json"
target = "~/AppData/Roaming/Code/User/settings.json"
```

### 4. 添加 Dotfiles

将实际配置文件放入 `dotfiles/` 目录：

```
home-sync/
├── config.toml
├── config.example.toml
├── dotfiles/
│   ├── .gitconfig
│   ├── settings.json
│   └── starship.toml
└── src/
```

### 5. 同步

```bash
home-sync sync
```

## 命令

| 命令 | 说明 |
|------|------|
| `home-sync init` | 从示例模板创建 `config.toml` |
| `home-sync sync` | 完整同步：Scoop 包 + Dotfiles |
| `home-sync sync --scoop-only` | 仅同步 Scoop |
| `home-sync sync --dotfiles-only` | 仅同步 Dotfiles |
| `home-sync sync --dry-run` | 预览模式，不实际执行 |
| `home-sync sync --delete` | 冲突时直接删除目标文件 |
| `home-sync sync --backup` | 冲突时备份目标文件为 `.bak` |
| `home-sync status` | 查看当前环境状态 |

### 全局选项

| 选项 | 说明 |
|------|------|
| `-c, --config <PATH>` | 配置文件路径（默认：`config.toml`） |

## 工作原理

### Scoop 同步

1. 检查 Scoop 是否已安装，未安装则通过官方 PowerShell 安装器自动安装
2. **Buckets 同步：** 添加缺失的 bucket，移除未声明的 bucket（需用户确认）
3. **Packages 同步：** 一次 `scoop list` 获取已安装列表，内存中对比：
   - 安装缺失的包
   - 卸载未声明的包（`scoop uninstall --purge`，需用户确认）

### Dotfile 同步

**`behavior = "copy"`（默认）— 文件复制**

1. 解析 `~` 为用户主目录
2. 自动创建目标路径的父目录
3. 目标已存在且内容相同 → 跳过
4. 目标已存在但内容不同 → 提示删除/备份/跳过（或通过 `--delete`/`--backup` 自动处理）
5. 目标不存在 → 直接复制

**`behavior = "merge"` — 深度合并（JSON/YAML）**

1. 解析 `~` 为用户主目录
2. 自动创建目标路径的父目录
3. 目标不存在 → 直接复制源文件
4. 目标已存在 → 解析源和目标，深度合并后写入
5. 合并规则：对象递归合并，数组和标量由源覆盖目标

## 配置格式

### Bucket 声明

```toml
buckets = [
    "main",                    # 简单名称
    "extras",
    { name = "my-bucket", source = "https://github.com/user/my-bucket" },  # 自定义源
]
```

### Package 声明

```toml
packages = [
    "git",                     # 简单名称（从默认 bucket 安装）
    "neovim",
    { name = "zig", bucket = "main" },       # 指定 bucket
    { name = "my-tool", bucket = "my-bucket" },
]
```

### Dotfile 声明

```toml
# type = "file"（默认，可省略）：target 为绝对路径，支持 ~ 展开
# behavior = "copy"（默认，可省略）：复制文件到目标路径
[[dotfiles]]
source = "dotfiles/.gitconfig"
target = "~/.gitconfig"

# 使用 merge 行为深度合并 JSON 文件（需要指定 format）
[[dotfiles]]
source = "dotfiles/settings-partial.json"
target = "~/AppData/Roaming/Code/User/settings.json"
behavior = "merge"
format = "json"

# type = "persist"：target 为相对于 ~/scoop/persist/ 的路径
[[dotfiles]]
source = "dotfiles/mihomo/config.yaml"
target = "mihomo/config.yaml"               # → ~/scoop/persist/mihomo/config.yaml
type = "persist"
```

`type` — 目标路径类型：
- `file`（默认）— `target` 为绝对路径，支持 `~` 展开
- `persist` — `target` 为相对于 `~/scoop/persist/` 的路径

`behavior` — 同步方式：
- `copy`（默认）— 复制文件到目标路径，内容不同时提示删除或备份
- `merge` — 深度合并文件内容（需要指定 `format`），对象递归合并，数组和标量由源覆盖

`format` — 合并格式（`behavior = "merge"` 时必填）：
- `json` — JSON 深度合并
- `yaml` — YAML 深度合并

完整示例参见 [`config.example.toml`](config.example.toml)。

## 环境要求

- Windows 10/11
- [Scoop](https://scoop.sh/)（缺失时自动安装）

## License

MIT