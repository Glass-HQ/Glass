use std::collections::BTreeMap;

use gpui::{App, Context, Window};
use service_hub::ServiceProviderDescriptor;
use ui::{ActiveTheme, AnyElement, Color, Label, LabelCommon, LabelSize, prelude::*};

use crate::{
    app_store_connect_provider::{APP_STORE_CONNECT_PROVIDER_ID, AppStoreConnectWorkspaceProvider},
    services_page::ServicesPage,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServicesPageState {
    pub provider_id: String,
    pub navigation_id: String,
    pub selected_resource_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServiceResourceMenuEntry {
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServiceResourceMenuModel {
    pub singular_label: String,
    pub current_label: String,
    pub entries: Vec<ServiceResourceMenuEntry>,
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServiceWorkspacePaneKind {
    AppStoreConnect,
    Unavailable,
}

pub(crate) enum ServiceWorkspacePane {
    AppStoreConnect(AppStoreConnectWorkspaceProvider),
    Unavailable(UnavailableServiceWorkspacePane),
}

impl ServiceWorkspacePane {
    pub fn from_descriptor(
        descriptor: ServiceProviderDescriptor,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        match pane_kind_for_provider(&descriptor.id) {
            ServiceWorkspacePaneKind::AppStoreConnect => Self::AppStoreConnect(
                AppStoreConnectWorkspaceProvider::new(descriptor, window, cx),
            ),
            ServiceWorkspacePaneKind::Unavailable => {
                Self::Unavailable(UnavailableServiceWorkspacePane::new(descriptor))
            }
        }
    }

    pub fn descriptor(&self) -> &ServiceProviderDescriptor {
        match self {
            Self::AppStoreConnect(provider) => provider.descriptor(),
            Self::Unavailable(provider) => provider.descriptor(),
        }
    }

    pub fn normalize_state(&self, state: &mut ServicesPageState) {
        match self {
            Self::AppStoreConnect(provider) => provider.normalize_state(state),
            Self::Unavailable(provider) => provider.normalize_state(state),
        }
    }

    pub fn refresh(
        &mut self,
        state: &mut ServicesPageState,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) {
        match self {
            Self::AppStoreConnect(provider) => provider.refresh(state, window, cx),
            Self::Unavailable(provider) => provider.refresh(state, window, cx),
        }
    }

    pub fn resource_menu(&self, state: &ServicesPageState) -> Option<ServiceResourceMenuModel> {
        match self {
            Self::AppStoreConnect(provider) => provider.resource_menu(state),
            Self::Unavailable(provider) => provider.resource_menu(state),
        }
    }

    pub fn select_resource(
        &mut self,
        state: &mut ServicesPageState,
        resource_id: String,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) {
        match self {
            Self::AppStoreConnect(provider) => {
                provider.select_resource(state, resource_id, window, cx)
            }
            Self::Unavailable(provider) => provider.select_resource(state, resource_id, window, cx),
        }
    }

    pub fn render_section(
        &self,
        state: &ServicesPageState,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) -> AnyElement {
        match self {
            Self::AppStoreConnect(provider) => provider.render_section(state, window, cx),
            Self::Unavailable(provider) => provider.render_section(state, window, cx),
        }
    }

    pub fn render_sidebar_footer(
        &self,
        window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) -> Option<AnyElement> {
        match self {
            Self::AppStoreConnect(provider) => provider.render_sidebar_footer(window, cx),
            Self::Unavailable(provider) => provider.render_sidebar_footer(window, cx),
        }
    }

    pub fn as_app_store_connect_mut(&mut self) -> Option<&mut AppStoreConnectWorkspaceProvider> {
        match self {
            Self::AppStoreConnect(provider) => Some(provider),
            Self::Unavailable(_) => None,
        }
    }
}

pub(crate) fn build_service_workspace_panes(
    descriptors: Vec<ServiceProviderDescriptor>,
    window: &mut Window,
    cx: &mut App,
) -> BTreeMap<String, ServiceWorkspacePane> {
    descriptors
        .into_iter()
        .map(|descriptor| {
            let provider_id = descriptor.id.clone();
            (
                provider_id,
                ServiceWorkspacePane::from_descriptor(descriptor, window, cx),
            )
        })
        .collect()
}

pub(crate) fn collect_provider_descriptors(
    panes: &BTreeMap<String, ServiceWorkspacePane>,
) -> Vec<ServiceProviderDescriptor> {
    panes
        .values()
        .map(|pane| pane.descriptor().clone())
        .collect()
}

pub(crate) fn normalize_services_page_state(
    providers: &[ServiceProviderDescriptor],
    initial_state: Option<ServicesPageState>,
) -> ServicesPageState {
    let provider = initial_state
        .as_ref()
        .and_then(|state| {
            providers
                .iter()
                .find(|provider| provider.id == state.provider_id)
        })
        .or_else(|| providers.first())
        .expect("service hub should register at least one provider");

    let navigation_id = initial_state
        .as_ref()
        .map(|state| state.navigation_id.clone())
        .filter(|navigation_id| {
            provider
                .shell
                .navigation_items
                .iter()
                .any(|item| &item.id == navigation_id)
        })
        .unwrap_or_else(|| provider.shell.default_navigation_item_id.clone());

    ServicesPageState {
        provider_id: provider.id.clone(),
        navigation_id,
        selected_resource_id: initial_state.and_then(|state| state.selected_resource_id),
    }
}

fn pane_kind_for_provider(provider_id: &str) -> ServiceWorkspacePaneKind {
    match provider_id {
        APP_STORE_CONNECT_PROVIDER_ID => ServiceWorkspacePaneKind::AppStoreConnect,
        _ => ServiceWorkspacePaneKind::Unavailable,
    }
}

pub(crate) struct UnavailableServiceWorkspacePane {
    descriptor: ServiceProviderDescriptor,
}

impl UnavailableServiceWorkspacePane {
    fn new(descriptor: ServiceProviderDescriptor) -> Self {
        Self { descriptor }
    }

    fn descriptor(&self) -> &ServiceProviderDescriptor {
        &self.descriptor
    }

    fn normalize_state(&self, state: &mut ServicesPageState) {
        if !self
            .descriptor
            .shell
            .navigation_items
            .iter()
            .any(|item| item.id == state.navigation_id)
        {
            state.navigation_id = self.descriptor.shell.default_navigation_item_id.clone();
        }
        state.selected_resource_id = None;
    }

    fn refresh(
        &mut self,
        _state: &mut ServicesPageState,
        _window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) {
        cx.notify();
    }

    fn resource_menu(&self, _state: &ServicesPageState) -> Option<ServiceResourceMenuModel> {
        None
    }

    fn select_resource(
        &mut self,
        _state: &mut ServicesPageState,
        _resource_id: String,
        _window: &mut Window,
        _cx: &mut Context<ServicesPage>,
    ) {
    }

    fn render_section(
        &self,
        state: &ServicesPageState,
        _window: &mut Window,
        cx: &mut Context<ServicesPage>,
    ) -> AnyElement {
        let section_label = self
            .descriptor
            .shell
            .navigation_items
            .iter()
            .find(|item| item.id == state.navigation_id)
            .map(|item| item.label.clone())
            .unwrap_or_else(|| "Overview".to_string());

        v_flex()
            .w_full()
            .gap_2()
            .p_5()
            .rounded_xl()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().background)
            .child(Label::new(self.descriptor.label.clone()))
            .child(
                Label::new(format!(
                    "{} is defined in service metadata, but there is no UI adapter for it yet.",
                    section_label
                ))
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .into_any_element()
    }

    fn render_sidebar_footer(
        &self,
        _window: &mut Window,
        _cx: &mut Context<ServicesPage>,
    ) -> Option<AnyElement> {
        None
    }
}

#[cfg(test)]
mod tests {
    use service_hub::{
        ServiceAuthKind, ServiceCapabilitySet, ServiceProviderDescriptor, ServiceShellDescriptor,
    };

    use super::{
        APP_STORE_CONNECT_PROVIDER_ID, ServiceWorkspacePaneKind, ServicesPageState,
        normalize_services_page_state, pane_kind_for_provider,
    };

    fn test_provider(id: &str, navigation_ids: &[&str]) -> ServiceProviderDescriptor {
        ServiceProviderDescriptor {
            id: id.to_string(),
            label: id.to_string(),
            shell: ServiceShellDescriptor {
                resource_kind: None,
                navigation_items: navigation_ids
                    .iter()
                    .map(
                        |navigation_id| service_hub::ServiceNavigationItemDescriptor {
                            id: (*navigation_id).to_string(),
                            label: (*navigation_id).to_string(),
                        },
                    )
                    .collect(),
                default_navigation_item_id: navigation_ids[0].to_string(),
            },
            auth_kind: ServiceAuthKind::None,
            auth: None,
            capabilities: ServiceCapabilitySet::default(),
        }
    }

    #[test]
    fn normalizes_invalid_provider_to_first_registered_provider() {
        let providers = vec![
            test_provider(APP_STORE_CONNECT_PROVIDER_ID, &["overview", "builds"]),
            test_provider("vercel", &["overview"]),
        ];

        let state = normalize_services_page_state(
            &providers,
            Some(ServicesPageState {
                provider_id: "missing".to_string(),
                navigation_id: "missing".to_string(),
                selected_resource_id: Some("resource-1".to_string()),
            }),
        );

        assert_eq!(state.provider_id, APP_STORE_CONNECT_PROVIDER_ID);
        assert_eq!(state.navigation_id, "overview");
        assert_eq!(state.selected_resource_id.as_deref(), Some("resource-1"));
    }

    #[test]
    fn normalizes_invalid_navigation_to_provider_default() {
        let providers = vec![test_provider(
            APP_STORE_CONNECT_PROVIDER_ID,
            &["overview", "builds"],
        )];

        let state = normalize_services_page_state(
            &providers,
            Some(ServicesPageState {
                provider_id: APP_STORE_CONNECT_PROVIDER_ID.to_string(),
                navigation_id: "releases".to_string(),
                selected_resource_id: None,
            }),
        );

        assert_eq!(state.provider_id, APP_STORE_CONNECT_PROVIDER_ID);
        assert_eq!(state.navigation_id, "overview");
    }

    #[test]
    fn maps_unknown_provider_ids_to_unavailable_workspace_panes() {
        assert_eq!(
            pane_kind_for_provider("convex"),
            ServiceWorkspacePaneKind::Unavailable
        );
        assert_eq!(
            pane_kind_for_provider(APP_STORE_CONNECT_PROVIDER_ID),
            ServiceWorkspacePaneKind::AppStoreConnect
        );
    }
}
