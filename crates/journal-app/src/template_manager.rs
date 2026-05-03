use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, FileDialog, FileFilter, Image, Label, ListBox,
    Orientation, ScrolledWindow, Window,
};
use journal_core::{BackgroundType, PageTemplate, TemplateId, TilingMode};
use journal_templates::{
    is_builtin, serialize_template_toml, template_file_from_page_template,
};
use uuid::Uuid;

use crate::state::SharedState;

pub fn open(parent: &ApplicationWindow, state: SharedState) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(false)
        .title("Templates")
        .default_width(480)
        .default_height(560)
        .build();

    let body = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    let title = Label::builder()
        .label("Page Templates")
        .halign(gtk4::Align::Start)
        .hexpand(true)
        .build();
    title.add_css_class("title-3");
    header.append(&title);

    let new_btn = Button::with_label("New template...");
    new_btn.add_css_class("suggested-action");
    header.append(&new_btn);

    let import_btn = Button::with_label("Import image...");
    header.append(&import_btn);

    let import_pdf_btn = Button::with_label("Import PDF...");
    header.append(&import_pdf_btn);
    body.append(&header);

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let list = ListBox::builder().build();
    scroller.set_child(Some(&list));
    body.append(&scroller);

    let close_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .halign(gtk4::Align::End)
        .build();
    let close = Button::with_label("Close");
    close.connect_clicked({
        let win = win.clone();
        move |_| win.close()
    });
    close_row.append(&close);
    body.append(&close_row);

    win.set_child(Some(&body));

    let list_rc = Rc::new(list);
    refresh_list(&list_rc, state.clone(), parent);

    {
        let parent = parent.clone();
        let state = state.clone();
        let list = list_rc.clone();
        new_btn.connect_clicked(move |_| {
            let list2 = list.clone();
            let state2 = state.clone();
            let parent2 = parent.clone();
            crate::template_creator::open(&parent, state.clone(), None, move || {
                refresh_list(&list2, state2.clone(), &parent2);
            });
        });
    }

    {
        let parent = parent.clone();
        let state = state.clone();
        let list = list_rc.clone();
        import_btn.connect_clicked(move |_| {
            run_import(&parent, state.clone(), list.clone());
        });
    }

    {
        let parent = parent.clone();
        let state = state.clone();
        let list = list_rc.clone();
        import_pdf_btn.connect_clicked(move |_| {
            run_pdf_import(&parent, state.clone(), list.clone());
        });
    }

    win.present();
}

fn pdfs_dir() -> Option<PathBuf> {
    Some(templates_dir()?.join("pdfs"))
}

fn run_pdf_import(parent: &ApplicationWindow, state: SharedState, list: Rc<ListBox>) {
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
    let parent_for_ref = parent.clone();
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
        if let Err(e) = import_pdf(&parent_for_cb, &src_path, state.clone(), list.clone()) {
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

fn import_pdf(parent: &ApplicationWindow, src: &std::path::Path, state: SharedState, list: Rc<ListBox>) -> anyhow::Result<()> {
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
        refresh_list(&list, state, parent);
        return Ok(());
    }

    // Ask the user which page to use (1-based display, 0-based storage).
    show_pdf_page_picker(parent, id, name, dst_str, n_pages, state, list);
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

fn show_pdf_page_picker(parent: &ApplicationWindow, id: Uuid, name: String, dst: String, n_pages: u32, state: SharedState, list: Rc<ListBox>) {
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
        ok_btn.connect_clicked(move |_| {
            let page = (spin.value() as u32).saturating_sub(1);
            finalize_pdf_template(id, name.clone(), dst.clone(), page, state.clone());
            refresh_list(&list, state.clone(), &parent_for_refresh);
            win.close();
        });
    }

    win.present();
}

fn refresh_list(list: &Rc<ListBox>, state: SharedState, parent: &ApplicationWindow) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let templates: Vec<PageTemplate> = {
        let s = state.borrow();
        let reg = s.templates.borrow();
        let mut v: Vec<PageTemplate> = reg.list().iter().map(|t| (*t).clone()).collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        v
    };

    for t in templates {
        let row = build_row(&t, state.clone(), list.clone(), parent);
        list.append(&row);
    }
}

fn build_row(t: &PageTemplate, state: SharedState, list: Rc<ListBox>, parent: &ApplicationWindow) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_top(4)
        .margin_bottom(4)
        .margin_start(4)
        .margin_end(4)
        .build();

    let icon = Image::from_icon_name(icon_for(&t.background));
    icon.set_icon_size(gtk4::IconSize::Large);
    row.append(&icon);

    let text_col = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .build();
    let name = Label::builder()
        .label(&t.name)
        .halign(gtk4::Align::Start)
        .build();
    name.add_css_class("title-4");
    text_col.append(&name);
    let kind = Label::builder()
        .label(describe(&t.background))
        .halign(gtk4::Align::Start)
        .build();
    kind.add_css_class("dim-label");
    text_col.append(&kind);
    row.append(&text_col);

    if !is_builtin(t.id) {
        let edit_btn = Button::from_icon_name("document-edit-symbolic");
        edit_btn.set_tooltip_text(Some("Edit template"));
        let template_for_edit = t.clone();
        let state_for_edit = state.clone();
        let list_for_edit = list.clone();
        let parent_for_edit = parent.clone();
        edit_btn.connect_clicked(move |_| {
            let list2 = list_for_edit.clone();
            let state2 = state_for_edit.clone();
            let parent2 = parent_for_edit.clone();
            crate::template_creator::open(
                &parent_for_edit,
                state_for_edit.clone(),
                Some(template_for_edit.clone()),
                move || {
                    refresh_list(&list2, state2.clone(), &parent2);
                },
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
        del.connect_clicked(move |_| {
            delete_template(tid, state_for_del.clone());
            refresh_list(&list_for_del, state_for_del.clone(), &parent_for_del);
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

fn icon_for(bg: &BackgroundType) -> &'static str {
    match bg {
        BackgroundType::Blank => "view-paged-symbolic",
        BackgroundType::Dots { .. } => "view-grid-symbolic",
        BackgroundType::Lines { .. } => "view-list-symbolic",
        BackgroundType::Grid { .. } => "view-grid-symbolic",
        BackgroundType::Image { .. } => "image-x-generic-symbolic",
        BackgroundType::Pdf { .. } => "x-office-document-symbolic",
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

fn run_import(parent: &ApplicationWindow, state: SharedState, list: Rc<ListBox>) {
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
        refresh_list(&list, state.clone(), &parent_for_ref);
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

