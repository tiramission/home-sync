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
- 🔗 **Dotfile 符号链接** — 将仓库文件映射到 Windows 目标路径，冲突时自动备份
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

1. 解析 `~` 为用户主目录
2. 自动创建目标路径的父目录
3. 目标已是正确的符号链接 → 跳过
4. 目标已存在但不同 → 备份为 `.bak` 后创建新链接
5. 目标不存在 → 直接创建符号链接

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
[[dotfiles]]
source = "dotfiles/.gitconfig"              # 仓库中的路径（相对于项目根目录）
target = "~/.gitconfig"                     # Windows 目标路径（支持 ~）
```

完整示例参见 [`config.example.toml`](config.example.toml)。

## 环境要求

- Windows 10/11
- [Scoop](https://scoop.sh/)（缺失时自动安装）
- 开启开发者模式（用于无需管理员权限创建符号链接）

## License

MIT