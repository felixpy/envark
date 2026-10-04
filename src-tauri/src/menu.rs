use envark_core::model::{Settings, Shortcut};
use serde::Deserialize;
use std::{collections::BTreeSet, sync::Mutex};
use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu},
};

const SHORTCUTS: &[(Shortcut, &str, &str)] = &[
    (Shortcut::AddRoot, "add-root", "CmdOrCtrl+O"),
    (Shortcut::Refresh, "refresh", "CmdOrCtrl+R"),
    (Shortcut::Settings, "settings", "CmdOrCtrl+,"),
    (Shortcut::Overview, "overview", "CmdOrCtrl+1"),
    (Shortcut::Env, "env", "CmdOrCtrl+2"),
    (Shortcut::Projects, "projects", "CmdOrCtrl+3"),
    (Shortcut::Caches, "caches", "CmdOrCtrl+4"),
    (Shortcut::Activity, "activity", "CmdOrCtrl+5"),
    (Shortcut::ToggleSidebar, "toggle-sidebar", "CmdOrCtrl+B"),
    (Shortcut::ZoomIn, "zoom-in", "CmdOrCtrl+Plus"),
    (Shortcut::ZoomOut, "zoom-out", "CmdOrCtrl+-"),
    (Shortcut::ZoomReset, "zoom-reset", "CmdOrCtrl+0"),
    (Shortcut::Shortcuts, "shortcuts", "F1"),
];

fn accelerator(id: &str, disabled: &BTreeSet<Shortcut>) -> Option<&'static str> {
    SHORTCUTS.iter().find_map(|(shortcut, action, binding)| {
        (*action == id && !disabled.contains(shortcut)).then_some(*binding)
    })
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

impl Theme {
    pub fn native(self) -> Option<tauri::Theme> {
        match self {
            Self::Light => Some(tauri::Theme::Light),
            Self::Dark => Some(tauri::Theme::Dark),
            Self::System => None,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
pub struct ViewState {
    pub sidebar: bool,
    pub zoom: f64,
    pub theme: Theme,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            sidebar: true,
            zoom: 1.0,
            theme: Theme::System,
        }
    }
}

#[derive(Default)]
pub struct MenuState(pub Mutex<ViewState>);

fn find<R: Runtime>(items: Vec<MenuItemKind<R>>, id: &str) -> Option<MenuItemKind<R>> {
    for item in items {
        if item.id().as_ref() == id {
            return Some(item);
        }
        if let MenuItemKind::Submenu(submenu) = item
            && let Some(found) = find(submenu.items().ok()?, id)
        {
            return Some(found);
        }
    }
    None
}

pub fn sync<R: Runtime>(app: &AppHandle<R>, state: ViewState) -> tauri::Result<()> {
    let Some(menu) = app.menu() else {
        return Ok(());
    };
    for (id, checked) in [
        ("toggle-sidebar", state.sidebar),
        ("theme-light", state.theme == Theme::Light),
        ("theme-dark", state.theme == Theme::Dark),
        ("theme-system", state.theme == Theme::System),
    ] {
        if let Some(MenuItemKind::Check(item)) = find(menu.items()?, id) {
            item.set_checked(checked)?;
        }
    }
    for (id, enabled) in [
        ("zoom-in", state.zoom < 1.5),
        ("zoom-out", state.zoom > 0.8),
    ] {
        if let Some(MenuItemKind::MenuItem(item)) = find(menu.items()?, id) {
            item.set_enabled(enabled)?;
        }
    }
    Ok(())
}

pub fn sync_shortcuts<R: Runtime>(
    app: &AppHandle<R>,
    disabled: &BTreeSet<Shortcut>,
) -> tauri::Result<()> {
    let Some(menu) = app.menu() else {
        return Ok(());
    };
    for (_, id, _) in SHORTCUTS {
        let binding = accelerator(id, disabled);
        match find(menu.items()?, id) {
            Some(MenuItemKind::MenuItem(item)) => item.set_accelerator(binding)?,
            Some(MenuItemKind::Check(item)) => item.set_accelerator(binding)?,
            _ => {}
        }
    }
    Ok(())
}

pub fn install<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> tauri::Result<()> {
    let label =
        |en: &'static str, zh: &'static str, tw: &'static str| match settings.language.as_str() {
            "zh-CN" => zh,
            "zh-TW" => tw,
            _ => en,
        };
    let item = |id, text| {
        MenuItem::with_id(
            app,
            id,
            text,
            true,
            accelerator(id, &settings.disabled_shortcuts),
        )
    };
    let add = item("add-root", label("Add folder…", "添加目录…", "新增目錄…"))?;
    let refresh = item("refresh", label("Rescan", "重新扫描", "重新掃描"))?;
    let settings_item = item("settings", label("Settings…", "设置…", "設定…"))?;
    let about = item("about", label("About Envark", "关于 Envark", "關於 Envark"))?;
    let close =
        PredefinedMenuItem::close_window(app, Some(label("Close window", "关闭窗口", "關閉視窗")))?;
    let quit = PredefinedMenuItem::quit(
        app,
        Some(label("Quit Envark", "退出 Envark", "結束 Envark")),
    )?;
    let separator = || PredefinedMenuItem::separator(app);
    let file = Submenu::with_items(app, label("File", "文件", "檔案"), true, &[&add, &refresh])?;
    #[cfg(not(target_os = "macos"))]
    file.append_items(&[&separator()?, &settings_item])?;
    file.append_items(&[&separator()?, &close])?;
    #[cfg(not(target_os = "macos"))]
    file.append(&quit)?;
    let edit = Submenu::with_items(
        app,
        label("Edit", "编辑", "編輯"),
        true,
        &[
            &PredefinedMenuItem::cut(app, Some(label("Cut", "剪切", "剪下")))?,
            &PredefinedMenuItem::copy(app, Some(label("Copy", "复制", "複製")))?,
            &PredefinedMenuItem::paste(app, Some(label("Paste", "粘贴", "貼上")))?,
            &separator()?,
            &PredefinedMenuItem::select_all(
                app,
                Some(label("Select all text", "全选文本", "全選文字")),
            )?,
        ],
    )?;
    let view = Submenu::new(app, label("View", "视图", "檢視"), true)?;
    for (id, text) in [
        ("overview", label("Overview", "概览", "概覽")),
        (
            "env",
            label("Environments & tools", "环境与工具", "環境與工具"),
        ),
        ("projects", label("Project space", "项目空间", "專案空間")),
        ("caches", label("Global caches", "全局缓存", "全域快取")),
        ("activity", label("Activity", "操作记录", "操作記錄")),
    ] {
        view.append(&item(id, text)?)?;
    }
    view.append(&separator()?)?;
    view.append(&CheckMenuItem::with_id(
        app,
        "toggle-sidebar",
        label("Show sidebar", "显示侧边栏", "顯示側邊欄"),
        true,
        true,
        accelerator("toggle-sidebar", &settings.disabled_shortcuts),
    )?)?;
    let theme = Submenu::new(app, label("Theme mode", "主题模式", "主題模式"), true)?;
    for (id, text, checked) in [
        ("theme-light", label("Light", "浅色", "淺色"), false),
        ("theme-dark", label("Dark", "深色", "深色"), false),
        (
            "theme-system",
            label("System", "跟随系统", "跟隨系統"),
            true,
        ),
    ] {
        theme.append(&CheckMenuItem::with_id(
            app,
            id,
            text,
            true,
            checked,
            None::<&str>,
        )?)?;
    }
    view.append(&theme)?;
    view.append(&separator()?)?;
    for (id, text) in [
        ("zoom-in", label("Zoom in", "放大", "放大")),
        ("zoom-out", label("Zoom out", "缩小", "縮小")),
        (
            "zoom-reset",
            label("Actual size (100%)", "实际大小 (100%)", "實際大小 (100%)"),
        ),
    ] {
        view.append(&item(id, text)?)?;
    }
    let help = Submenu::with_items(
        app,
        label("Help", "帮助", "說明"),
        true,
        &[&item(
            "shortcuts",
            label("Keyboard shortcuts", "键盘快捷键", "鍵盤快捷鍵"),
        )?],
    )?;
    help.append_items(&[
        &item(
            "check-update",
            label("Check for updates…", "检查更新…", "檢查更新…"),
        )?,
        &item("releases", label("Release notes", "发布说明", "發佈說明"))?,
        &item(
            "issues",
            label("Report an issue…", "报告问题…", "回報問題…"),
        )?,
    ])?;
    #[cfg(not(target_os = "macos"))]
    help.append_items(&[&separator()?, &about])?;
    let menu = Menu::new(app)?;
    #[cfg(target_os = "macos")]
    menu.append(&Submenu::with_items(
        app,
        "Envark",
        true,
        &[
            &about,
            &separator()?,
            &settings_item,
            &separator()?,
            &PredefinedMenuItem::services(app, None)?,
            &separator()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &separator()?,
            &quit,
        ],
    )?)?;
    menu.append_items(&[&file, &edit, &view, &help])?;
    app.set_menu(menu)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabling_one_accelerator_preserves_other_bindings_and_restores_defaults() {
        for (disabled, disabled_id, _) in SHORTCUTS {
            let preferences = BTreeSet::from([*disabled]);
            for (shortcut, id, binding) in SHORTCUTS {
                assert_eq!(serde_json::to_value(shortcut).unwrap(), *id);
                assert_eq!(
                    accelerator(id, &preferences),
                    if id == disabled_id {
                        None
                    } else {
                        Some(*binding)
                    }
                );
                assert_eq!(accelerator(id, &BTreeSet::new()), Some(*binding));
            }
        }
        assert_eq!(accelerator("about", &BTreeSet::new()), None);
    }
}
