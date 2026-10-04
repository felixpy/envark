use tauri::{
    AppHandle, Runtime,
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
};

pub fn install<R: Runtime>(app: &AppHandle<R>, language: &str) -> tauri::Result<()> {
    let label = |en: &'static str, zh: &'static str, tw: &'static str| match language {
        "zh-CN" => zh,
        "zh-TW" => tw,
        _ => en,
    };
    let item = |id, text, shortcut| MenuItem::with_id(app, id, text, true, shortcut);
    let add = item(
        "add-root",
        label("Add folder…", "添加目录…", "新增目錄…"),
        Some("CmdOrCtrl+O"),
    )?;
    let refresh = item(
        "refresh",
        label("Rescan", "重新扫描", "重新掃描"),
        Some("CmdOrCtrl+R"),
    )?;
    let settings = item(
        "settings",
        label("Settings…", "设置…", "設定…"),
        Some("CmdOrCtrl+,"),
    )?;
    let about = item(
        "about",
        label("About Envark", "关于 Envark", "關於 Envark"),
        None::<&str>,
    )?;
    let close =
        PredefinedMenuItem::close_window(app, Some(label("Close window", "关闭窗口", "關閉視窗")))?;
    let quit = PredefinedMenuItem::quit(
        app,
        Some(label("Quit Envark", "退出 Envark", "結束 Envark")),
    )?;
    let separator = || PredefinedMenuItem::separator(app);
    let file = Submenu::with_items(app, label("File", "文件", "檔案"), true, &[&add, &refresh])?;
    #[cfg(not(target_os = "macos"))]
    file.append_items(&[&separator()?, &settings])?;
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
    for (index, (id, text)) in [
        ("overview", label("Overview", "概览", "概覽")),
        (
            "env",
            label("Environments & tools", "环境与工具", "環境與工具"),
        ),
        ("projects", label("Project space", "项目空间", "專案空間")),
        ("worktrees", "Worktrees"),
        ("caches", label("Global caches", "全局缓存", "全域快取")),
        ("activity", label("Activity", "操作记录", "操作記錄")),
    ]
    .iter()
    .enumerate()
    {
        view.append(&MenuItem::with_id(
            app,
            *id,
            *text,
            true,
            Some(format!("CmdOrCtrl+{}", index + 1)),
        )?)?;
    }
    let help = Submenu::with_items(
        app,
        label("Help", "帮助", "說明"),
        true,
        &[&item(
            "help",
            label(
                "Getting started & shortcuts",
                "使用说明与快捷键",
                "使用說明與快捷鍵",
            ),
            Some("F1"),
        )?],
    )?;
    #[cfg(not(target_os = "macos"))]
    help.append(&about)?;
    let menu = Menu::new(app)?;
    #[cfg(target_os = "macos")]
    menu.append(&Submenu::with_items(
        app,
        "Envark",
        true,
        &[
            &about,
            &separator()?,
            &settings,
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
