use crate::context_menu_handler::ContextMenuContext;
use crate::tab::TabEvent;
use gpui::{Context, DismissEvent, Entity, Pixels, Point, Subscription, Window};

use super::BrowserView;

pub(super) struct BrowserContextMenu {
    pub(super) menu: Entity<ui::ContextMenu>,
    pub(super) position: Point<Pixels>,
    pub(super) _dismiss_subscription: Subscription,
}

pub(super) struct PendingContextMenu {
    pub(super) context: ContextMenuContext,
}

impl BrowserView {
    pub(super) fn open_context_menu(
        &mut self,
        context: ContextMenuContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = window.mouse_position();
        let tab = match self.active_tab().cloned() {
            Some(tab) => tab,
            None => return,
        };

        let menu = ui::ContextMenu::build(window, cx, move |mut menu, _window, _cx| {
            let has_link = context.link_url.is_some();
            let has_selection = context.selection_text.is_some();

            if let Some(link_url) = &context.link_url {
                let url = link_url.clone();
                let tab = tab.clone();
                menu = menu.entry("Open Link in New Tab", None, move |_window, cx| {
                    tab.update(cx, |_, cx| {
                        cx.emit(TabEvent::OpenNewTab(url.clone()));
                    });
                });

                let url = link_url.clone();
                menu = menu.entry("Copy Link Address", None, move |_window, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(url.clone()));
                });

                menu = menu.separator();
            }

            if context.is_editable {
                if context.can_undo {
                    let tab = tab.clone();
                    menu = menu.entry("Undo", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.undo());
                    });
                }
                if context.can_redo {
                    let tab = tab.clone();
                    menu = menu.entry("Redo", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.redo());
                    });
                }
                if context.can_undo || context.can_redo {
                    menu = menu.separator();
                }
                if context.can_cut {
                    let tab = tab.clone();
                    menu = menu.entry("Cut", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.cut());
                    });
                }
                if context.can_copy {
                    let tab = tab.clone();
                    menu = menu.entry("Copy", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.copy());
                    });
                }
                if context.can_paste {
                    let tab = tab.clone();
                    menu = menu.entry("Paste", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.paste());
                    });
                }
                if context.can_delete {
                    let tab = tab.clone();
                    menu = menu.entry("Delete", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.delete());
                    });
                }
                menu = menu.separator();
                if context.can_select_all {
                    let tab = tab.clone();
                    menu = menu.entry("Select All", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.select_all());
                    });
                }
            } else {
                if has_selection {
                    let tab = tab.clone();
                    menu = menu.entry("Copy", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.copy());
                    });
                    menu = menu.separator();
                }
            }

            if !has_link && !has_selection && !context.is_editable {
                {
                    let tab = tab.clone();
                    menu = menu.entry("Back", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.go_back());
                    });
                }
                {
                    let tab = tab.clone();
                    menu = menu.entry("Forward", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.go_forward());
                    });
                }
                {
                    let tab = tab.clone();
                    menu = menu.entry("Reload", None, move |_window, cx| {
                        tab.update(cx, |tab, _| tab.reload());
                    });
                }
                menu = menu.separator();
            }

            {
                menu = menu.entry("Inspect", None, move |_window, cx| {
                    tab.update(cx, |tab, _| tab.open_devtools());
                });
            }

            menu
        });

        let dismiss_subscription = cx.subscribe(&menu, {
            move |this, _, _event: &DismissEvent, cx| {
                this.context_menu.take();
                cx.notify();
            }
        });

        self.context_menu = Some(BrowserContextMenu {
            menu,
            position,
            _dismiss_subscription: dismiss_subscription,
        });

        cx.notify();
    }

    pub(super) fn dismiss_context_menu(&mut self) {
        if let Some(cm) = self.context_menu.take() {
            drop(cm);
        }
    }
}
