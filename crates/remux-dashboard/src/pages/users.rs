use crate::{
    components::*, pages::streams::StreamFilterEditor, router::Route, state::AppState,
};
use chrono::Utc;
use dioxus::prelude::*;
use remux_sdks::remux::{
    AdminSetPassword, CollectionFilter, CreateUser, DeleteUser, FilterGroup,
    FilterMatchMode, GetUserStats, GetUsers, StreamFilter, StreamRule, UpdateUser,
    UpdateUserPolicy, UserDto, UserStatsResponse,
};
use uuid::Uuid;

/// Window (in minutes) for treating a user as "online" based on the
/// `last_activity_at` timestamp on `users`. Matches the dashboard
/// SessionsCard active-window.
const ACTIVE_WINDOW_MINUTES: i64 = 16;

/// Parse an RFC3339-ish timestamp string produced by the server into a UTC
/// DateTime. Returns None on anything unparseable (legacy / null rows).
fn parse_dt(s: &Option<String>) -> Option<chrono::DateTime<Utc>> {
    s.as_ref()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Render `dt` as a short relative label ("3m ago", "2h ago", "5d ago").
/// None / unparseable → "—".
fn relative_time(dt: Option<chrono::DateTime<Utc>>) -> String {
    let Some(dt) = dt else {
        return "—".into();
    };
    let now = Utc::now();
    let dur = now.signed_duration_since(dt);
    let secs = dur.num_seconds();
    if secs < 0 {
        return "—".into();
    }
    if secs < 60 {
        return format!("{secs}s ago");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days}d ago");
    }
    dt.format("%Y-%m-%d")
        .to_string()
}

/// First-letter avatar fallback (up to 2 letters).
fn initials(name: &str) -> String {
    let mut iter = name
        .split_whitespace()
        .filter_map(|w| {
            w.chars()
                .next()
        });
    let a = iter.next();
    let b = iter.next();
    match (a, b) {
        (Some(a), Some(b)) => format!("{}{}", a, b),
        (Some(a), None) => a.to_string(),
        _ => "?".into(),
    }
}

#[derive(Clone)]
pub enum UserFormMode {
    Create,
    Edit(UserDto),
}

impl PartialEq for UserFormMode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Create, Self::Create) => true,
            (Self::Edit(a), Self::Edit(b)) => a.id == b.id,
            _ => false,
        }
    }
}

#[component]
pub fn UsersPage(app_state: AppState) -> Element {
    let mut users: Signal<Vec<UserDto>> = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);
    let mut refresh = use_signal(|| 0_u32);
    let mut form_mode: Signal<Option<UserFormMode>> = use_signal(|| None);
    let mut search: Signal<String> = use_signal(String::new);

    // ID of the currently logged-in user (to disable self-delete)
    let self_id = app_state
        .server
        .user_id
        .clone();
    let client = app_state
        .client
        .clone();

    let app_state_effect = app_state.clone();
    use_effect(move || {
        let _r = *refresh.read();
        loading.set(true);
        let client = app_state_effect
            .client
            .clone();
        spawn(async move {
            match client
                .execute(GetUsers)
                .await
            {
                Ok(list) => {
                    users.set(list);
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("Failed to load users: {e}"))),
            }
            loading.set(false);
        });
    });

    // Filter by search term (case-insensitive substring on name).
    let filtered: Vec<UserDto> = users
        .read()
        .clone()
        .into_iter()
        .filter(|u| {
            let q = search
                .read()
                .to_lowercase();
            q.is_empty()
                || u.name
                    .to_lowercase()
                    .contains(&q)
        })
        .collect();

    rsx! {
        div { class: "card",
            div { class: "card-header",
                span { class: "card-title", "Users" }
                button {
                    class: "btn btn-primary",
                    style: "height:32px;font-size:.68rem",
                    onclick: move |_| form_mode.set(Some(UserFormMode::Create)),
                    "+ New User"
                }
            }
            div { class: "card-body tight",
                div { style: "margin-bottom:10px",
                    input {
                        r#type: "search",
                        class: "search-input",
                        placeholder: "Search users…",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                    }
                }

                if *loading.read() {
                    LoadingText {}
                } else if let Some(err) = error.read().as_ref() {
                    span { class: "loading-text", style: "color:var(--error)", "{err}" }
                } else if users.read().is_empty() {
                    EmptyState { message: "No users found" }
                } else if filtered.is_empty() {
                    EmptyState { message: "No users match your search" }
                } else {
                    div { class: "data-table-container",
                        div { class: "row-list",
                            for user in filtered {
                                {
                                    let is_self    = user.id.to_string() == self_id;
                                    let is_admin   = user.policy.is_administrator;
                                    let is_disabled = user.policy.is_disabled;
                                    let last_login = parse_dt(&user.last_login_date.map(|d| d.to_rfc3339()));
                                    let last_activity = parse_dt(&user.last_activity_date.map(|d| d.to_rfc3339()));
                                    let online = !is_disabled
                                        && last_activity
                                            .map(|d| (Utc::now() - d).num_minutes() < ACTIVE_WINDOW_MINUTES)
                                            .unwrap_or(false);
                                    let status_class = if is_disabled {
                                        "user-status user-status-disabled"
                                    } else if online {
                                        "user-status user-status-online"
                                    } else {
                                        "user-status"
                                    };
                                    let user_edit   = user.clone();
                                    let user_view   = user.clone();
                                    let user_toggle = user.clone();
                                    let user_id     = user.id;
                                    let client_del  = client.clone();
                                    let client_toggle = client.clone();
                                    rsx! {
                                        div { class: "flex items-center border-b border-[var(--border)] hover:bg-[rgba(0,0,0,0.03)] even:bg-[rgba(0,0,0,0.02)] even:hover:bg-[rgba(0,0,0,0.03)]", key: "{user.id}",
                                            div { class: "flex-1 min-w-0 px-3 py-[10px] flex items-center gap-3",
                                                span { class: "user-avatar", title: "{user.name}",
                                                    if let Some(_) = user.primary_image_tag.as_ref() {
                                                        img {
                                                            src: "/users/{user.id}/images/primary",
                                                            style: "width:100%;height:100%;object-fit:cover",
                                                        }
                                                    } else {
                                                        {initials(&user.name)}
                                                    }
                                                }
                                                span {
                                                    class: "{status_class}",
                                                    title: if is_disabled { "Disabled" } else if online { "Active now" } else { "Offline" },
                                                }
                                                div { class: "user-info",
                                                    div { class: "flex items-center gap-2",
                                                        button {
                                                            class: "user-name",
                                                            style: "background:none;border:none;padding:0;cursor:pointer;color:inherit",
                                                            title: "View details",
                                                            onclick: move |_| {
                                                                navigator().replace(Route::AccessUserDetailRoute { id: user_view.id });
                                                            },
                                                            "{user.name}"
                                                        }
                                                        if is_self {
                                                            span { class: "user-badge user-badge-self", "You" }
                                                        }
                                                        if is_admin {
                                                            span { class: "user-badge user-badge-admin", "Admin" }
                                                        }
                                                        if is_disabled {
                                                            span { class: "user-badge user-badge-disabled", "Disabled" }
                                                        }
                                                    }
                                                    div { class: "user-meta",
                                                        "login: {relative_time(last_login)}  ·  active: {relative_time(last_activity)}"
                                                    }
                                                }
                                            }
                                            div { class: "shrink-0 px-3 py-[10px] flex items-center gap-2",
                                        button {
                                            class: "btn btn-ghost",
                                            style: "height:30px;font-size:.68rem;padding:0 10px",
                                            title: if is_disabled { "Enable user" } else { "Disable user" },
                                            disabled: self_id == user_toggle.id.to_string(),
                                            onclick: move |_| {
                                                let c = client_toggle.clone();
                                                let mut policy = user_toggle.policy.clone();
                                                policy.is_disabled = !policy.is_disabled;
                                                let uid = user_toggle.id;
                                                spawn(async move {
                                                    let _ = c.execute(UpdateUserPolicy { user_id: uid, policy }).await;
                                                    let v = *refresh.peek() + 1;
                                                    refresh.set(v);
                                                });
                                            },
                                            if is_disabled { "Enable" } else { "Disable" }
                                        }
                                        button {
                                            class: "btn btn-ghost",
                                            style: "height:30px;font-size:.68rem;padding:0 10px",
                                            onclick: move |_| form_mode.set(Some(UserFormMode::Edit(user_edit.clone()))),
                                            "Edit"
                                        }
                                        button {
                                            class: "btn btn-ghost",
                                            style: "height:30px;font-size:.68rem;padding:0 10px;color:var(--error);border-color:var(--error)",
                                            disabled: is_self,
                                            onclick: move |_| {
                                                let c = client_del.clone();
                                                spawn(async move {
                                                    let _ = c.execute(DeleteUser { user_id }).await;
                                                    let v = *refresh.peek() + 1;
                                                    refresh.set(v);
                                                });
                                            },
                                            "Delete"
                                        }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(mode) = form_mode.read().clone() {
            div { class: "modal-backdrop",
                div { class: "modal",
                    UserForm {
                        mode,
                        app_state: app_state.clone(),
                        on_done: move |_| {
                            form_mode.set(None);
                            let v = *refresh.peek() + 1;
                            refresh.set(v);
                        },
                        on_cancel: move |_| form_mode.set(None),
                    }
                }
            }
        }
    }
}

#[component]
pub fn UserForm(
    mode: UserFormMode,
    app_state: AppState,
    on_done: EventHandler,
    on_cancel: EventHandler,
) -> Element {
    let is_edit = matches!(mode, UserFormMode::Edit(_));
    let existing: Option<UserDto> = match &mode {
        UserFormMode::Edit(u) => Some(u.clone()),
        UserFormMode::Create => None,
    };

    let mut username = use_signal(|| {
        existing
            .as_ref()
            .map(|u| {
                u.name
                    .clone()
            })
            .unwrap_or_default()
    });
    let mut is_admin = use_signal(|| {
        existing
            .as_ref()
            .map(|u| {
                u.policy
                    .is_administrator
            })
            .unwrap_or(false)
    });
    let mut password = use_signal(String::new);
    let mut password2 = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut err = use_signal(|| Option::<String>::None);
    let fr_match: Signal<FilterMatchMode> = use_signal(|| {
        existing
            .as_ref()
            .and_then(|u| {
                u.policy
                    .filter_rules
                    .as_ref()
            })
            .map(|f| {
                f.match_mode
                    .clone()
            })
            .unwrap_or(FilterMatchMode::All)
    });
    let fr_groups: Signal<Vec<FilterGroup>> = use_signal(|| {
        existing
            .as_ref()
            .and_then(|u| {
                u.policy
                    .filter_rules
                    .as_ref()
            })
            .map(|f| {
                f.groups
                    .clone()
            })
            .unwrap_or_else(|| vec![FilterGroup::default()])
    });
    let sf_stream_match: Signal<FilterMatchMode> = use_signal(|| {
        existing
            .as_ref()
            .and_then(|u| {
                u.policy
                    .stream_filter
                    .as_ref()
            })
            .map(|f| {
                f.match_mode
                    .clone()
            })
            .unwrap_or(FilterMatchMode::All)
    });
    let sf_stream_rules: Signal<Vec<StreamRule>> = use_signal(|| {
        existing
            .as_ref()
            .and_then(|u| {
                u.policy
                    .stream_filter
                    .as_ref()
            })
            .map(|f| {
                f.rules
                    .clone()
            })
            .unwrap_or_default()
    });
    let mut enable_remote_search = use_signal(|| {
        existing
            .as_ref()
            .map(|u| {
                u.policy
                    .enable_remote_search
            })
            .unwrap_or(true)
    });
    let mut max_active_sessions: Signal<i64> = use_signal(|| {
        existing
            .as_ref()
            .map(|u| {
                u.policy
                    .max_active_sessions
            })
            .unwrap_or(0)
    });
    let mut enable_video_transcoding = use_signal(|| {
        existing
            .as_ref()
            .map(|u| {
                u.policy
                    .enable_video_playback_transcoding
            })
            .unwrap_or(true)
    });
    let mut is_disabled: Signal<bool> = use_signal(|| {
        existing
            .as_ref()
            .map(|u| {
                u.policy
                    .is_disabled
            })
            .unwrap_or(false)
    });
    let mut max_parental_rating: Signal<String> = use_signal(|| {
        existing
            .as_ref()
            .and_then(|u| {
                u.policy
                    .max_parental_rating
            })
            .map(|n| n.to_string())
            .unwrap_or_default()
    });
    let blocked_tags: Signal<Vec<String>> = use_signal(|| {
        existing
            .as_ref()
            .map(|u| {
                u.policy
                    .blocked_tags
                    .clone()
            })
            .unwrap_or_default()
    });

    let on_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let pw = password
            .peek()
            .clone();
        let pw2 = password2
            .peek()
            .clone();
        if !pw.is_empty() && pw != pw2 {
            err.set(Some("Passwords do not match".into()));
            return;
        }
        if !is_edit && pw.is_empty() {
            err.set(Some("Password is required".into()));
            return;
        }

        let client = app_state
            .client
            .clone();
        let name = username
            .peek()
            .clone();
        let admin = *is_admin.peek();
        let user_dto = existing.clone();
        let groups_snapshot = fr_groups
            .peek()
            .clone();
        let match_snapshot = fr_match
            .peek()
            .clone();
        let stream_rules_snapshot = sf_stream_rules
            .peek()
            .clone();
        let stream_match_snapshot = sf_stream_match
            .peek()
            .clone();
        let remote_search_snapshot = *enable_remote_search.peek();
        let max_sessions_snapshot = *max_active_sessions.peek();
        let video_transcoding_snapshot = *enable_video_transcoding.peek();
        let is_disabled_snapshot = *is_disabled.peek();
        let parental_snapshot = max_parental_rating
            .peek()
            .clone();
        let parental_snapshot = parental_snapshot
            .trim()
            .parse::<i32>()
            .ok();
        let blocked_tags_snapshot = blocked_tags
            .peek()
            .clone();

        saving.set(true);
        err.set(None);
        spawn(async move {
            let has_rules = groups_snapshot
                .iter()
                .any(|g| {
                    !g.rules
                        .is_empty()
                });
            let filter_rules = if has_rules {
                Some(CollectionFilter {
                    match_mode: match_snapshot,
                    groups: groups_snapshot,
                })
            } else {
                None
            };
            let stream_filter = if stream_rules_snapshot.is_empty() {
                None
            } else {
                Some(StreamFilter {
                    match_mode: stream_match_snapshot,
                    rules: stream_rules_snapshot,
                })
            };
            let result: Result<(), remux_sdks::ClientError> = async {
                if is_edit {
                    let user = user_dto
                        .as_ref()
                        .unwrap();
                    // Update username
                    let mut updated = user.clone();
                    updated.name = name;
                    client
                        .execute(UpdateUser {
                            user_id: user.id,
                            dto: updated,
                        })
                        .await?;
                    // Update admin flag and filter rules
                    let mut policy = user
                        .policy
                        .clone();
                    policy.is_administrator = admin;
                    policy.filter_rules = filter_rules.clone();
                    policy.stream_filter = stream_filter.clone();
                    policy.enable_remote_search = remote_search_snapshot;
                    policy.max_active_sessions = max_sessions_snapshot;
                    policy.enable_video_playback_transcoding =
                        video_transcoding_snapshot;
                    policy.is_disabled = is_disabled_snapshot;
                    policy.max_parental_rating = parental_snapshot;
                    policy.blocked_tags = blocked_tags_snapshot.clone();
                    client
                        .execute(UpdateUserPolicy {
                            user_id: user.id,
                            policy,
                        })
                        .await?;
                    // Change password only if provided
                    if !pw.is_empty() {
                        client
                            .execute(AdminSetPassword {
                                user_id: user.id,
                                new_pw: pw,
                            })
                            .await?;
                    }
                } else {
                    // Create user
                    let new_user = client
                        .execute(CreateUser { name, password: pw })
                        .await?;
                    if admin
                        || filter_rules.is_some()
                        || stream_filter.is_some()
                        || !remote_search_snapshot
                        || max_sessions_snapshot > 0
                        || !video_transcoding_snapshot
                        || is_disabled_snapshot
                        || parental_snapshot.is_some()
                        || !blocked_tags_snapshot.is_empty()
                    {
                        let mut policy = new_user
                            .policy
                            .clone();
                        policy.is_administrator = admin;
                        policy.filter_rules = filter_rules.clone();
                        policy.stream_filter = stream_filter.clone();
                        policy.enable_remote_search = remote_search_snapshot;
                        policy.max_active_sessions = max_sessions_snapshot;
                        policy.enable_video_playback_transcoding =
                            video_transcoding_snapshot;
                        policy.is_disabled = is_disabled_snapshot;
                        policy.max_parental_rating = parental_snapshot;
                        policy.blocked_tags = blocked_tags_snapshot.clone();
                        client
                            .execute(UpdateUserPolicy {
                                user_id: new_user.id,
                                policy,
                            })
                            .await?;
                    }
                }
                Ok(())
            }
            .await;

            match result {
                Ok(_) => on_done.call(()),
                Err(e) => {
                    err.set(Some(e.user_message()));
                    saving.set(false);
                }
            }
        });
    };

    rsx! {
        p { class: "modal-title",
            if is_edit { "Edit User" } else { "New User" }
        }

        form {
            onsubmit: on_submit,
            style: "display:flex;flex-direction:column;gap:14px",

            div { class: "field",
                label { class: "field-label", r#for: "u-name", "Username" }
                input {
                    id: "u-name",
                    r#type: "text",
                    class: "field-input",
                    required: true,
                    value: "{username}",
                    oninput: move |e| username.set(e.value()),
                }
            }

            div { class: "field",
                label { class: "field-label", r#for: "u-pw",
                    if is_edit { "New Password" } else { "Password" }
                }
                input {
                    id: "u-pw",
                    r#type: "password",
                    class: "field-input",
                    required: !is_edit,
                    placeholder: if is_edit { "Leave blank to keep current" } else { "" },
                    value: "{password}",
                    oninput: move |e| password.set(e.value()),
                }
            }

            if !password.read().is_empty() || !is_edit {
                div { class: "field",
                    label { class: "field-label", r#for: "u-pw2", "Confirm Password" }
                    input {
                        id: "u-pw2",
                        r#type: "password",
                        class: "field-input",
                        required: !is_edit,
                        value: "{password2}",
                        oninput: move |e| password2.set(e.value()),
                    }
                }
            }

            ToggleRow {
                label: "Administrator",
                checked: *is_admin.read(),
                on_change: move |v| is_admin.set(v),
            }

            ToggleRow {
                label: "Allow Remote Search",
                checked: *enable_remote_search.read(),
                on_change: move |v| enable_remote_search.set(v),
            }

            ToggleRow {
                label: "Allow Video Transcoding",
                checked: *enable_video_transcoding.read(),
                on_change: move |v| enable_video_transcoding.set(v),
            }

            div { class: "field",
                label { class: "field-label", r#for: "u-max-streams", "Max Concurrent Streams" }
                input {
                    id: "u-max-streams",
                    r#type: "number",
                    class: "field-input",
                    min: "1",
                    placeholder: "Unlimited",
                    value: if *max_active_sessions.read() > 0 { max_active_sessions.read().to_string() } else { String::new() },
                    oninput: move |e| {
                        let v = e.value();
                        max_active_sessions.set(
                            v.parse::<i64>().map(|n| n.max(1)).unwrap_or(0)
                        );
                    },
                }
                span { class: "field-hint", "Leave blank for unlimited" }
            }

            ToggleRow {
                label: "Account Disabled",
                checked: *is_disabled.read(),
                on_change: move |v| is_disabled.set(v),
            }

            div { class: "field",
                label { class: "field-label", r#for: "u-parental", "Max Parental Rating (age)" }
                input {
                    id: "u-parental",
                    r#type: "number",
                    class: "field-input",
                    min: "0",
                    placeholder: "No limit",
                    value: "{max_parental_rating}",
                    oninput: move |e| max_parental_rating.set(e.value()),
                }
                span { class: "field-hint", "Items above this age rating are hidden for this user" }
            }

            div { class: "field",
                label { class: "field-label", "Blocked Tags" }
                TagChipInput { tags: blocked_tags }
                span { class: "field-hint", "Items with any of these tags are hidden for this user" }
            }

            FilterRuleEditor {
                match_mode: fr_match,
                groups: fr_groups,
            }

            div { style: "margin-top:10px",
                StreamFilterEditor {
                    match_mode: sf_stream_match,
                    rules: sf_stream_rules,
                }
            }

            if let Some(e) = err.read().as_ref() {
                ErrorAlert { message: e.clone() }
            }

            FormActions {
                button {
                    r#type: "button",
                    class: "btn btn-ghost",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                button {
                    r#type: "submit",
                    class: "btn btn-primary",
                    disabled: *saving.read(),
                    if *saving.read() { "Saving…" } else { "Save" }
                }
            }
        }
    }
}

/// A single stat tile in the detail grid.
#[component]
fn StatTile(label: String, value: String, sub: Option<String>) -> Element {
    rsx! {
        div { class: "stat-tile",
            div { class: "stat-tile-label", "{label}" }
            div { class: "stat-tile-value", "{value}" }
            if let Some(s) = sub {
                div { class: "stat-tile-sub", "{s}" }
            }
        }
    }
}

/// Humanize a duration of seconds into "Xh Ym" / "Ym" / "Xs".
fn humanize_seconds(secs: i64) -> String {
    if secs <= 0 {
        return "0m".into();
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{secs}s")
    }
}

#[component]
pub fn UserDetailPage(app_state: AppState, user_id: Uuid) -> Element {
    let mut stats: Signal<Option<UserStatsResponse>> = use_signal(|| None);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);
    let refresh = use_signal(|| 0_u32);

    let app_state_effect = app_state.clone();
    use_effect(move || {
        let _r = *refresh.read();
        loading.set(true);
        let client = app_state_effect
            .client
            .clone();
        spawn(async move {
            match client
                .execute(GetUserStats {
                    user_id,
                    recent: Some(20),
                })
                .await
            {
                Ok(s) => {
                    stats.set(Some(s));
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("Failed to load stats: {e}"))),
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "card",
            div { class: "card-header",
                button {
                    class: "btn btn-ghost",
                    style: "height:32px;font-size:.68rem;padding:0 10px;margin-right:8px",
                    onclick: move |_| {
                        navigator().replace(Route::AccessUsersRoute {});
                    },
                    "← Back"
                }
                span { class: "card-title", "User Details" }
            }
            div { class: "card-body",
                if *loading.read() {
                    LoadingText {}
                } else if let Some(err) = error.read().as_ref() {
                    span { class: "loading-text", style: "color:var(--error)", "{err}" }
                } else {
                    {
                        // Clone the stats response out of the Signal so the rsx
                        // closures below are 'static (the Signal borrow would not
                        // outlive the closures captured by the for-loop).
                        let resp_opt = stats.read().clone();
                        if let Some(resp) = resp_opt {
                        let s = resp.stats.clone();
                        let recent = resp.recent.clone();
                        let online = !s.is_disabled
                            && s.last_activity_at
                                .map(|d| (Utc::now() - d).num_minutes() < ACTIVE_WINDOW_MINUTES)
                                .unwrap_or(false);
                        let online_status = if online { "online" } else { "offline" };
                        rsx! {
                            div {
                                div { class: "flex items-center gap-3", style: "margin-bottom:16px",
                                    span { class: "user-avatar lg",
                                        img {
                                            src: "/users/{s.user_id}/images/primary",
                                            style: "width:100%;height:100%;object-fit:cover",
                                        }
                                    }
                                    div {
                                        div { class: "flex items-center gap-2",
                                            span { style: "font-size:1.1rem;font-weight:700", "{s.username}" }
                                            if s.is_admin {
                                                span { class: "user-badge user-badge-admin", "Admin" }
                                            }
                                            if s.is_disabled {
                                                span { class: "user-badge user-badge-disabled", "Disabled" }
                                            }
                                        }
                                        div { class: "user-meta",
                                            "last login: {relative_time(s.last_login_at)}  ·  last active: {relative_time(s.last_activity_at)}"
                                        }
                                    }
                                }

                                if online {
                                    span { class: "user-status user-status-online", style: "margin-right:6px" }
                                }

                                div { class: "stat-grid", style: "margin-top:8px",
                                    StatTile {
                                        label: "Total Plays".to_string(),
                                        value: s.total_plays.to_string(),
                                        sub: None,
                                    }
                                    StatTile {
                                        label: "Played Items".to_string(),
                                        value: s.played_items.to_string(),
                                        sub: None,
                                    }
                                    StatTile {
                                        label: "Favorites".to_string(),
                                        value: s.favorite_items.to_string(),
                                        sub: None,
                                    }
                                    StatTile {
                                        label: "Resume".to_string(),
                                        value: s.resume_items.to_string(),
                                        sub: None,
                                    }
                                    StatTile {
                                        label: "Active Devices".to_string(),
                                        value: s.active_device_count.to_string(),
                                        sub: Some(online_status.to_string()),
                                    }
                                    StatTile {
                                        label: "Watch Time".to_string(),
                                        value: humanize_seconds(s.watch_time_seconds),
                                        sub: Some("estimated".to_string()),
                                    }
                                }

                                div { style: "margin-top:18px",
                                    div { class: "card-title", style: "font-size:.8rem;margin-bottom:8px", "Recently Played" }
                                    if recent.is_empty() {
                                        EmptyState { message: "No playback history yet".to_string() }
                                    } else {
                                        div { class: "data-table-container",
                                            div { class: "row-list",
                                                for item in recent.iter() {
                                                    div {
                                                        class: "flex items-center border-b border-[var(--border)] hover:bg-[rgba(0,0,0,0.03)] even:bg-[rgba(0,0,0,0.02)]",
                                                        key: "{item.media_id}",
                                                        div { class: "flex-1 min-w-0 px-3 py-[8px]",
                                                            div { style: "font-size:.84rem;font-weight:600",
                                                                {item.title.clone().unwrap_or_else(|| item.media_id.to_string())}
                                                            }
                                                            div { class: "user-meta",
                                                                "{item.kind.as_deref().unwrap_or(\"\")} · played {item.play_count}×"
                                                            }
                                                        }
                                                        div { class: "shrink-0 px-3 py-[8px] text-right",
                                                            span { class: "user-meta", "{relative_time(item.last_played_at)}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        } else { rsx! {} }
                    }
                }
            }
        }
    }
}
