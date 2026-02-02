use super::{Client, Status, proto};
use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use cloud_api_client::websocket_protocol::MessageToClient;
use cloud_api_client::{GetAuthenticatedUserResponse, PlanInfo};
use cloud_llm_client::{
    EDIT_PREDICTIONS_USAGE_AMOUNT_HEADER_NAME, EDIT_PREDICTIONS_USAGE_LIMIT_HEADER_NAME, Plan,
    UsageLimit,
};
use collections::HashMap;
use derive_more::Deref;
use feature_flags::FeatureFlagAppExt;
use futures::{Future, StreamExt, channel::mpsc};
use gpui::{App, Context, EventEmitter, SharedString, SharedUri, Task, WeakEntity};
use http_client::http::{HeaderMap, HeaderValue};
use postage::{sink::Sink, watch};
use rpc::proto::{RequestMessage, UsersResponse};
use std::{
    str::FromStr as _,
    sync::{Arc, Weak},
};
use text::ReplicaId;
use util::{ResultExt, TryFutureExt as _};

pub type UserId = u64;

#[derive(
    Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, serde::Serialize, serde::Deserialize,
)]
pub struct ChannelId(pub u64);

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct ProjectId(pub u64);

impl ProjectId {
    pub fn to_proto(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParticipantIndex(pub u32);

#[derive(Default, Debug)]
pub struct User {
    pub id: UserId,
    pub github_login: SharedString,
    pub avatar_uri: SharedUri,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collaborator {
    pub peer_id: proto::PeerId,
    pub replica_id: ReplicaId,
    pub user_id: UserId,
    pub is_host: bool,
    pub committer_name: Option<String>,
    pub committer_email: Option<String>,
}

impl PartialOrd for User {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for User {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.github_login.cmp(&other.github_login)
    }
}

impl PartialEq for User {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.github_login == other.github_login
    }
}

impl Eq for User {}

#[derive(Debug, PartialEq)]
pub struct Contact {
    pub user: Arc<User>,
    pub online: bool,
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactRequestStatus {
    None,
    RequestSent,
    RequestReceived,
    RequestAccepted,
}

pub struct UserStore {
    users: HashMap<u64, Arc<User>>,
    by_github_login: HashMap<SharedString, u64>,
    participant_indices: HashMap<u64, ParticipantIndex>,
    update_contacts_tx: mpsc::UnboundedSender<UpdateContacts>,
    edit_prediction_usage: Option<EditPredictionUsage>,
    plan_info: Option<PlanInfo>,
    current_user: watch::Receiver<Option<Arc<User>>>,
    contacts: Vec<Arc<Contact>>,
    incoming_contact_requests: Vec<Arc<User>>,
    outgoing_contact_requests: Vec<Arc<User>>,
    pending_contact_requests: HashMap<u64, usize>,
    client: Weak<Client>,
    _maintain_contacts: Task<()>,
    _maintain_current_user: Task<Result<()>>,
    weak_self: WeakEntity<Self>,
}

#[derive(Clone)]
pub struct InviteInfo {
    pub count: u32,
    pub url: Arc<str>,
}

pub enum Event {
    Contact {
        user: Arc<User>,
        kind: ContactEventKind,
    },
    ShowContacts,
    ParticipantIndicesChanged,
    PrivateUserInfoUpdated,
    PlanUpdated,
}

#[derive(Clone, Copy)]
pub enum ContactEventKind {
    Requested,
    Accepted,
    Cancelled,
}

impl EventEmitter<Event> for UserStore {}

enum UpdateContacts {
    Wait(postage::barrier::Sender),
    Clear(postage::barrier::Sender),
}

#[derive(Debug, Clone, Copy, Deref)]
pub struct EditPredictionUsage(pub RequestUsage);

#[derive(Debug, Clone, Copy)]
pub struct RequestUsage {
    pub limit: UsageLimit,
    pub amount: i32,
}

impl UserStore {
    pub fn new(client: Arc<Client>, cx: &Context<Self>) -> Self {
        let (mut current_user_tx, current_user_rx) = watch::channel();
        let (update_contacts_tx, mut update_contacts_rx) = mpsc::unbounded();
        let rpc_subscriptions: Vec<super::Subscription> = vec![];

        client.add_message_to_client_handler({
            let this = cx.weak_entity();
            move |message, cx| Self::handle_message_to_client(this.clone(), message, cx)
        });

        Self {
            users: Default::default(),
            by_github_login: Default::default(),
            current_user: current_user_rx,
            plan_info: None,
            edit_prediction_usage: None,
            contacts: Default::default(),
            incoming_contact_requests: Default::default(),
            participant_indices: Default::default(),
            outgoing_contact_requests: Default::default(),
            client: Arc::downgrade(&client),
            update_contacts_tx,
            _maintain_contacts: cx.spawn(async move |this, cx| {
                let _subscriptions = rpc_subscriptions;
                while let Some(message) = update_contacts_rx.next().await {
                    if let Ok(task) = this.update(cx, |this, cx| this.update_contacts(message, cx))
                    {
                        task.log_err().await;
                    } else {
                        break;
                    }
                }
            }),
            _maintain_current_user: cx.spawn(async move |this, cx| {
                let mut status = client.status();
                let weak = Arc::downgrade(&client);
                drop(client);
                while let Some(status) = status.next().await {
                    // if the client is dropped, the app is shutting down.
                    let Some(client) = weak.upgrade() else {
                        return Ok(());
                    };
                    match status {
                        Status::Authenticated
                        | Status::Reauthenticated
                        | Status::Connected { .. } => {
                            if let Some(user_id) = client.user_id() {
                                let response = client
                                    .cloud_client()
                                    .get_authenticated_user()
                                    .await
                                    .log_err();

                                let current_user_and_response = if let Some(response) = response {
                                    let user = Arc::new(User {
                                        id: user_id,
                                        github_login: response.user.github_login.clone().into(),
                                        avatar_uri: response.user.avatar_url.clone().into(),
                                        name: response.user.name.clone(),
                                    });

                                    Some((user, response))
                                } else {
                                    None
                                };
                                current_user_tx
                                    .send(
                                        current_user_and_response
                                            .as_ref()
                                            .map(|(user, _)| user.clone()),
                                    )
                                    .await
                                    .ok();

                                cx.update(|cx| {
                                    if let Some((user, response)) = current_user_and_response {
                                        this.update(cx, |this, cx| {
                                            this.by_github_login
                                                .insert(user.github_login.clone(), user_id);
                                            this.users.insert(user_id, user);
                                            this.update_authenticated_user(response, cx)
                                        })
                                    } else {
                                        anyhow::Ok(())
                                    }
                                })?;

                                this.update(cx, |_, cx| cx.notify())?;
                            }
                        }
                        Status::SignedOut => {
                            current_user_tx.send(None).await.ok();
                            this.update(cx, |this, cx| {
                                this.clear_plan_and_usage();
                                cx.emit(Event::PrivateUserInfoUpdated);
                                cx.notify();
                                this.clear_contacts()
                            })?
                            .await;
                        }
                        Status::ConnectionLost => {
                            this.update(cx, |this, cx| {
                                cx.notify();
                                this.clear_contacts()
                            })?
                            .await;
                        }
                        _ => {}
                    }
                }
                Ok(())
            }),
            pending_contact_requests: Default::default(),
            weak_self: cx.weak_entity(),
        }
    }

    #[cfg(feature = "test-support")]
    pub fn clear_cache(&mut self) {
        self.users.clear();
        self.by_github_login.clear();
    }


    fn update_contacts(&mut self, message: UpdateContacts, _cx: &Context<Self>) -> Task<Result<()>> {
        match message {
            UpdateContacts::Wait(barrier) => {
                drop(barrier);
                Task::ready(Ok(()))
            }
            UpdateContacts::Clear(barrier) => {
                self.contacts.clear();
                self.incoming_contact_requests.clear();
                self.outgoing_contact_requests.clear();
                drop(barrier);
                Task::ready(Ok(()))
            }
        }
    }

    pub fn contacts(&self) -> &[Arc<Contact>] {
        &self.contacts
    }

    pub fn has_contact(&self, user: &Arc<User>) -> bool {
        self.contacts
            .binary_search_by_key(&&user.github_login, |contact| &contact.user.github_login)
            .is_ok()
    }

    pub fn incoming_contact_requests(&self) -> &[Arc<User>] {
        &self.incoming_contact_requests
    }

    pub fn outgoing_contact_requests(&self) -> &[Arc<User>] {
        &self.outgoing_contact_requests
    }

    pub fn is_contact_request_pending(&self, user: &User) -> bool {
        self.pending_contact_requests.contains_key(&user.id)
    }

    pub fn contact_request_status(&self, user: &User) -> ContactRequestStatus {
        if self
            .contacts
            .binary_search_by_key(&&user.github_login, |contact| &contact.user.github_login)
            .is_ok()
        {
            ContactRequestStatus::RequestAccepted
        } else if self
            .outgoing_contact_requests
            .binary_search_by_key(&&user.github_login, |user| &user.github_login)
            .is_ok()
        {
            ContactRequestStatus::RequestSent
        } else if self
            .incoming_contact_requests
            .binary_search_by_key(&&user.github_login, |user| &user.github_login)
            .is_ok()
        {
            ContactRequestStatus::RequestReceived
        } else {
            ContactRequestStatus::None
        }
    }

    pub fn request_contact(
        &mut self,
        _responder_id: u64,
        _cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        Task::ready(Err(anyhow::anyhow!("Contact requests are not supported")))
    }

    pub fn remove_contact(&mut self, _user_id: u64, _cx: &mut Context<Self>) -> Task<Result<()>> {
        Task::ready(Err(anyhow::anyhow!("Contact management is not supported")))
    }

    pub fn has_incoming_contact_request(&self, user_id: u64) -> bool {
        self.incoming_contact_requests
            .iter()
            .any(|user| user.id == user_id)
    }

    pub fn respond_to_contact_request(
        &mut self,
        _requester_id: u64,
        _accept: bool,
        _cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        Task::ready(Err(anyhow::anyhow!("Contact requests are not supported")))
    }

    pub fn dismiss_contact_request(
        &self,
        _requester_id: u64,
        _cx: &Context<Self>,
    ) -> Task<Result<()>> {
        Task::ready(Err(anyhow::anyhow!("Contact requests are not supported")))
    }

    pub fn clear_contacts(&self) -> impl Future<Output = ()> + use<> {
        let (tx, mut rx) = postage::barrier::channel();
        self.update_contacts_tx
            .unbounded_send(UpdateContacts::Clear(tx))
            .unwrap();
        async move {
            rx.next().await;
        }
    }

    pub fn contact_updates_done(&self) -> impl Future<Output = ()> {
        let (tx, mut rx) = postage::barrier::channel();
        self.update_contacts_tx
            .unbounded_send(UpdateContacts::Wait(tx))
            .unwrap();
        async move {
            rx.next().await;
        }
    }

    pub fn get_users(
        &self,
        user_ids: Vec<u64>,
        cx: &Context<Self>,
    ) -> Task<Result<Vec<Arc<User>>>> {
        let mut user_ids_to_fetch = user_ids.clone();
        user_ids_to_fetch.retain(|id| !self.users.contains_key(id));

        cx.spawn(async move |this, cx| {
            if !user_ids_to_fetch.is_empty() {
                this.update(cx, |this, cx| {
                    this.load_users(
                        proto::GetUsers {
                            user_ids: user_ids_to_fetch,
                        },
                        cx,
                    )
                })?
                .await?;
            }

            this.read_with(cx, |this, _| {
                user_ids
                    .iter()
                    .map(|user_id| {
                        this.users
                            .get(user_id)
                            .cloned()
                            .with_context(|| format!("user {user_id} not found"))
                    })
                    .collect()
            })?
        })
    }

    pub fn get_cached_user(&self, user_id: u64) -> Option<Arc<User>> {
        self.users.get(&user_id).cloned()
    }

    pub fn get_user_optimistic(&self, user_id: u64, cx: &Context<Self>) -> Option<Arc<User>> {
        if let Some(user) = self.users.get(&user_id).cloned() {
            return Some(user);
        }

        self.get_user(user_id, cx).detach_and_log_err(cx);
        None
    }

    pub fn get_user(&self, user_id: u64, cx: &Context<Self>) -> Task<Result<Arc<User>>> {
        if let Some(user) = self.users.get(&user_id).cloned() {
            return Task::ready(Ok(user));
        }

        let load_users = self.get_users(vec![user_id], cx);
        cx.spawn(async move |this, cx| {
            load_users.await?;
            this.read_with(cx, |this, _| {
                this.users
                    .get(&user_id)
                    .cloned()
                    .context("server responded with no users")
            })?
        })
    }

    pub fn cached_user_by_github_login(&self, github_login: &str) -> Option<Arc<User>> {
        self.by_github_login
            .get(github_login)
            .and_then(|id| self.users.get(id).cloned())
    }

    pub fn current_user(&self) -> Option<Arc<User>> {
        self.current_user.borrow().clone()
    }

    pub fn plan(&self) -> Option<Plan> {
        #[cfg(debug_assertions)]
        if let Ok(plan) = std::env::var("ZED_SIMULATE_PLAN").as_ref() {
            use cloud_llm_client::PlanV2;

            return match plan.as_str() {
                "free" => Some(Plan::V2(PlanV2::ZedFree)),
                "trial" => Some(Plan::V2(PlanV2::ZedProTrial)),
                "pro" => Some(Plan::V2(PlanV2::ZedPro)),
                _ => {
                    panic!("ZED_SIMULATE_PLAN must be one of 'free', 'trial', or 'pro'");
                }
            };
        }

        self.plan_info.as_ref().map(|info| info.plan())
    }

    pub fn subscription_period(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        self.plan_info
            .as_ref()
            .and_then(|plan| plan.subscription_period)
            .map(|subscription_period| {
                (
                    subscription_period.started_at.0,
                    subscription_period.ended_at.0,
                )
            })
    }

    pub fn trial_started_at(&self) -> Option<DateTime<Utc>> {
        self.plan_info
            .as_ref()
            .and_then(|plan| plan.trial_started_at)
            .map(|trial_started_at| trial_started_at.0)
    }

    /// Returns whether the user's account is too new to use the service.
    pub fn account_too_young(&self) -> bool {
        self.plan_info
            .as_ref()
            .map(|plan| plan.is_account_too_young)
            .unwrap_or_default()
    }

    /// Returns whether the current user has overdue invoices and usage should be blocked.
    pub fn has_overdue_invoices(&self) -> bool {
        self.plan_info
            .as_ref()
            .map(|plan| plan.has_overdue_invoices)
            .unwrap_or_default()
    }

    pub fn edit_prediction_usage(&self) -> Option<EditPredictionUsage> {
        self.edit_prediction_usage
    }

    pub fn update_edit_prediction_usage(
        &mut self,
        usage: EditPredictionUsage,
        cx: &mut Context<Self>,
    ) {
        self.edit_prediction_usage = Some(usage);
        cx.notify();
    }

    pub fn clear_plan_and_usage(&mut self) {
        self.plan_info = None;
        self.edit_prediction_usage = None;
    }

    fn update_authenticated_user(
        &mut self,
        response: GetAuthenticatedUserResponse,
        cx: &mut Context<Self>,
    ) {
        let staff = response.user.is_staff && !*feature_flags::ZED_DISABLE_STAFF;
        cx.update_flags(staff, response.feature_flags);
        if let Some(client) = self.client.upgrade() {
            client
                .telemetry
                .set_authenticated_user_info(Some(response.user.metrics_id.clone()), staff);
        }

        self.edit_prediction_usage = Some(EditPredictionUsage(RequestUsage {
            limit: response.plan.usage.edit_predictions.limit,
            amount: response.plan.usage.edit_predictions.used as i32,
        }));
        self.plan_info = Some(response.plan);
        cx.emit(Event::PrivateUserInfoUpdated);
    }

    fn handle_message_to_client(this: WeakEntity<Self>, message: &MessageToClient, cx: &App) {
        cx.spawn(async move |cx| {
            match message {
                MessageToClient::UserUpdated => {
                    let cloud_client = cx
                        .update(|cx| {
                            this.read_with(cx, |this, _cx| {
                                this.client.upgrade().map(|client| client.cloud_client())
                            })
                        })?
                        .ok_or(anyhow::anyhow!("Failed to get Cloud client"))?;

                    let response = cloud_client.get_authenticated_user().await?;
                    cx.update(|cx| {
                        this.update(cx, |this, cx| {
                            this.update_authenticated_user(response, cx);
                        })
                    })?;
                }
            }

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn watch_current_user(&self) -> watch::Receiver<Option<Arc<User>>> {
        self.current_user.clone()
    }

    fn load_users(
        &self,
        request: impl RequestMessage<Response = UsersResponse>,
        cx: &Context<Self>,
    ) -> Task<Result<Vec<Arc<User>>>> {
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            if let Some(rpc) = client.upgrade() {
                let response = rpc.request(request).await.context("error loading users")?;
                let users = response.users;

                this.update(cx, |this, _| this.insert(users))
            } else {
                Ok(Vec::new())
            }
        })
    }

    pub fn insert(&mut self, users: Vec<proto::User>) -> Vec<Arc<User>> {
        let mut ret = Vec::with_capacity(users.len());
        for user in users {
            let user = User::new(user);
            if let Some(old) = self.users.insert(user.id, user.clone())
                && old.github_login != user.github_login
            {
                self.by_github_login.remove(&old.github_login);
            }
            self.by_github_login
                .insert(user.github_login.clone(), user.id);
            ret.push(user)
        }
        ret
    }

    pub fn set_participant_indices(
        &mut self,
        participant_indices: HashMap<u64, ParticipantIndex>,
        cx: &mut Context<Self>,
    ) {
        if participant_indices != self.participant_indices {
            self.participant_indices = participant_indices;
            cx.emit(Event::ParticipantIndicesChanged);
        }
    }

    pub fn participant_indices(&self) -> &HashMap<u64, ParticipantIndex> {
        &self.participant_indices
    }

    pub fn participant_names(
        &self,
        user_ids: impl Iterator<Item = u64>,
        cx: &App,
    ) -> HashMap<u64, SharedString> {
        let mut ret = HashMap::default();
        let mut missing_user_ids = Vec::new();
        for id in user_ids {
            if let Some(github_login) = self.get_cached_user(id).map(|u| u.github_login.clone()) {
                ret.insert(id, github_login);
            } else {
                missing_user_ids.push(id)
            }
        }
        if !missing_user_ids.is_empty() {
            let this = self.weak_self.clone();
            cx.spawn(async move |cx| {
                this.update(cx, |this, cx| this.get_users(missing_user_ids, cx))?
                    .await
            })
            .detach_and_log_err(cx);
        }
        ret
    }
}

impl User {
    fn new(message: proto::User) -> Arc<Self> {
        Arc::new(User {
            id: message.id,
            github_login: message.github_login.into(),
            avatar_uri: message.avatar_url.into(),
            name: message.name,
        })
    }
}


impl Collaborator {
    pub fn from_proto(message: proto::Collaborator) -> Result<Self> {
        Ok(Self {
            peer_id: message.peer_id.context("invalid peer id")?,
            replica_id: ReplicaId::new(message.replica_id as u16),
            user_id: message.user_id as UserId,
            is_host: message.is_host,
            committer_name: message.committer_name,
            committer_email: message.committer_email,
        })
    }
}

impl RequestUsage {
    pub fn over_limit(&self) -> bool {
        match self.limit {
            UsageLimit::Limited(limit) => self.amount >= limit,
            UsageLimit::Unlimited => false,
        }
    }

    fn from_headers(
        limit_name: &str,
        amount_name: &str,
        headers: &HeaderMap<HeaderValue>,
    ) -> Result<Self> {
        let limit = headers
            .get(limit_name)
            .with_context(|| format!("missing {limit_name:?} header"))?;
        let limit = UsageLimit::from_str(limit.to_str()?)?;

        let amount = headers
            .get(amount_name)
            .with_context(|| format!("missing {amount_name:?} header"))?;
        let amount = amount.to_str()?.parse::<i32>()?;

        Ok(Self { limit, amount })
    }
}

impl EditPredictionUsage {
    pub fn from_headers(headers: &HeaderMap<HeaderValue>) -> Result<Self> {
        Ok(Self(RequestUsage::from_headers(
            EDIT_PREDICTIONS_USAGE_LIMIT_HEADER_NAME,
            EDIT_PREDICTIONS_USAGE_AMOUNT_HEADER_NAME,
            headers,
        )?))
    }
}
