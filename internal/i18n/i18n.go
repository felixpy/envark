// Package i18n holds the short interface catalogs. English is the fallback
// when a Simplified Chinese string is missing.
package i18n

import "strings"

// Resolve picks en or zh. An explicit en or zh preference wins. system
// follows the locale strings, and anything else is English.
func Resolve(pref string, locales ...string) string {
	switch pref {
	case "en":
		return "en"
	case "zh", "zh-CN", "zh-Hans", "zh-TW", "zh-Hant":
		return "zh"
	}
	for _, locale := range locales {
		locale = strings.ToLower(strings.TrimSpace(locale))
		if i := strings.IndexByte(locale, '.'); i >= 0 {
			locale = locale[:i]
		}
		locale = strings.ReplaceAll(locale, "-", "_")
		if strings.HasPrefix(locale, "zh") {
			return "zh"
		}
	}
	return "en"
}

// T returns the Simplified Chinese text for an English source string.
func T(lang, english string) string {
	if lang == "zh" {
		if text, ok := zh[english]; ok && text != "" {
			return text
		}
	}
	return english
}

var zh = map[string]string{
	"Space":             "空间",
	"Environments":      "环境",
	"Review":            "核对",
	"History":           "历史",
	"Settings":          "设置",
	"Keyboard":          "键盘",
	"Runtimes and disk": "运行时与磁盘",
	"File lengths and allocated blocks, for the folder you chose.":                                     "你选定的文件夹：文件长度与占用块。",
	"Choose a folder to scan":                                                                          "选择要扫描的文件夹",
	"Envark measures only the folders you add. It does not walk your home directory until you say so.": "Envark 只测量你添加的文件夹。在你指定之前，不会遍历家目录。",
	"Add folder":                     "添加文件夹",
	"Scan":                           "扫描",
	"Cancel scan":                    "取消扫描",
	"Filter by name":                 "按名称过滤",
	"All":                            "全部",
	"Reclaimable":                    "可回收",
	"Name":                           "名称",
	"Kind":                           "类型",
	"Logical":                        "逻辑大小",
	"Allocated":                      "占用大小",
	"Risk":                           "风险",
	"Details":                        "详情",
	"Select a row.":                  "选择一行。",
	"Nothing in this filter.":        "这个过滤条件下没有内容。",
	"Largest first, by logical size": "按逻辑大小，从大到小",
	"Mark for review":                "加入核对",
	"Remove mark":                    "取消标记",
	"Some paths could not be read. Sizes are incomplete.": "有些路径无法读取。大小不完整。",
	"This subtree is incomplete.":                         "这个子树不完整。",
	"Scanning.":                                           "正在扫描。",
	"Low":                                                 "低",
	"Caution":                                             "注意",
	"None":                                                "无",
	"Directory":                                           "目录",
	"File":                                                "文件",
	"Link":                                                "链接",
	"Other volume":                                        "其他卷",
	"Node dependencies":                                   "Node 依赖",
	"Python cache":                                        "Python 缓存",
	"Cargo output":                                        "Cargo 输出",
	"Framework cache":                                     "框架缓存",
	"Build output":                                        "构建输出",
	"Virtualenv":                                          "虚拟环境",
	"Gradle project cache":                                "Gradle 项目缓存",
	"Vendored modules":                                    "Vendor 模块",
	"Individual files are shown so large ones can be found. Envark does not delete them from Space.":                   "显示单个文件，是为了找到大文件。Envark 不会从空间里删除它们。",
	"Symlinks are not followed and are not removed.":                                                                   "不会跟随符号链接，也不会删除它们。",
	"Not a recognized build output or cache.":                                                                          "不是已识别的构建输出或缓存。",
	"Dependencies beside package.json. Trash can restore them; the package manager can recreate them.":                 "package.json 旁边的依赖。废纸篓可以恢复；包管理器可以重新创建。",
	"Build output beside Cargo.toml. Trash can restore it; cargo build can recreate it.":                               "Cargo.toml 旁边的构建输出。废纸篓可以恢复；cargo build 可以重新创建。",
	"This is already fnm's default. Envark will not remove it or set it again.":                                        "这已经是 fnm 的默认版本。Envark 不会卸载它，也不会再次设置。",
	"This is already uv's default. Envark will not remove it.":                                                         "这已经是 uv 的默认版本。Envark 不会卸载它。",
	"Showing the newest Node.js versions fnm list-remote returned. The full list is longer.":                           "显示 fnm list-remote 返回的较新 Node.js 版本。完整列表更长。",
	"Showing 12 Python versions uv can download. The full list is longer.":                                             "显示 uv 可以下载的 12 个 Python 版本。完整列表更长。",
	"Installed versions, and the manager that owns them. Install and default changes are offered for fnm and uv only.": "已安装的版本，以及谁在管理它们。安装和默认版本只提供给 fnm 和 uv。",
	"Refresh":                  "刷新",
	"Measure locations":        "测量位置",
	"Check available versions": "检查可用版本",
	"Cancel":                   "取消",
	"Working…":                 "正在处理…",
	"No probe yet. Refresh reads the versions on this PATH.": "还没有探测。刷新会读取这个 PATH 上的版本。",
	"Known locations": "已知位置",
	"Refresh also looks up cache locations. A missing tool stays missing; it is not shown as an empty cache.": "刷新也会查找缓存位置。缺少的工具会保持缺少，不会显示成空缓存。",
	"Ecosystem":          "生态",
	"Version":            "版本",
	"Scope":              "范围",
	"Manager":            "管理器",
	"Ownership":          "归属",
	"Action":             "操作",
	"default":            "默认",
	"installed":          "已安装",
	"missing":            "不在 PATH",
	"available":          "可安装",
	"Location":           "位置",
	"Status":             "状态",
	"Not present":        "不存在",
	"Add to review":      "加入核对",
	"Review removal":     "核对卸载",
	"Review install":     "核对安装",
	"Make default":       "设为默认",
	"Available versions": "可用版本",
	"Check available versions asks fnm and uv. It does not install anything.":                                                                "检查可用版本会询问 fnm 和 uv。它不会安装任何东西。",
	"Only fnm and uv can install or change a default here. nvm, mise, asdf, volta, sdkman, Homebrew, rustup, Go, and Java stay unsupported.": "这里只有 fnm 和 uv 可以安装或更改默认版本。nvm、mise、asdf、volta、sdkman、Homebrew、rustup、Go 和 Java 保持不支持。",
	"Envark runs the manager in a separate process. That does not change the terminal that is already open.":                                 "Envark 在单独的进程里运行管理器。这不会改变已经打开的终端。",
	"Project pin": "项目钉选",
	"uv python pin writes .python-version in a directory you choose. It does not change an open terminal.": "uv python pin 会在你选择的目录写入 .python-version。它不会改变已经打开的终端。",
	"Project directory": "项目目录",
	"Python version":    "Python 版本",
	"Review pin":        "核对钉选",
	"Nothing is selected until you mark it. Nested directories count once. Manager commands do not add a byte total.": "在你标记之前，什么都不会被选中。嵌套目录只计一次。管理器命令不计入字节数。",
	"Nothing is marked. Mark a reclaimable directory in Space, or add a command from Environments.":                   "还没有标记。在空间里标记可回收目录，或从环境加入一条命令。",
	"Manager commands":   "管理器命令",
	"Move to Trash":      "移到废纸篓",
	"Delete permanently": "永久删除",
	"These commands cannot be restored from Trash.":                                 "这些命令无法从废纸篓恢复。",
	"Run manager commands":                                                          "运行管理器命令",
	"Trash can restore this directory.":                                             "废纸篓可以恢复这个目录。",
	"Each line is one item. A failed or cancelled item is not reported as removed.": "每一行是一项。失败或取消的项不会记成已删除。",
	"No operations yet.":                                                            "还没有操作。",
	"Restore":                                                                       "还原",
	"Scan roots, excludes, protected projects, and language are stored in this app's data directory. The previous settings.json file is left as it was.": "扫描根、排除项、受保护项目和语言保存在这个应用的数据目录。原来的 settings.json 不会被改动。",
	"Scan roots":               "扫描根",
	"No folders yet.":          "还没有文件夹。",
	"Remove":                   "移除",
	"Excluded directory names": "排除的目录名",
	"Save excludes":            "保存排除项",
	"Protected projects":       "受保护项目",
	"Cleanup refuses these paths, their parents, and their contents.": "清理会拒绝这些路径、它们的上级，以及它们的内容。",
	"Absolute path":            "绝对路径",
	"Protect":                  "保护",
	"Move files to Trash":      "把文件移到废纸篓",
	"Allow permanent deletion": "允许永久删除",
	"Scan the first folder when Envark opens": "Envark 打开时扫描第一个文件夹",
	"Language": "语言",
	"system follows the operating system. en is English. zh is Simplified Chinese.": "system 跟随操作系统。en 是英文。zh 是简体中文。",
	"Appearance":                "外观",
	"Data directory":            "数据目录",
	"Ctrl/Cmd 1–5 switch pages": "Ctrl/Cmd 1–5 切换页面",
	"Ctrl/Cmd R scans, or refreshes Environments": "Ctrl/Cmd R 扫描，或刷新环境",
	"Ctrl/Cmd F shows Space":                      "Ctrl/Cmd F 显示空间",
	"Escape cancels a scan":                       "Escape 取消扫描",
	"Up and Down move in the table":               "上和下在表格中移动",
	"Left and Right collapse and expand a row":    "左和右折叠或展开一行",
	"Delete permanently?":                         "永久删除？",
	"These directories will be deleted. Trash will not have them. This cannot be undone.": "这些目录会被删除。废纸篓里不会有它们。此操作无法撤销。",
	"Delete permanently now": "立即永久删除",
	"Run manager commands?":  "运行管理器命令？",
	"Each command runs as a separate argument list. Envark will not report bytes freed.": "每条命令都是独立的参数列表。Envark 不会报告释放了多少字节。",
	"Run commands":   "运行命令",
	"Move to Trash?": "移到废纸篓？",
	"The directories below move to Trash. You can restore them from History while the trash path still exists.": "下面的目录会移到废纸篓。废纸篓路径还在时，可以从历史还原。",
	"Confirm move to Trash":          "确认移到废纸篓",
	"Back":                           "返回",
	"Node.js was not found on PATH.": "在 PATH 上没有找到 Node.js。",
	"Python was not found on PATH.":  "在 PATH 上没有找到 Python。",
	"Java was not found on PATH.":    "在 PATH 上没有找到 Java。",
	"Rust was not found on PATH.":    "在 PATH 上没有找到 Rust。",
	"Go was not found on PATH.":      "在 PATH 上没有找到 Go。",
	"This is the active or default Node.js. Envark will not remove it.":           "这是当前或默认的 Node.js。Envark 不会卸载它。",
	"This is the active or default Python. Envark will not remove it.":            "这是当前或默认的 Python。Envark 不会卸载它。",
	"Java removal is not offered. Use the manager or package that installed it.":  "不提供 Java 卸载。请使用安装它的管理器或软件包。",
	"Go removal is not offered. Remove it with the installer that placed GOROOT.": "不提供 Go 卸载。请用放置 GOROOT 的安装器移除。",
	"fnm and uv were not found. Envark will not invent versions.":                 "没有找到 fnm 和 uv。Envark 不会编造版本。",
	"fnm was not found. Node.js versions are not listed.":                         "没有找到 fnm。不会列出 Node.js 版本。",
	"uv was not found. Python versions are not listed.":                           "没有找到 uv。不会列出 Python 版本。",
	"This plan is stale. Refresh versions and review the command again.":          "这个计划已过期。请刷新版本并重新核对命令。",
	"Version check cancelled.":                                                    "版本检查已取消。",
	"The install runs in a separate process. It does not change PATH in the terminal that is already open. Open a new terminal, or run the manager's use command there.": "安装在单独的进程中运行。它不会改变已经打开的终端的 PATH。请打开新终端，或在那里运行管理器的 use 命令。",
	"This changes the version new shells start with. It does not change the terminal that is already open. In that terminal, run the use command, or open a new one.":    "这会改变新 shell 启动时的版本。它不会改变已经打开的终端。请在那个终端运行 use 命令，或打开一个新终端。",
	"This writes .python-version in the chosen directory. It does not change PATH in the terminal that is already open.":                                                 "这会在选定目录写入 .python-version。它不会改变已经打开的终端的 PATH。",
	"Up":                                    "向上",
	"Open":                                  "打开",
	"Reduce motion":                         "减少动效",
	"Renderer":                              "渲染",
	"Shader sample":                         "着色器样本",
	"Scope marks are state, not disk size.": "范围标记表示状态，不是磁盘大小。",
	"Map: GPU shader. Cell area is logical size.":                                                                            "地图：GPU 着色器。格子面积是逻辑大小。",
	"Map: shader interpreted on the CPU. Cell area is logical size. This process has no GPU renderer.":                       "地图：着色器在 CPU 上解释执行。格子面积是逻辑大小。这个进程没有 GPU 渲染器。",
	"Map: CPU rectangles. The same cells are drawn by the shader when a GPU renderer is present. Cell area is logical size.": "地图：CPU 矩形。有 GPU 渲染器时，同样的格子由着色器绘制。格子面积是逻辑大小。",
	"Map: not drawn yet.": "地图：还没有绘制。",
	"Area is logical size. A warmer cell has more reclaimable bytes inside it.":                                                                       "面积是逻辑大小。更暖的格子里，可回收的字节更多。",
	"A small map drawn by the shader, including on the CPU interpreter. The main map uses it when a GPU renderer is present or the area stays small.": "一小块由着色器绘制的地图，在 CPU 解释器上也会画。主地图在有 GPU 渲染器，或面积仍然较小时使用它。",
}
