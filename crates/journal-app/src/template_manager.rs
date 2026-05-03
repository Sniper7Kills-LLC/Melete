use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    Align, ApplicationWindow, Box as GtkBox, Button, DrawingArea, FileDialog, FileFilter, Frame,
    Label, ListBox, ListBoxRow, Orientation, ScrolledWindow, Stack, StackSwitcher, Window,
};
use journal_canvas::{paint_with_widgets, ViewportTransform};
use journal_core::{
    BackgroundType, NotebookTemplate, PageTemplate, Point, Rect, TemplateId, TilingMode, Viewport,
};
use journal_templates::{
    is_builtin, is_builtin_notebook_template, serialize_template_toml,
    template_file_from_page_template,
};
use std::collections::HashSet;
use uuid::Uuid;

use crate::state::SharedState;

/// Callback type used to open the full-screen template editor.
/// `Some(template)` edits an existing template; `None` creates a new one.
pub type OpenEditorFn = Rc<dyn Fn(Option<PageTemplate>)>;

/// Callback type used to open the full-screen notebook template editor.
pub type OpenNbEditorFn = Rc<dyn Fn(Option<journal_core::NotebookTemplate>)>;

// Thread-local slot: set by `window::build_home_into` so that the template
// manager's notebook-template edit button routes through the stack-page editor
// rather than the modal fallback.
std::thread_local! {
    static NB_EDITOR_OPENER: std::cell::RefCell<Option<OpenNbEditorFn>> =
        std::cell::RefCell::new(None);
}

/// Register the stack-page notebook template editor opener.
/// Called once from `window::build_home_into`.
pub fn set_nb_editor_opener(opener: OpenNbEditorFn) {
    NB_EDITOR_OPENER.with(|cell| {
        *cell.borrow_mut() = Some(opener);
    });
}

/// Invoke the registered notebook template editor opener (if set), otherwise
/// fall back to the modal `prompt_notebook_template_editor`.
fn open_nb_editor(
    parent: &ApplicationWindow,
    state: SharedState,
    edit: Option<journal_core::NotebookTemplate>,
    list: Rc<ListBox>,
    parent_for_refresh: ApplicationWindow,
    close_manager: Rc<dyn Fn()>,
) {
    let has_opener = NB_EDITOR_OPENER.with(|cell| cell.borrow().is_some());
    if has_opener {
        // Close the template manager window, then open the stack-page editor.
        (close_manager)();
        NB_EDITOR_OPENER.with(|cell| {
            if let Some(ref opener) = *cell.borrow() {
                (opener)(edit);
            }
        });
    } else {
        // Fallback: modal editor (back-compat path when no stack-page opener is registered).
        let state_inner = state.clone();
        let list_inner = list.clone();
        let parent_inner = parent_for_refresh.clone();
        let close_inner = close_manager.clone();
        crate::dialogs::prompt_notebook_template_editor(
            parent,
            state,
            edit,
            Box::new(move |_id| {
                refresh_notebook_template_list(
                    &list_inner,
                    state_inner.clone(),
                    &parent_inner,
                    close_inner.clone(),
                );
            }),
        );
    }
}

pub fn open(parent: &ApplicationWindow, state: SharedState, open_editor: OpenEditorFn) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(false)
        .title("Templates")
        .default_width(720)
        .default_height(620)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let stack = Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    let switcher = StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_halign(Align::Center);
    body.append(&switcher);
    body.append(&stack);

    let close_manager: Rc<dyn Fn()> = {
        let win = win.clone();
        Rc::new(move || win.close())
    };

    let pages_tab = build_page_templates_tab(
        parent,
        state.clone(),
        open_editor.clone(),
        close_manager.clone(),
    );
    stack.add_titled(&pages_tab, Some("pages"), "Page Templates");

    let nb_tab = build_notebook_templates_tab(parent, state.clone(), close_manager.clone());
    stack.add_titled(&nb_tab, Some("notebooks"), "Notebook Templates");

    let close_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(Align::End)
        .build();
    let close = Button::with_label("Close");
    close.connect_clicked({
        let win = win.clone();
        move |_| win.close()
    });
    close_row.append(&close);
    body.append(&close_row);

    win.set_child(Some(&body));
    win.present();
}

fn build_page_templates_tab(
    parent: &ApplicationWindow,
    state: SharedState,
    open_editor: OpenEditorFn,
    close_manager: Rc<dyn Fn()>,
) -> GtkBox {
    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    let title = Label::builder()
        .label("Page Templates")
        .halign(Align::Start)
        .hexpand(true)
        .build();
    title.add_css_class("title-4");
    header.append(&title);

    let new_btn = Button::with_label("New template…");
    new_btn.add_css_class("suggested-action");
    header.append(&new_btn);

    let import_btn = Button::with_label("Import image…");
    header.append(&import_btn);

    let import_pdf_btn = Button::with_label("Import PDF…");
    header.append(&import_pdf_btn);
    root.append(&header);

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let list = ListBox::builder().build();
    list.add_css_class("template-list");
    scroller.set_child(Some(&list));
    root.append(&scroller);

    let list_rc = Rc::new(list);
    refresh_list(
        &list_rc,
        state.clone(),
        parent,
        open_editor.clone(),
        close_manager.clone(),
    );

    {
        let close_manager = close_manager.clone();
        let open_editor = open_editor.clone();
        new_btn.connect_clicked(move |_| {
            (open_editor)(None);
            (close_manager)();
        });
    }

    {
        let parent = parent.clone();
        let state = state.clone();
        let list = list_rc.clone();
        let open_editor = open_editor.clone();
        let close_manager = close_manager.clone();
        import_btn.connect_clicked(move |_| {
            run_import(
                &parent,
                state.clone(),
                list.clone(),
                open_editor.clone(),
                close_manager.clone(),
            );
        });
    }

    {
        let parent = parent.clone();
        let list = list_rc.clone();
        let open_editor = open_editor.clone();
        let close_manager = close_manager.clone();
        import_pdf_btn.connect_clicked(move |_| {
            run_pdf_import(
                &parent,
                state.clone(),
                list.clone(),
                open_editor.clone(),
                close_manager.clone(),
            );
        });
    }

    root
}

fn build_notebook_templates_tab(
    parent: &ApplicationWindow,
    state: SharedState,
    close_manager: Rc<dyn Fn()>,
) -> GtkBox {
    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    let title = Label::builder()
        .label("Notebook Templates")
        .halign(Align::Start)
        .hexpand(true)
        .build();
    title.add_css_class("title-4");
    header.append(&title);

    let new_btn = Button::with_label("New notebook template…");
    new_btn.add_css_class("suggested-action");
    header.append(&new_btn);
    root.append(&header);

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let list = ListBox::builder().build();
    scroller.set_child(Some(&list));
    root.append(&scroller);

    let list_rc = Rc::new(list);
    refresh_notebook_template_list(&list_rc, state.clone(), parent, close_manager.clone());

    {
        let parent = parent.clone();
        let state = state.clone();
        let list = list_rc.clone();
        let close_mgr = close_manager.clone();
        new_btn.connect_clicked(move |_| {
            let list_inner = list.clone();
            let parent_inner = parent.clone();
            let close_inner = close_mgr.clone();
            // Use the stack-page editor if registered, otherwise fall back to modal.
            open_nb_editor(
                &parent,
                state.clone(),
                None,
                list_inner,
                parent_inner,
                close_inner,
            );
        });
    }

    root
}

fn refresh_notebook_template_list(
    list: &Rc<ListBox>,
    state: SharedState,
    parent: &ApplicationWindow,
    close_manager: Rc<dyn Fn()>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let templates: Vec<NotebookTemplate> = {
        let s = state.borrow();
        let reg = s.notebook_templates.borrow();
        let mut v: Vec<NotebookTemplate> = reg.list().iter().map(|t| (*t).clone()).collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    };

    if templates.is_empty() {
        let empty = Label::builder()
            .label("No notebook templates yet — click \"New notebook template…\" above.")
            .halign(Align::Start)
            .build();
        empty.add_css_class("dim-label");
        list.append(&empty);
        return;
    }

    for t in templates {
        let row = build_notebook_template_row(&t, state.clone(), list.clone(), parent, close_manager.clone());
        list.append(&row);
    }
}

fn build_notebook_template_row(
    t: &NotebookTemplate,
    state: SharedState,
    list: Rc<ListBox>,
    parent: &ApplicationWindow,
    close_manager: Rc<dyn Fn()>,
) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(10)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    let icon = gtk4::Image::from_icon_name("x-office-calendar-symbolic");
    icon.set_pixel_size(28);
    icon.add_css_class("dim-label");
    row.append(&icon);

    let text_col = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .hexpand(true)
        .build();
    let name = Label::builder().label(&t.name).halign(Align::Start).build();
    name.add_css_class("title-4");
    text_col.append(&name);
    let desc = if t.description.is_empty() {
        format!(
            "Grouping: {:?} · {} daily slot(s)",
            t.grouping,
            t.daily_slots.len()
        )
    } else {
        t.description.clone()
    };
    let desc_lbl = Label::builder().label(&desc).halign(Align::Start).wrap(true).build();
    desc_lbl.add_css_class("dim-label");
    text_col.append(&desc_lbl);
    row.append(&text_col);

    if !is_builtin_notebook_template(t.id) {
        // Edit button — opens the stack-page editor (falling back to modal if
        // the opener hasn't been registered yet).
        let edit_btn = Button::from_icon_name("document-edit-symbolic");
        edit_btn.set_tooltip_text(Some("Edit template"));
        let template_for_edit = t.clone();
        let state_for_edit = state.clone();
        let list_for_edit = list.clone();
        let parent_for_edit = parent.clone();
        let close_for_edit = close_manager.clone();
        edit_btn.connect_clicked(move |_| {
            open_nb_editor(
                &parent_for_edit,
                state_for_edit.clone(),
                Some(template_for_edit.clone()),
                list_for_edit.clone(),
                parent_for_edit.clone(),
                close_for_edit.clone(),
            );
        });
        row.append(&edit_btn);

        let del = Button::from_icon_name("edit-delete-symbolic");
        del.set_tooltip_text(Some("Delete template"));
        del.add_css_class("destructive-action");
        let tid = t.id;
        let state_for_del = state.clone();
        let list_for_del = list.clone();
        let parent_for_del = parent.clone();
        let close_for_del = close_manager.clone();
        del.connect_clicked(move |_| {
            delete_notebook_template(tid, state_for_del.clone());
            refresh_notebook_template_list(
                &list_for_del,
                state_for_del.clone(),
                &parent_for_del,
                close_for_del.clone(),
            );
        });
        row.append(&del);
    } else {
        let badge = Label::builder().label("built-in").halign(Align::End).build();
        badge.add_css_class("dim-label");
        row.append(&badge);
    }

    row
}

fn delete_notebook_template(id: TemplateId, state: SharedState) {
    let s = state.borrow();
    let removed = {
        let mut reg = s.notebook_templates.borrow_mut();
        reg.remove(id)
    };
    if removed.is_none() {
        return;
    }
    if let Some(dir) = notebook_templates_dir() {
        let p = dir.join(format!("{}.toml", id.0));
        if p.exists() {
            if let Err(e) = std::fs::remove_file(&p) {
                tracing::warn!("remove notebook template file {:?}: {}", p, e);
            }
        }
    }
}

fn notebook_templates_dir() -> Option<PathBuf> {
    let base = dirs::data_dir().or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))?;
    Some(base.join("journal").join("notebook_templates"))
}

fn pdfs_dir() -> Option<PathBuf> {
    Some(templates_dir()?.join("pdfs"))
}

fn run_pdf_import(
    parent: &ApplicationWindow,
    state: SharedState,
    list: Rc<ListBox>,
    open_editor: OpenEditorFn,
    close_manager: Rc<dyn Fn()>,
) {
    let dialog = FileDialog::builder().title("Import PDF").modal(true).build();
    let filter = FileFilter::new();
    filter.set_name(Some("PDF"));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    let filters = gtk4::gio::ListStore::new::<FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));

    let parent_for_cb = parent.clone();
    dialog.open(Some(parent), None::<&gtk4::gio::Cancellable>, move |result| {
        let file = match result {
            Ok(f) => f,
            Err(e) => {
                if !e.matches(gtk4::gio::IOErrorEnum::Cancelled)
                    && !e.matches(gtk4::gio::IOErrorEnum::Failed)
                {
                    tracing::warn!("pdf dialog error: {}", e);
                }
                return;
            }
        };
        let src_path = match file.path() {
            Some(p) => p,
            None => { tracing::warn!("pdf file has no local path"); return; }
        };
        if let Err(e) = import_pdf(
            &parent_for_cb,
            &src_path,
            state.clone(),
            list.clone(),
            open_editor.clone(),
            close_manager.clone(),
        ) {
            tracing::error!("pdf import: {:#}", e);
            show_error(&parent_for_cb, &format!("Failed to import PDF: {}", e));
        }
    });
}

fn pdf_page_count(path: &std::path::Path) -> u32 {
    #[cfg(feature = "pdf")]
    {
        use poppler::Document;
        if let Ok(abs) = path.canonicalize() {
            let uri = format!("file://{}", abs.display());
            if let Ok(doc) = Document::from_file(&uri, None) {
                let n = doc.n_pages();
                if n > 0 {
                    return n as u32;
                }
            }
        }
    }
    1
}

fn import_pdf(
    parent: &ApplicationWindow,
    src: &std::path::Path,
    state: SharedState,
    list: Rc<ListBox>,
    open_editor: OpenEditorFn,
    close_manager: Rc<dyn Fn()>,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4();
    let pdf_dir = pdfs_dir().ok_or_else(|| anyhow::anyhow!("could not resolve data dir"))?;
    std::fs::create_dir_all(&pdf_dir)?;
    let dst = pdf_dir.join(format!("{}.pdf", id));
    std::fs::copy(src, &dst)?;

    let n_pages = pdf_page_count(&dst);
    let name = src.file_stem().and_then(|s| s.to_str()).unwrap_or("PDF").to_string();
    let dst_str = dst.to_string_lossy().to_string();

    if n_pages <= 1 {
        finalize_pdf_template(id, name, dst_str, 0, state.clone());
        refresh_list(&list, state, parent, open_editor, close_manager);
        return Ok(());
    }

    // Ask the user which page to use (1-based display, 0-based storage).
    show_pdf_page_picker(
        parent, id, name, dst_str, n_pages, state, list, open_editor, close_manager,
    );
    Ok(())
}

fn finalize_pdf_template(id: Uuid, name: String, dst: String, page: u32, state: SharedState) {
    let template = PageTemplate {
        id: journal_core::TemplateId(id),
        name: name.clone(),
        description: format!("PDF page {}", page + 1),
        background: BackgroundType::Pdf { path: dst.clone(), page },
        size_mm: (215.9, 279.4),
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Imported".into(),
    };

    let tdir = match templates_dir() {
        Some(d) => d,
        None => { tracing::error!("could not resolve data dir"); return; }
    };
    if let Err(e) = std::fs::create_dir_all(&tdir) {
        tracing::error!("create templates dir: {}", e);
        return;
    }
    let toml_path = tdir.join(format!("{}.toml", id));
    let file = template_file_from_page_template(&template);
    match serialize_template_toml(&file) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&toml_path, text) {
                tracing::error!("write template toml: {}", e);
                return;
            }
        }
        Err(e) => { tracing::error!("serialize template: {}", e); return; }
    }
    let s = state.borrow();
    s.templates.borrow_mut().insert(template);
}

fn show_pdf_page_picker(
    parent: &ApplicationWindow,
    id: Uuid,
    name: String,
    dst: String,
    n_pages: u32,
    state: SharedState,
    list: Rc<ListBox>,
    open_editor: OpenEditorFn,
    close_manager: Rc<dyn Fn()>,
) {
    let parent_for_refresh = parent.clone();
    use gtk4::{Adjustment, Align, SpinButton, Window};

    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Choose PDF page")
        .default_width(280)
        .build();

    let body = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();

    let lbl = Label::new(Some(&format!("This PDF has {} pages.\nWhich page to use as background?", n_pages)));
    lbl.set_halign(Align::Start);
    body.append(&lbl);

    let adj = Adjustment::new(1.0, 1.0, n_pages as f64, 1.0, 10.0, 0.0);
    let spin = SpinButton::new(Some(&adj), 1.0, 0);
    spin.set_numeric(true);
    body.append(&spin);

    let btn_row = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .halign(Align::End)
        .build();

    let cancel_btn = Button::with_label("Cancel");
    let ok_btn = Button::with_label("OK");
    ok_btn.add_css_class("suggested-action");
    btn_row.append(&cancel_btn);
    btn_row.append(&ok_btn);
    body.append(&btn_row);

    win.set_child(Some(&body));

    {
        let win = win.clone();
        cancel_btn.connect_clicked(move |_| {
            win.close();
        });
    }
    {
        let win = win.clone();
        let spin = spin.clone();
        let open_editor = open_editor.clone();
        let close_manager = close_manager.clone();
        ok_btn.connect_clicked(move |_| {
            let page = (spin.value() as u32).saturating_sub(1);
            finalize_pdf_template(id, name.clone(), dst.clone(), page, state.clone());
            refresh_list(
                &list,
                state.clone(),
                &parent_for_refresh,
                open_editor.clone(),
                close_manager.clone(),
            );
            win.close();
        });
    }

    win.present();
}

fn refresh_list(
    list: &Rc<ListBox>,
    state: SharedState,
    parent: &ApplicationWindow,
    open_editor: OpenEditorFn,
    close_manager: Rc<dyn Fn()>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let mut templates: Vec<PageTemplate> = {
        let s = state.borrow();
        let reg = s.templates.borrow();
        reg.list().iter().map(|t| (*t).clone()).collect()
    };
    // Sort by category-priority first, then alphabetically within a
    // category. The ListBox header function then inserts visual dividers
    // when the category changes.
    templates.sort_by(|a, b| {
        let ca = canonical_category(&a.category);
        let cb = canonical_category(&b.category);
        category_priority(&ca)
            .cmp(&category_priority(&cb))
            .then_with(|| ca.to_lowercase().cmp(&cb.to_lowercase()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    for t in templates {
        let row_widget = build_row(
            &t,
            state.clone(),
            list.clone(),
            parent,
            open_editor.clone(),
            close_manager.clone(),
        );
        let row = ListBoxRow::new();
        row.set_child(Some(&row_widget));
        let cat = if t.category.is_empty() { "Uncategorized".to_string() } else { t.category.clone() };
        unsafe {
            row.set_data::<String>("template-category", cat);
        }
        list.append(&row);
    }

    // Header function: insert a visible category label above the first row of
    // each new category. Pulls the per-row category back out via the qdata
    // we attached above.
    list.set_header_func(|row: &ListBoxRow, before: Option<&ListBoxRow>| {
        let cur: String = match unsafe { row.data::<String>("template-category") } {
            Some(p) => unsafe { p.as_ref() }.clone(),
            None => return,
        };
        if let Some(prev) = before {
            if let Some(p) = unsafe { prev.data::<String>("template-category") } {
                let prev_cat: String = unsafe { p.as_ref() }.clone();
                if prev_cat == cur {
                    row.set_header(None::<&gtk4::Widget>);
                    return;
                }
            }
        }
        let header = Label::builder().label(&cur).halign(Align::Start).build();
        header.add_css_class("template-category-header");
        row.set_header(Some(&header));
    });
}

fn build_row(
    t: &PageTemplate,
    state: SharedState,
    list: Rc<ListBox>,
    parent: &ApplicationWindow,
    open_editor: OpenEditorFn,
    close_manager: Rc<dyn Fn()>,
) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(12)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();

    let preview = build_template_preview(t.clone(), state.borrow().dark_mode);
    row.append(&preview);

    let text_col = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .valign(Align::Center)
        .build();
    let name = Label::builder()
        .label(&t.name)
        .halign(Align::Start)
        .build();
    name.add_css_class("title-4");
    text_col.append(&name);
    let kind = Label::builder()
        .label(describe(&t.background))
        .halign(Align::Start)
        .build();
    kind.add_css_class("dim-label");
    text_col.append(&kind);
    row.append(&text_col);

    if !is_builtin(t.id) {
        let edit_btn = Button::from_icon_name("document-edit-symbolic");
        edit_btn.set_tooltip_text(Some("Edit template"));
        let template_for_edit = t.clone();
        let open_editor_btn = open_editor.clone();
        let close_manager_btn = close_manager.clone();
        edit_btn.connect_clicked(move |_| {
            (open_editor_btn)(Some(template_for_edit.clone()));
            (close_manager_btn)();
        });
        row.append(&edit_btn);

        let del = Button::from_icon_name("edit-delete-symbolic");
        del.set_tooltip_text(Some("Delete template"));
        del.add_css_class("destructive-action");
        let tid = t.id;
        let state_for_del = state.clone();
        let list_for_del = list.clone();
        let parent_for_del = parent.clone();
        let open_editor_del = open_editor.clone();
        let close_manager_del = close_manager.clone();
        del.connect_clicked(move |_| {
            delete_template(tid, state_for_del.clone());
            refresh_list(
                &list_for_del,
                state_for_del.clone(),
                &parent_for_del,
                open_editor_del.clone(),
                close_manager_del.clone(),
            );
        });
        row.append(&del);
    } else {
        let badge = Label::builder()
            .label("built-in")
            .halign(gtk4::Align::End)
            .build();
        badge.add_css_class("dim-label");
        row.append(&badge);
    }

    row
}

/// Render a small thumbnail of the template (background + widgets) inside a
/// 64×84 DrawingArea wrapped in a `Frame` for visual separation.
fn build_template_preview(template: PageTemplate, dark_mode: bool) -> Frame {
    const PREVIEW_W: i32 = 64;
    const PREVIEW_H: i32 = 84;

    let area = DrawingArea::builder()
        .width_request(PREVIEW_W)
        .height_request(PREVIEW_H)
        .build();

    area.set_draw_func(move |_a, ctx, w, h| {
        if w <= 0 || h <= 0 {
            return;
        }
        let page_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: template.size_mm.0,
            height: template.size_mm.1,
        };
        let margin = 0.95;
        let zoom_x = w as f64 / page_rect.width;
        let zoom_y = h as f64 / page_rect.height;
        let zoom = zoom_x.min(zoom_y) * margin;

        let viewport = Viewport {
            center: Point {
                x: page_rect.x + page_rect.width * 0.5,
                y: page_rect.y + page_rect.height * 0.5,
            },
            zoom,
            rotation: 0.0,
        };
        let transform = ViewportTransform::new(viewport, w as f64, h as f64);
        let bg = journal_templates::page_template_to_background_config(&template);
        let empty: HashSet<Uuid> = HashSet::new();
        paint_with_widgets(
            ctx, &transform, &bg, page_rect,
            &template.widgets, &[], &empty, dark_mode,
        );
    });

    let frame = Frame::builder().build();
    frame.add_css_class("template-preview-frame");
    frame.set_child(Some(&area));
    frame
}

/// Map an empty category to the literal "Uncategorized" so it groups with
/// itself instead of being its own empty bucket.
fn canonical_category(s: &str) -> String {
    if s.trim().is_empty() {
        "Uncategorized".to_string()
    } else {
        s.trim().to_string()
    }
}

/// Sort rank for known categories. Lower value = appears earlier in the
/// list. Anything not in this table falls into the "user-defined" bucket
/// (priority 100) and sorts alphabetically among its peers.
///
///   0   Basics                — foundational page templates
///   10  Daily Planner         — built-in planner spreads
///   20–99 reserved for additional well-known categories
///   100 user-defined / custom — sorted alphabetically among themselves
///   200 Imported              — image / PDF imports
///   300 Uncategorized         — fallback bucket
fn category_priority(name: &str) -> u32 {
    match name {
        "Basics" => 0,
        "Daily Planner" => 10,
        "Imported" => 200,
        "Uncategorized" => 300,
        _ => 100,
    }
}

fn describe(bg: &BackgroundType) -> String {
    match bg {
        BackgroundType::Blank => "Blank".into(),
        BackgroundType::Dots { spacing } => format!("Dots ({}mm)", spacing),
        BackgroundType::Lines { spacing } => format!("Lines ({}mm)", spacing),
        BackgroundType::Grid { spacing } => format!("Grid ({}mm)", spacing),
        BackgroundType::Image { .. } => "Image background".into(),
        BackgroundType::Pdf { page, .. } => format!("PDF page {}", page),
    }
}

fn delete_template(id: TemplateId, state: SharedState) {
    let s = state.borrow();
    let removed = {
        let mut reg = s.templates.borrow_mut();
        reg.remove(id)
    };
    if removed.is_none() {
        return;
    }
    let dir = match templates_dir() {
        Some(d) => d,
        None => return,
    };
    let toml_path = dir.join(format!("{}.toml", id.0));
    if toml_path.exists() {
        if let Err(e) = std::fs::remove_file(&toml_path) {
            tracing::warn!("failed to remove template file {:?}: {}", toml_path, e);
        }
    }
    if let Some(BackgroundType::Image { path }) = removed.as_ref().map(|t| &t.background) {
        let p = PathBuf::from(path);
        if p.starts_with(&dir) && p.exists() {
            if let Err(e) = std::fs::remove_file(&p) {
                tracing::warn!("failed to remove template image {:?}: {}", p, e);
            }
        }
    }
}

fn templates_dir() -> Option<PathBuf> {
    let base = dirs::data_dir().or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))?;
    Some(base.join("journal").join("templates"))
}

fn images_dir() -> Option<PathBuf> {
    Some(templates_dir()?.join("images"))
}

fn run_import(
    parent: &ApplicationWindow,
    state: SharedState,
    list: Rc<ListBox>,
    open_editor: OpenEditorFn,
    close_manager: Rc<dyn Fn()>,
) {
    let dialog = FileDialog::builder().title("Import image").modal(true).build();

    let filter = FileFilter::new();
    filter.set_name(Some("Images"));
    filter.add_mime_type("image/*");
    let filters = gtk4::gio::ListStore::new::<FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));

    let parent_for_cb = parent.clone();
    let parent_for_ref = parent.clone();
    dialog.open(Some(parent), None::<&gtk4::gio::Cancellable>, move |result| {
        let file = match result {
            Ok(f) => f,
            Err(e) => {
                if !e.matches(gtk4::gio::IOErrorEnum::Cancelled)
                    && !e.matches(gtk4::gio::IOErrorEnum::Failed)
                {
                    tracing::warn!("file dialog error: {}", e);
                }
                return;
            }
        };
        let src_path = match file.path() {
            Some(p) => p,
            None => {
                tracing::warn!("imported file has no local path");
                return;
            }
        };
        if let Err(e) = import_image(&src_path, state.clone()) {
            tracing::error!("failed to import template image: {:#}", e);
            show_error(&parent_for_cb, &format!("Failed to import image: {}", e));
            return;
        }
        refresh_list(
            &list,
            state.clone(),
            &parent_for_ref,
            open_editor.clone(),
            close_manager.clone(),
        );
    });
}

fn import_image(src: &std::path::Path, state: SharedState) -> anyhow::Result<()> {
    let id = Uuid::new_v4();
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("img")
        .to_lowercase();

    let img_dir = images_dir().ok_or_else(|| anyhow::anyhow!("could not resolve data dir"))?;
    std::fs::create_dir_all(&img_dir)?;
    let dst_image = img_dir.join(format!("{}.{}", id, ext));
    std::fs::copy(src, &dst_image)?;

    let name = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported image")
        .to_string();

    let template = PageTemplate {
        id: journal_core::TemplateId(id),
        name,
        description: format!("Imported from {}", src.display()),
        background: BackgroundType::Image {
            path: dst_image.to_string_lossy().to_string(),
        },
        size_mm: (215.9, 279.4),
        tiling: TilingMode::None,
        default_viewport: None,
        widgets: Vec::new(),
        category: "Imported".into(),
    };

    let tdir = templates_dir().ok_or_else(|| anyhow::anyhow!("could not resolve data dir"))?;
    std::fs::create_dir_all(&tdir)?;
    let toml_path = tdir.join(format!("{}.toml", id));
    let file = template_file_from_page_template(&template);
    let toml_text = serialize_template_toml(&file)
        .map_err(|e| anyhow::anyhow!("serialize template: {}", e))?;
    std::fs::write(&toml_path, toml_text)?;

    let s = state.borrow();
    s.templates.borrow_mut().insert(template);
    Ok(())
}

fn show_error(parent: &ApplicationWindow, message: &str) {
    let dialog = gtk4::AlertDialog::builder()
        .modal(true)
        .message("Template import failed")
        .detail(message)
        .build();
    dialog.show(Some(parent));
}

