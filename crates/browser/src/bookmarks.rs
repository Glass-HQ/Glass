use crate::session;
use gpui::{
    Context, DismissEvent, Entity, EventEmitter, IntoElement, MouseButton, ParentElement, Pixels,
    Point, Render, SharedString, SharedUri, Styled, Subscription, Task, Window, anchored, deferred,
    div, img, prelude::*,
};
use serde::{Deserialize, Serialize};
use ui::{Icon, IconName, IconSize, prelude::*};
use util::ResultExt as _;

#[derive(Serialize, Deserialize, Clone)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub favicon_url: Option<String>,
    pub folder_id: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BookmarkFolder {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
pub enum BookmarkBarVisibility {
    #[default]
    Always,
    NewTabOnly,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct BookmarkStore {
    pub bookmarks: Vec<Bookmark>,
    pub folders: Vec<BookmarkFolder>,
    next_folder_id: u64,
    #[serde(default)]
    pub visibility: BookmarkBarVisibility,
}

#[allow(dead_code)]
impl BookmarkStore {
    pub fn add_bookmark(
        &mut self,
        url: String,
        title: String,
        favicon_url: Option<String>,
        folder_id: Option<u64>,
    ) {
        if self.find_by_url(&url).is_some() {
            return;
        }
        self.bookmarks.push(Bookmark {
            url,
            title,
            favicon_url,
            folder_id,
        });
    }

    pub fn remove_bookmark(&mut self, url: &str) {
        self.bookmarks.retain(|bookmark| bookmark.url != url);
    }

    pub fn add_folder(&mut self, name: String) -> u64 {
        let id = self.next_folder_id;
        self.next_folder_id += 1;
        self.folders.push(BookmarkFolder { id, name });
        id
    }

    pub fn remove_folder(&mut self, folder_id: u64) {
        self.folders.retain(|folder| folder.id != folder_id);
        for bookmark in &mut self.bookmarks {
            if bookmark.folder_id == Some(folder_id) {
                bookmark.folder_id = None;
            }
        }
    }

    pub fn move_to_folder(&mut self, url: &str, folder_id: Option<u64>) {
        for bookmark in &mut self.bookmarks {
            if bookmark.url == url {
                bookmark.folder_id = folder_id;
                break;
            }
        }
    }

    pub fn top_level_bookmarks(&self) -> Vec<&Bookmark> {
        self.bookmarks
            .iter()
            .filter(|bookmark| bookmark.folder_id.is_none())
            .collect()
    }

    pub fn bookmarks_in_folder(&self, folder_id: u64) -> Vec<&Bookmark> {
        self.bookmarks
            .iter()
            .filter(|bookmark| bookmark.folder_id == Some(folder_id))
            .collect()
    }

    pub fn find_by_url(&self, url: &str) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|bookmark| bookmark.url == url)
    }

    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty() && self.folders.is_empty()
    }

    pub fn serialize(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

pub enum BookmarkBarEvent {
    NavigateToUrl(String),
    OpenInNewTab(String),
}

pub struct BookmarkBar {
    store: BookmarkStore,
    is_active_tab_new_tab_page: bool,
    _save_task: Option<Task<()>>,
    bar_menu: Option<BarContextMenu>,
}

struct BarContextMenu {
    menu: Entity<ui::ContextMenu>,
    position: Point<Pixels>,
    _dismiss_subscription: Subscription,
}

impl EventEmitter<BookmarkBarEvent> for BookmarkBar {}

#[allow(dead_code)]
impl BookmarkBar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let store = session::restore_bookmarks().unwrap_or_default();
        Self {
            store,
            is_active_tab_new_tab_page: true,
            _save_task: None,
            bar_menu: None,
        }
    }

    pub fn set_active_tab_is_new_tab_page(&mut self, value: bool) {
        self.is_active_tab_new_tab_page = value;
    }

    pub fn is_visible(&self) -> bool {
        if self.store.is_empty() {
            return false;
        }
        match self.store.visibility {
            BookmarkBarVisibility::Always => true,
            BookmarkBarVisibility::NewTabOnly => self.is_active_tab_new_tab_page,
        }
    }

    pub fn add_bookmark(
        &mut self,
        url: String,
        title: String,
        favicon_url: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.store.add_bookmark(url, title, favicon_url, None);
        self.save(cx);
        cx.notify();
    }

    pub fn remove_bookmark(&mut self, url: &str, cx: &mut Context<Self>) {
        self.store.remove_bookmark(url);
        self.save(cx);
        cx.notify();
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.store.find_by_url(url).is_some()
    }

    pub fn add_folder(&mut self, name: String, cx: &mut Context<Self>) -> u64 {
        let id = self.store.add_folder(name);
        self.save(cx);
        cx.notify();
        id
    }

    pub fn remove_folder(&mut self, folder_id: u64, cx: &mut Context<Self>) {
        self.store.remove_folder(folder_id);
        self.save(cx);
        cx.notify();
    }

    pub fn move_to_folder(
        &mut self,
        url: &str,
        folder_id: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        self.store.move_to_folder(url, folder_id);
        self.save(cx);
        cx.notify();
    }

    fn set_visibility(
        &mut self,
        visibility: BookmarkBarVisibility,
        cx: &mut Context<Self>,
    ) {
        self.store.visibility = visibility;
        self.save(cx);
        cx.notify();
    }

    fn open_bar_context_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let position = window.mouse_position();
        let is_always = self.store.visibility == BookmarkBarVisibility::Always;
        let view = cx.entity().downgrade();

        let menu = ui::ContextMenu::build(window, cx, move |mut menu, _window, _cx| {
            let view = view.clone();
            menu = menu.entry(
                if is_always {
                    "Show Only on New Tab"
                } else {
                    "Show on All Tabs"
                },
                None,
                move |_window, cx| {
                    let new_visibility = if is_always {
                        BookmarkBarVisibility::NewTabOnly
                    } else {
                        BookmarkBarVisibility::Always
                    };
                    view.update(cx, |this, cx| {
                        this.set_visibility(new_visibility, cx);
                    })
                    .ok();
                },
            );
            menu
        });

        let dismiss_subscription = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, _window, _cx| {
            this.bar_menu = None;
        });

        self.bar_menu = Some(BarContextMenu {
            menu,
            position,
            _dismiss_subscription: dismiss_subscription,
        });
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let json = self.store.serialize();
        self._save_task = Some(cx.spawn(async move |this, cx| {
            if let Some(json) = json {
                session::save_bookmarks(json).await.log_err();
            }
            this.update(cx, |this, _| {
                this._save_task.take();
            })
            .ok();
        }));
    }
}

impl Render for BookmarkBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.is_visible() {
            return div().into_any_element();
        }

        let theme = cx.theme();
        let bg_color = theme.colors().title_bar_background;
        let border_color = theme.colors().border;

        let top_level = self.store.top_level_bookmarks();
        let folders = self.store.folders.clone();

        let bookmark_data: Vec<_> = top_level
            .into_iter()
            .map(|b| {
                (
                    b.url.clone(),
                    b.title.clone(),
                    b.favicon_url.clone(),
                )
            })
            .collect();

        let folder_data: Vec<_> = folders
            .iter()
            .map(|folder| {
                let bookmarks: Vec<(String, String)> = self
                    .store
                    .bookmarks_in_folder(folder.id)
                    .into_iter()
                    .map(|b| (b.url.clone(), b.title.clone()))
                    .collect();
                (folder.id, folder.name.clone(), bookmarks)
            })
            .collect();

        let view = cx.entity().downgrade();

        // Build bookmark chip elements (each with its own right-click menu)
        let bookmark_chips: Vec<_> = bookmark_data
            .into_iter()
            .map({
                move |(url, title, favicon_url)| {
                    let display_title = if title.len() > 20 {
                        let truncated = match title.char_indices().nth(17) {
                            Some((byte_index, _)) => &title[..byte_index],
                            None => &title,
                        };
                        format!("{truncated}...")
                    } else if title.is_empty() {
                        truncate_url(&url)
                    } else {
                        title
                    };

                    let navigate_url = url.clone();
                    let delete_url = url.clone();
                    let new_tab_url = url.clone();
                    let open_new_tab_url = url.clone();
                    let view_for_ctx = view.clone();
                    let view_for_open = view.clone();

                    ui::right_click_menu(SharedString::from(format!(
                        "bookmark-ctx-{}",
                        url
                    )))
                    .trigger({
                        let view = view.clone();
                        move |_, _, _| {
                            let navigate_url = navigate_url.clone();
                            let new_tab_url = new_tab_url.clone();
                            let view = view.clone();
                            let view_cmd = view.clone();

                            let favicon_element = favicon_url.as_ref().map(|url| {
                                img(SharedUri::from(url.clone()))
                                    .size(px(14.))
                                    .rounded_sm()
                                    .flex_shrink_0()
                                    .into_any_element()
                            });

                            div()
                                .id(SharedString::from(format!(
                                    "bookmark-chip-{}",
                                    navigate_url
                                )))
                                .flex()
                                .gap_1()
                                .items_center()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .cursor_pointer()
                                .hover(|style| {
                                    style.bg(gpui::hsla(0., 0., 0.5, 0.1))
                                })
                                .on_click(move |event, _window, cx| {
                                    let cmd_held = event.modifiers().platform;
                                    if cmd_held {
                                        let url = new_tab_url.clone();
                                        view_cmd
                                            .update(cx, |_, cx| {
                                                cx.emit(
                                                    BookmarkBarEvent::OpenInNewTab(url),
                                                );
                                            })
                                            .ok();
                                    } else {
                                        let url = navigate_url.clone();
                                        view.update(cx, |_, cx| {
                                            cx.emit(
                                                BookmarkBarEvent::NavigateToUrl(url),
                                            );
                                        })
                                        .ok();
                                    }
                                })
                                .when_some(favicon_element, |this, f| this.child(f))
                                .when(favicon_url.is_none(), |this| {
                                    this.child(
                                        Icon::new(IconName::Star)
                                            .size(IconSize::XSmall)
                                            .color(Color::Muted),
                                    )
                                })
                                .child(
                                    div()
                                        .text_size(rems(0.6875))
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .max_w(px(120.))
                                        .overflow_hidden()
                                        .child(display_title),
                                )
                        }
                    })
                    .menu(move |window, cx| {
                        let delete_url = delete_url.clone();
                        let open_url = open_new_tab_url.clone();
                        let view = view_for_ctx.clone();
                        let view_open = view_for_open.clone();
                        ui::ContextMenu::build(
                            window,
                            cx,
                            move |menu, _window, _cx| {
                                let view_delete = view.clone();
                                let del_url = delete_url;
                                menu.entry(
                                    "Open in New Tab",
                                    None,
                                    {
                                        let view = view_open.clone();
                                        let url = open_url;
                                        move |_window, cx| {
                                            view.update(cx, |_, cx| {
                                                cx.emit(BookmarkBarEvent::OpenInNewTab(url.clone()));
                                            })
                                            .ok();
                                        }
                                    },
                                )
                                .entry(
                                    "Delete",
                                    None,
                                    move |_window, cx| {
                                        view_delete
                                            .update(cx, |this, cx| {
                                                this.remove_bookmark(&del_url, cx);
                                            })
                                            .ok();
                                    },
                                )
                            },
                        )
                    })
                    .into_any_element()
                }
            })
            .collect();

        // Build folder elements
        let folder_elements: Vec<_> = folder_data
            .into_iter()
            .map(|(folder_id, folder_name, _bookmarks)| {
                div()
                    .id(("folder", folder_id as usize))
                    .flex()
                    .gap_1()
                    .items_center()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|style| style.bg(gpui::hsla(0., 0., 0.5, 0.1)))
                    .child(
                        Icon::new(IconName::FolderOpen)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        div()
                            .text_size(rems(0.6875))
                            .whitespace_nowrap()
                            .child(folder_name),
                    )
                    .into_any_element()
            })
            .collect();

        // Build the bar: bookmark chips + folders, right-click on empty space for settings
        let bar_menu_element = self.bar_menu.as_ref().map(|bar_menu| {
            let menu = bar_menu.menu.clone();
            let position = bar_menu.position;
            deferred(
                anchored()
                    .snap_to_window_with_margin(gpui::px(8.))
                    .position(position)
                    .child(div().occlude().child(menu)),
            )
            .with_priority(1)
        });

        h_flex()
            .id("bookmark-bar")
            .w_full()
            .h(px(26.))
            .flex_shrink_0()
            .px_2()
            .gap_0p5()
            .items_center()
            .overflow_hidden()
            .bg(bg_color)
            .border_b_1()
            .border_color(border_color)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, window, cx| {
                    this.open_bar_context_menu(window, cx);
                }),
            )
            .children(bookmark_chips)
            .children(folder_elements)
            .children(bar_menu_element)
            .into_any_element()
    }
}

fn truncate_url(url: &str) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let stripped = stripped.strip_prefix("www.").unwrap_or(stripped);
    if stripped.len() > 25 {
        match stripped.char_indices().nth(22) {
            Some((byte_index, _)) => format!("{}...", &stripped[..byte_index]),
            None => stripped.to_string(),
        }
    } else {
        stripped.to_string()
    }
}
