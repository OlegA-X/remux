use crate::{components::Card, router::Route, state::AppState};
use dioxus::prelude::*;
use remux_sdks::remux::{GetUsersOverviewStats, UsersOverviewStats};

#[component]
pub fn UsersOverviewCard(app_state: AppState) -> Element {
    let mut stats: Signal<Option<UsersOverviewStats>> = use_signal(|| None);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);

    use_effect(move || {
        let client = app_state
            .client
            .clone();
        spawn(async move {
            match client
                .execute(GetUsersOverviewStats)
                .await
            {
                Ok(s) => {
                    stats.set(Some(s));
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("Failed to fetch user stats: {e}"))),
            }
            loading.set(false);
        });
    });

    // Clone out of the Signal so the for-loop closures below are 'static.
    let s_opt = stats
        .read()
        .clone();

    rsx! {
        Card {
            title: "Users".to_string(),
            action: Some(rsx! {
                button {
                    class: "btn btn-ghost",
                    style: "height:32px;font-size:.68rem;padding:0 10px",
                    onclick: move |_| {
                        navigator().replace(Route::AccessUsersRoute {});
                    },
                    "Manage"
                }
            }),

            if *loading.read() {
                span { class: "loading-text", "Loading…" }
            } else if let Some(err) = error.read().as_ref() {
                span { class: "loading-text", style: "color:var(--error)", "{err}" }
            } else if let Some(s) = s_opt.as_ref() {
                div { class: "stat-grid",
                    div { class: "stat-tile",
                        div { class: "stat-tile-label", "Total" }
                        div { class: "stat-tile-value", "{s.total_users}" }
                    }
                    div { class: "stat-tile",
                        div { class: "stat-tile-label", "Admins" }
                        div { class: "stat-tile-value", "{s.admin_users}" }
                    }
                    div { class: "stat-tile",
                        div { class: "stat-tile-label", "Disabled" }
                        div { class: "stat-tile-value", "{s.disabled_users}" }
                    }
                    div { class: "stat-tile",
                        div { class: "stat-tile-label", "Active 24h" }
                        div { class: "stat-tile-value", "{s.active_24h}" }
                        div { class: "stat-tile-sub", "{s.active_7d} in last 7d" }
                    }
                    div { class: "stat-tile",
                        div { class: "stat-tile-label", "Total Plays" }
                        div { class: "stat-tile-value", "{s.total_plays}" }
                    }
                }

                if s.top_users.is_empty() {
                    div { class: "empty-state", "No playback activity yet" }
                } else {
                    div { style: "margin-top:14px",
                        div { class: "card-title", style: "font-size:.78rem;margin-bottom:6px", "Top by plays" }
                        div { class: "row-list",
                            for u in s.top_users.iter().take(5).cloned() {
                                {let uid = u.user_id;
                                rsx! {
                                    div {
                                        class: "flex items-center border-b border-[var(--border)] hover:bg-[rgba(0,0,0,0.03)]",
                                        key: "{uid}",
                                        div { class: "flex-1 min-w-0 px-3 py-[8px]",
                                            button {
                                                style: "background:none;border:none;padding:0;cursor:pointer;color:inherit;font-size:.84rem;font-weight:600;text-align:left",
                                                title: "View details",
                                                onclick: move |_| {
                                                    navigator().replace(Route::AccessUserDetailRoute { id: uid });
                                                },
                                                "{u.username}"
                                            }
                                        }
                                        div { class: "shrink-0 px-3 py-[8px] text-right",
                                            span { class: "user-meta", "{u.total_plays} plays" }
                                        }
                                    }
                                }}
                            }
                        }
                    }
                }
            }
        }
    }
}
