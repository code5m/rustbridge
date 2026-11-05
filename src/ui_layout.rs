//! RustBridge UI 2.0 workbench shell and navigation.
use super::{
    App, BottomTab, Mode, SidebarSection, cargo_runner, learning_path, lessons, parser,
    symbol_search,
};
use eframe::egui::{self, Color32};

impl App {
    pub(super) fn render_workbench(&mut self, ctx: &egui::Context) {
        let window_width = ctx.content_rect().width();
        // Code remains the primary surface. Dual code panes need substantially
        // more width than a one-pane lesson or editor.
        let compare = matches!(self.mode, Mode::Compare);
        let nav_threshold = if compare { 1100.0 } else { 760.0 };
        let guide_threshold = if compare { 1580.0 } else { 1180.0 };
        let show_left = self.show_left_sidebar && window_width >= nav_threshold;
        let show_right = self.show_right_guide
            && window_width >= guide_threshold
            && !matches!(self.mode, Mode::Learn);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("RustBridge");
                ui.separator();

                if ui.button("打开项目").clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    self.open_project(folder);
                }
                if ui.button("打开文件").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Rust", &["rs"])
                        .pick_file()
                {
                    self.open(path);
                }
                let save = ui.add_enabled(
                    self.file.is_some() && self.dirty && self.saving.is_none(),
                    egui::Button::new("保存"),
                );
                if save.clicked() {
                    self.save();
                }
                if ui
                    .add_enabled(
                        !self.navigation_back.is_empty(),
                        egui::Button::new("返回定义前 · Alt+←"),
                    )
                    .clicked()
                {
                    self.navigate_back();
                }
                if self.live_dirty_count() > 0
                    && (!self.dirty || self.live_dirty_count() > 1)
                    && ui.button("全部保存").clicked()
                {
                    self.save_all_documents();
                }
                if self.saving.is_some() {
                    ui.spinner();
                    ui.small("安全保存中");
                }

                ui.separator();
                for (mode, label) in [
                    (Mode::Java, "Java 阅读"),
                    (Mode::Rust, "Rust 编辑"),
                    (Mode::Compare, "双栏对照"),
                    (Mode::Learn, "学习"),
                ] {
                    if ui.selectable_label(self.mode == mode, label).clicked() {
                        self.mode = mode;
                        self.last_scroll = None;
                        self.jump_to = self.selected;
                    }
                }

                ui.separator();
                if ui
                    .selectable_label(self.show_left_sidebar, "导航")
                    .clicked()
                {
                    self.show_left_sidebar = !self.show_left_sidebar;
                }
                if ui.selectable_label(self.show_right_guide, "说明").clicked() {
                    self.show_right_guide = !self.show_right_guide;
                }
                if ui
                    .selectable_label(self.show_bottom_panel, "工具窗口")
                    .clicked()
                {
                    self.show_bottom_panel = !self.show_bottom_panel;
                }

                if self.dirty {
                    ui.colored_label(Color32::YELLOW, "当前文档未保存");
                } else if self.file.is_none() {
                    ui.weak("内置示例 · 只读");
                }
                if !self.documents.is_empty() {
                    ui.weak(format!(
                        "{} 个标签 · {} 个未保存",
                        self.documents.len(),
                        self.live_dirty_count()
                    ));
                }
                if self.recovery_scan_warning.is_some() || !self.orphan_recovery.is_empty() {
                    ui.colored_label(
                        Color32::YELLOW,
                        if self.recovery_scan_warning.is_some() {
                            "恢复记录需要核查"
                        } else {
                            "发现孤立的恢复记录"
                        },
                    );
                    if ui.button("查看恢复记录").clicked() {
                        self.show_left_sidebar = true;
                        self.sidebar_section = SidebarSection::Tools;
                    }
                }
                if self.external_change {
                    ui.colored_label(Color32::RED, "磁盘文件已在外部修改");
                    if ui.button("重新加载磁盘版本").clicked() {
                        self.reload_disk_copy();
                    }
                }
            });
            if window_width < guide_threshold && self.show_right_guide {
                ui.small("优先保留代码宽度：说明栏已自动隐藏；放大窗口或切换到单栏阅读可显示。");
            }
            if window_width < nav_threshold && self.show_left_sidebar {
                ui.small("当前为窄窗口，项目导航已自动收起以保护代码阅读区域。");
            }
        });

        if !self.documents.is_empty() {
            // Render lightweight tab metadata; do not clone full source buffers
            // or recovery journals on every egui frame.
            let tabs: Vec<_> = self
                .documents
                .iter()
                .map(|document| {
                    (
                        document.path.clone(),
                        if self.file.as_ref() == Some(&document.path) {
                            self.dirty
                        } else {
                            document.is_dirty()
                        },
                        document.external_change,
                        document.pending_recovery.is_some(),
                    )
                })
                .collect();
            egui::TopBottomPanel::top("document_tabs").show(ctx, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("document_tab_strip")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (path, dirty, external, recovery) in tabs {
                                let active = self.file.as_ref() == Some(&path);
                                let name = path.file_name().unwrap_or_default().to_string_lossy();
                                let label = format!(
                                    "{}{}{}{}",
                                    if dirty { "● " } else { "" },
                                    name,
                                    if external { " !" } else { "" },
                                    if recovery { " ↻" } else { "" }
                                );
                                let tooltip = format!(
                                    "{}{}",
                                    path.display(),
                                    if recovery {
                                        " · 存在未恢复的崩溃快照"
                                    } else {
                                        ""
                                    }
                                );
                                if ui
                                    .selectable_label(active, label)
                                    .on_hover_text(tooltip)
                                    .clicked()
                                {
                                    self.open(path.clone());
                                }
                                if ui
                                    .small_button("×")
                                    .on_hover_text("关闭此文档标签")
                                    .clicked()
                                {
                                    self.request_close_document(path.clone());
                                }
                                ui.separator();
                            }
                            if self.opening.is_some() {
                                ui.spinner();
                                ui.small("正在打开…");
                            }
                        });
                    });
            });
        }

        if show_left {
            egui::SidePanel::left("navigation")
                .resizable(true)
                .default_width(248.0)
                .min_width(210.0)
                .max_width(320.0)
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (section, label) in [
                            (SidebarSection::Project, "项目"),
                            (SidebarSection::Structure, "结构"),
                            (SidebarSection::Search, "搜索"),
                            (SidebarSection::Learn, "学习"),
                            (SidebarSection::Tools, "工具"),
                        ] {
                            if ui
                                .selectable_label(self.sidebar_section == section, label)
                                .clicked()
                            {
                                self.sidebar_section = section;
                            }
                        }
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("sidebar_section_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match self.sidebar_section {
                            SidebarSection::Project => self.project_sidebar(ui),
                            SidebarSection::Structure => self.structure_sidebar(ui),
                            SidebarSection::Search => self.search_sidebar(ui),
                            SidebarSection::Learn => self.learn_sidebar(ui),
                            SidebarSection::Tools => self.tools_sidebar(ui),
                        });
                });
        }

        if show_right {
            egui::SidePanel::right("explanation")
                .resizable(true)
                .default_width(300.0)
                .min_width(250.0)
                .max_width(380.0)
                .show(ctx, |ui| self.right_guide(ui));
        }

        if self.show_bottom_panel {
            egui::TopBottomPanel::bottom("tool_window")
                .resizable(true)
                .default_height(190.0)
                .min_height(110.0)
                .max_height(340.0)
                .show(ctx, |ui| self.bottom_tool_window(ui));
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.small(&self.status);
                ui.separator();
                ui.small(format!("{} 个 AST 精确链接", self.projection.anchors.len()));
                ui.separator();
                ui.small(format!("{} 行", self.editor_line_starts.len()));
                if let Some(path) = self.file.as_ref() {
                    ui.separator();
                    ui.small(path.display().to_string());
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong(
                    self.file
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "内置示例（只读）".into()),
                );
                ui.weak(match self.mode {
                    Mode::Java => " · Java 心智模型 · 只读",
                    Mode::Rust => " · 原始 Rust 源码",
                    Mode::Compare => " · AST 双向对应",
                    Mode::Learn => " · Java 8 → Rust 学习",
                });
            });
            ui.separator();
            self.body(ui);
        });
    }

    fn project_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("项目");
        let Some(root) = self.root.clone() else {
            ui.label("尚未打开项目。");
            ui.small("打开一个真实文件夹，浏览它的实际目录结构。");
            return;
        };

        ui.strong(root.file_name().unwrap_or_default().to_string_lossy());
        ui.small(root.display().to_string());
        ui.small(if self.cargo_project {
            "已发现 Cargo.toml"
        } else {
            "根目录没有 Cargo.toml"
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("刷新目录").clicked() {
                self.dir_index = super::file_tree::DirectoryIndex::new();
                self.directory_limits.clear();
                self.refresh_file_search();
            }
            ui.checkbox(&mut self.show_excluded, "显示 .git / target 等目录");
        });
        ui.separator();
        self.directory(ui, &root, 0);

        let recent: Vec<_> = self
            .recent_files
            .iter()
            .filter(|path| path.starts_with(&root))
            .cloned()
            .collect();
        if !recent.is_empty() {
            ui.separator();
            ui.strong("最近打开");
            for path in recent {
                let label = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                if ui
                    .selectable_label(self.file.as_ref() == Some(&path), label)
                    .clicked()
                {
                    self.open(path);
                }
            }
        }
    }

    fn structure_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("代码结构");
        ui.small("来自当前 Rust 文件的真实 AST 声明与概念；不是类型推断结果。");
        if self.projection_pending {
            ui.spinner();
            ui.label("正在更新结构索引…");
            return;
        }
        let count = self.projection.structures.len();
        if count == 0 {
            ui.label("当前文件暂无可识别的 Rust AST 声明。");
            ui.small("自由函数、导入、mod、struct、enum、trait、impl 与字段均可出现在结构视图。");
            return;
        }
        ui.small(format!(
            "{count} 个真实 AST 结构项 · 点击定位到两栏原位置。"
        ));
        let mut chosen = None;
        for item in self.projection.structures.iter().take(800) {
            ui.horizontal(|ui| {
                ui.add_space(f32::from(item.depth) * 11.0);
                ui.weak(format!("L{}", item.rust_line + 1));
                let name = format!("{} · {}", item.kind, item.title);
                if ui
                    .selectable_label(false, name)
                    .on_hover_text(item.explanation)
                    .clicked()
                {
                    chosen = Some(item.rust_byte);
                }
            });
        }
        if count > 800 {
            ui.small("当前只展示前 800 项；其余可通过文件内搜索访问。");
        }
        if let Some(byte) = chosen {
            self.navigate_structure(byte);
        }
    }

    fn search_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("搜索");
        let Some(root) = self.root.clone() else {
            ui.label("先打开项目，才能搜索其中的真实 Rust 文件和声明。");
            return;
        };

        ui.strong("文件名");
        let changed = ui
            .add(
                egui::TextEdit::singleline(&mut self.file_query)
                    .hint_text("搜索项目中的 .rs 文件…"),
            )
            .changed();
        let excluded_changed = ui
            .checkbox(&mut self.show_excluded, "包含 .git / target / node_modules")
            .changed();
        if changed || excluded_changed {
            self.refresh_file_search();
        }
        if self.file_search_pending {
            ui.spinner();
            ui.small("正在后台搜索真实文件…");
        } else if !self.file_query.trim().is_empty() {
            ui.small(format!(
                "找到 {} 个结果（最多 100 个）",
                self.search_results.len()
            ));
        }
        let results = self.search_results.clone();
        for path in results {
            let label = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            if ui
                .selectable_label(self.file.as_ref() == Some(&path), label)
                .clicked()
            {
                self.open(path);
            }
        }

        ui.separator();
        ui.strong("同名 AST 声明候选");
        ui.small("这是语法级同名候选，不代表编译器已解析出真实定义。");
        ui.add(
            egui::TextEdit::singleline(&mut self.symbol_query)
                .hint_text("输入类型、函数或字段名称…"),
        );
        if ui
            .add_enabled(!self.symbol_pending, egui::Button::new("搜索声明候选"))
            .clicked()
        {
            self.symbol_revision += 1;
            self.symbol_pending = true;
            self.symbol_hits.clear();
            self.symbol_worker
                .get_or_insert_with(symbol_search::SymbolSearch::new)
                .submit(
                    self.symbol_revision,
                    root.clone(),
                    self.symbol_query.clone(),
                );
        }
        if self.symbol_pending {
            ui.spinner();
            ui.small("正在搜索真实 Rust 声明…");
        }
        let hits: Vec<_> = self.symbol_hits.iter().take(80).cloned().collect();
        for hit in hits {
            let label = format!(
                "{} · {}:{} [{}]",
                hit.name,
                hit.path.file_name().unwrap_or_default().to_string_lossy(),
                hit.line + 1,
                hit.kind
            );
            if ui
                .selectable_label(false, label)
                .on_hover_text(hit.path.display().to_string())
                .clicked()
            {
                let span = parser::ByteSpan {
                    start: hit.byte_start,
                    end: hit.byte_end,
                };
                if self.file.as_ref() == Some(&hit.path) {
                    self.mode = Mode::Rust;
                    self.editor_jump = Some(span);
                } else {
                    self.symbol_target = Some((hit.path.clone(), span));
                    self.open(hit.path);
                }
            }
        }
        if !self.symbol_pending
            && !self.symbol_query.trim().is_empty()
            && self.symbol_hits.is_empty()
        {
            ui.small("没有找到同名 AST 声明候选。");
        }
    }

    fn learn_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("Java → Rust 六阶段学习");
        let completed = self.completed_lessons.iter().filter(|&&done| done).count();
        let practice_done = self.practice_passed.iter().filter(|&&done| done).count();
        ui.label(format!(
            "知识测验 {completed}/{} · 真实编译练习 {practice_done}/{}",
            lessons::LESSONS.len(),
            learning_path::EXERCISES.len()
        ));
        ui.small("测验答对不代表已经独立掌握；真实练习和重复错误单独统计。");
        ui.separator();
        for (stage_idx, stage) in learning_path::STAGES.iter().enumerate() {
            let done = stage
                .lesson_indices
                .iter()
                .filter(|&&index| self.completed_lessons.get(index) == Some(&true))
                .count();
            egui::CollapsingHeader::new(format!(
                "{} · {done}/{}",
                stage.name,
                stage.lesson_indices.len()
            ))
            .id_salt(("learning_stage", stage_idx))
            .default_open(stage_idx == learning_path::stage_for_lesson(self.lesson))
            .show(ui, |ui| {
                ui.small(stage.objective);
                for &index in stage.lesson_indices {
                    let lesson = &lessons::LESSONS[index];
                    let done = self.completed_lessons.get(index) == Some(&true);
                    let errors = self.quiz_mistakes.get(index).copied().unwrap_or(0);
                    let label = format!(
                        "{} {} → {}{}",
                        if done { "✓" } else { "○" },
                        lesson.java,
                        lesson.rust,
                        if errors > 0 {
                            format!(" · 答错 {errors}")
                        } else {
                            String::new()
                        }
                    );
                    if ui.selectable_label(self.lesson == index, label).clicked() {
                        self.lesson = index;
                        self.quiz_choice = None;
                        self.lesson_return = None;
                        self.mode = Mode::Learn;
                    }
                }
            });
        }
    }

    fn tools_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("开发工具");
        ui.small("高级能力按需启用；打开项目不会自动启动索引或执行代码。");
        ui.separator();

        ui.strong("rust-analyzer / 诊断");
        ui.label(&self.analyzer_status);
        if ui
            .add_enabled(
                self.analyzer_check.is_none(),
                egui::Button::new("检查 rust-analyzer 是否可用"),
            )
            .clicked()
        {
            self.check_analyzer();
        }
        ui.separator();
        ui.small(&self.analyzer_diagnostics_status);
        if let Some(task) = self.analyzer_diagnostics.as_ref() {
            if ui.button("取消诊断").clicked() {
                task.cancel();
            }
        } else if ui
            .add_enabled(
                self.cargo_project && self.root.is_some(),
                egui::Button::new("检查已保存文件的 Cargo 诊断"),
            )
            .clicked()
        {
            self.start_analyzer_diagnostics();
        }

        ui.separator();
        ui.strong("rust-analyzer LSP");
        ui.small("进入 Cargo 项目的 Rust 编辑器时自动启动；Hover、Completion、Definition 直接融入编辑器，不会自动应用工作区修改。");
        ui.horizontal_wrapped(|ui| {
            if self.lsp_task.is_none() {
                if ui
                    .add_enabled(self.cargo_project, egui::Button::new("启动 LSP"))
                    .clicked()
                {
                    self.start_lsp();
                }
            } else if ui.button("停止 LSP").clicked() {
                if let Some(task) = self.lsp_task.take() {
                    task.cancel();
                }
                self.lsp_ready = false;
                self.lsp_user_stopped = true;
                self.lsp_status =
                    "rust-analyzer LSP 已停止；本项目不会自动重启，直到你手动启动。".into();
            }
        });
        ui.small(&self.lsp_status);
        if let Some(warning) = self.recovery_scan_warning.as_deref() {
            ui.separator();
            ui.colored_label(egui::Color32::YELLOW, warning);
            if let Some(store) = self.state_store.as_ref() {
                ui.small(format!("恢复记录目录：{}", store.directory().display()));
            }
        }
        if !self.orphan_recovery.is_empty() {
            ui.separator();
            ui.strong(format!(
                "孤立恢复快照（{} 条，原始文件已删除或移动）",
                self.orphan_recovery.len(),
            ));
            ui.small("不会自动创建、覆盖或修改原始文件。请选择新的文件名导出后人工比对。");
            let total = self.orphan_recovery.len();
            let page_size = 20;
            self.orphan_recovery_offset = self
                .orphan_recovery_offset
                .min((total - 1) / page_size * page_size);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.orphan_recovery_offset > 0, egui::Button::new("上一页"))
                    .clicked()
                {
                    self.orphan_recovery_offset =
                        self.orphan_recovery_offset.saturating_sub(page_size);
                }
                ui.label(format!(
                    "第 {} / {} 页",
                    self.orphan_recovery_offset / page_size + 1,
                    total.div_ceil(page_size)
                ));
                if ui
                    .add_enabled(
                        self.orphan_recovery_offset + page_size < total,
                        egui::Button::new("下一页"),
                    )
                    .clicked()
                {
                    self.orphan_recovery_offset += page_size;
                }
            });
            if self.recovery_export_receiver.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("恢复文件正在后台导出");
                });
            }
            let entries: Vec<_> = self
                .orphan_recovery
                .iter()
                .skip(self.orphan_recovery_offset)
                .take(page_size)
                .map(|entry| entry.path.clone())
                .collect();
            for source in entries {
                ui.horizontal_wrapped(|ui| {
                    ui.label(source.display().to_string());
                    if ui
                        .add_enabled(
                            self.recovery_export_receiver.is_none(),
                            egui::Button::new("导出恢复副本"),
                        )
                        .clicked()
                    {
                        let name = source
                            .file_name()
                            .map(|name| format!("{}.recovery.rs", name.to_string_lossy()))
                            .unwrap_or_else(|| "recovered.rs".into());
                        if let Some(destination) =
                            rfd::FileDialog::new().set_file_name(&name).save_file()
                        {
                            self.start_recovery_export(source.clone(), destination);
                        }
                    }
                });
            }
        }
        if self.lsp_ready {
            ui.small("编辑器原生交互：鼠标悬停查看类型 · 输入时自动补全 · F12 跳转定义 · Ctrl+Space 手动补全。");
        }
    }

    fn bottom_tool_window(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (tab, label) in [
                (BottomTab::Problems, "问题"),
                (BottomTab::Build, "构建"),
                (BottomTab::Run, "运行"),
                (BottomTab::Output, "输出"),
            ] {
                if ui.selectable_label(self.bottom_tab == tab, label).clicked() {
                    self.bottom_tab = tab;
                }
            }
            if self.cargo_task.is_some()
                && ui.button("取消当前任务").clicked()
                && let Some(task) = self.cargo_task.as_ref()
            {
                task.cancel();
            }
        });
        ui.separator();

        match self.bottom_tab {
            BottomTab::Problems => {
                ui.small(&self.cargo_status);
                if self.cargo_diagnostics.is_empty() {
                    ui.label("当前没有 Cargo 诊断。构建或检查完成后，问题会显示在这里。");
                } else {
                    let diagnostics = self.cargo_diagnostics.clone();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for diagnostic in diagnostics {
                            let label = format!(
                                "{} · {}:{}:{} · {}",
                                diagnostic.level,
                                diagnostic
                                    .path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                                diagnostic.line,
                                diagnostic.column,
                                diagnostic.message
                            );
                            if ui
                                .selectable_label(false, label)
                                .on_hover_text(diagnostic.path.display().to_string())
                                .clicked()
                            {
                                self.jump_to_cargo_diagnostic(diagnostic.clone());
                            }
                            if let Some((code, advice)) =
                                diagnostic.code.as_deref().and_then(|code| {
                                    learning_path::explain_error(code).map(|advice| (code, advice))
                                })
                            {
                                ui.collapsing(format!("{} · {}", code, advice.plain), |ui| {
                                    ui.strong("Java 心智模型");
                                    ui.label(advice.java);
                                    ui.strong("Rust 原因");
                                    ui.label(advice.rust);
                                    ui.strong("正确修复方向");
                                    ui.label(advice.remedy);
                                    ui.small(
                                        "这是教学建议，仍需根据真实上下文编译验证，不能机械套用。",
                                    );
                                });
                            }
                        }
                    });
                }
            }
            BottomTab::Build => {
                ui.horizontal_wrapped(|ui| {
                    let enabled = self.cargo_project
                        && self.cargo_task.is_none()
                        && self.live_dirty_count() == 0
                        && self.saving.is_none();
                    if ui
                        .add_enabled(enabled, egui::Button::new("Cargo Check"))
                        .clicked()
                    {
                        self.cargo_confirm = Some(cargo_runner::Action::Check);
                    }
                    if ui
                        .add_enabled(enabled, egui::Button::new("Cargo Test"))
                        .clicked()
                    {
                        self.cargo_confirm = Some(cargo_runner::Action::Test);
                    }
                });
                ui.small("每次执行都需要明确授权；Cargo 可能运行 build.rs 或项目测试代码。");
                ui.small(&self.cargo_status);
            }
            BottomTab::Run => {
                let enabled = self.cargo_project
                    && self.cargo_task.is_none()
                    && self.live_dirty_count() == 0
                    && self.saving.is_none();
                if ui
                    .add_enabled(enabled, egui::Button::new("运行 Cargo 项目"))
                    .clicked()
                {
                    self.cargo_confirm = Some(cargo_runner::Action::Run);
                }
                ui.small("运行前必须信任该项目。打开项目本身不会运行任何代码。");
                ui.small(&self.cargo_status);
            }
            BottomTab::Output => {
                ui.small(&self.cargo_status);
                if self.cargo_log.is_empty() {
                    ui.label("尚无构建或运行输出。");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for line in self.cargo_log.iter().rev() {
                            ui.monospace(line);
                        }
                    });
                }
            }
        }
    }
}
