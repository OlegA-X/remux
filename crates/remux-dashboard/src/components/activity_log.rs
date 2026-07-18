use crate::{
    components::{Card, EmptyState, LoadingText},
    state::AppState,
};
use dioxus::prelude::*;
use remux_sdks::remux::{ActivityLogEntryDto, GetActivityLogEntries};

#[component]
pub fn ActivityLogCard(app_state: AppState) -> Element {
    let mut entries: Signal<Vec<ActivityLogEntryDto>> = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);

    use_effect(move || {
        let client = app_state
            .client
            .clone();
        spawn(async move {
            match client
                .execute(GetActivityLogEntries {
                    limit: Some(200),
                    ..Default::default()
                })
                .await
            {
                Ok(r) => {
                    entries.set(r.items);
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("Failed to fetch activity log: {e}"))),
            }
            loading.set(false);
        });
    });

    rsx! {
        Card { title: "Activity Log", tight: true,
            if *loading.read() {
                LoadingText {}
            } else if let Some(err) = error.read().as_ref() {
                span { class: "loading-text", style: "color:var(--error)", "{err}" }
            } else if entries.read().is_empty() {
                EmptyState { message: "No activity recorded yet".to_string() }
            } else {
                div { class: "data-table-container",
                    div { class: "row-list",
                        for entry in entries.read().iter() {
                            div {
                                class: "flex items-center border-b border-[var(--border)] hover:bg-[rgba(0,0,0,0.03)] even:bg-[rgba(0,0,0,0.02)]",
                                key: "{entry.id}",
                                div { class: "flex-1 min-w-0 px-3 py-[8px]",
                                    div { style: "font-size:.84rem;font-weight:600", "{entry.name}" }
                                    if let Some(ov) = &entry.short_overview {
                                        div { class: "user-meta", "{ov}" }
                                    }
                                }
                                div { class: "shrink-0 px-3 py-[8px] text-right",
                                    span { class: "user-meta", "{entry.type_}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
