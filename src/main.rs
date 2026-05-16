use gloo_net::http::Request;
use leptos::logging::log;
use leptos::prelude::*;
use leptos::{mount::mount_to_body, view, *};
use serde::{Deserialize, Serialize};

fn main() {
    mount_to_body(|| view! {<App />})
}

#[component]
fn App() -> impl IntoView {
    // This gets compiled into the wasm binary. They are safe to redistribute
    // but they can change over time so make them be provided as part of the env
    log!("Getting environment");
    let supabase_url = env!("SUPABASE_URL");
    let publishable_api_key = env!("SUPABASE_PUBLISHABLE_API_KEY");
    log!(
        "Done Getting environment. Server url was \"{}\"",
        supabase_url
    );

    // Keep the list of names as a local resource that takes time to fetch
    let names = LocalResource::new(move || async move {
        fetch_names(&supabase_url, &publishable_api_key).await
    });

    // Search bar query signals
    let (query, set_query) = signal(String::new());

    // Future searchable which responds to the search bar requests
    let filtered_names = move || {
        let q = query.get().to_lowercase();
        log!("From the filtering search bar \"{}\"", q);

        match names.get() {
            Some(names) => match (names) {
                Ok(names) => Ok(names
                    .into_iter()
                    .filter(|n| n.name.to_lowercase().contains(&q))
                    .collect::<Vec<_>>()),
                Err(e) => Err(e),
            },
            None => Err("Failed to fetch unwrap".to_string()),
        }
    };

    view! {
        <TitleEntry/>
        <div class="main">

            <SearchBar value=query set_value=set_query />

            <NamesList names=filtered_names />

            //{AlphabetNav()}

        </div>

    }
}

#[component]
fn TitleEntry() -> impl IntoView {
    view! {
        <header class="top-bar">
            <div class="header-content">
            <img
                src = "images/headshot.jpg"
                class = "profile-circle"
            />
            <h1 class="title"> "Name this bitch :)"</h1>
            </div>
        </header>
    }
}

#[component]
fn SearchBar(value: ReadSignal<String>, set_value: WriteSignal<String>) -> impl IntoView {
    view! {
        <div class="search-container">
          <input
                class="sleek-input"
                type="text"
                placeholder="Search names"
                prop:value=value
                on:input=move |e| set_value.set(event_target_value(&e))
            />
        </div>
    }
}

#[component]
fn NamesList(
    names: impl Fn() -> Result<Vec<NameEntry>, String> + 'static + std::marker::Send,
) -> impl IntoView {
    view! {
        <div class = "scroll-viewport">

            <Transition fallback = move || view! {<p>"Loading names..."</p>}>
                {move || {
                    match names()
                    {
                        Ok(names) => {
                                    names.into_iter().map(|entry| view! {<li>{entry.name}</li>}.into_any()).collect_view()
                                },
                                Err(err_msg) =>
                                {
                                    vec![view! {<p>{format!("Failed due to {}", err_msg)}</p>}.into_any()]
                                }
                            }
                        }
                }
            </Transition>

        </div>
    }
}

#[component]
fn AlphabetNav() -> impl IntoView {
    let alphabet = 'A'..='Z';

    view! {
        <div class="alphabet-nav">
        {alphabet.map(|letter| view!{
            <button on:click=move |_| jump_to(letter)>
                {letter}
            </button>
        }).collect_view()}
        </div>
    }
}

fn jump_to(letter: char) {
    log!("Jump to {}", letter)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct NameEntry {
    id: i32,
    name: String,
}

struct NameDisplayer {
    supabase_url: &'static str,
    api: &'static str,
}

async fn fetch_names(url: &str, api_key: &str) -> Result<Vec<NameEntry>, String> {
    log!("Fetching begins");
    return Ok(vec![
        NameEntry {
            id: 1,
            name: "Lacy".to_string(),
        },
        NameEntry {
            id: 2,
            name: "Laurel".to_string(),
        },
        NameEntry {
            id: 2,
            name: "A".to_string(),
        },
        NameEntry {
            id: 2,
            name: "AA".to_string(),
        },
        NameEntry {
            id: 2,
            name: "B".to_string(),
        },
        NameEntry {
            id: 2,
            name: "BB".to_string(),
        },
        NameEntry {
            id: 2,
            name: "C".to_string(),
        },
        NameEntry {
            id: 2,
            name: "CC".to_string(),
        },
        NameEntry {
            id: 2,
            name: "D".to_string(),
        },
        NameEntry {
            id: 2,
            name: "DD".to_string(),
        },
        NameEntry {
            id: 2,
            name: "E".to_string(),
        },
        NameEntry {
            id: 2,
            name: "EE".to_string(),
        },
        NameEntry {
            id: 2,
            name: "F".to_string(),
        },
        NameEntry {
            id: 2,
            name: "FF".to_string(),
        },
        NameEntry {
            id: 2,
            name: "G".to_string(),
        },
        NameEntry {
            id: 2,
            name: "GG".to_string(),
        },
        NameEntry {
            id: 2,
            name: "H".to_string(),
        },
        NameEntry {
            id: 2,
            name: "HH".to_string(),
        },
        NameEntry {
            id: 2,
            name: "I".to_string(),
        },
        NameEntry {
            id: 2,
            name: "II".to_string(),
        },
        NameEntry {
            id: 2,
            name: "J".to_string(),
        },
        NameEntry {
            id: 2,
            name: "JJ".to_string(),
        },
        NameEntry {
            id: 2,
            name: "K".to_string(),
        },
        NameEntry {
            id: 2,
            name: "KK".to_string(),
        },
        NameEntry {
            id: 2,
            name: "L".to_string(),
        },
        NameEntry {
            id: 2,
            name: "LL".to_string(),
        },
        NameEntry {
            id: 2,
            name: "M".to_string(),
        },
        NameEntry {
            id: 2,
            name: "MM".to_string(),
        },
    ]);

    // Address and publishable api key allowable for in browser use with rls
    let table_url = format!("{}/rest/v1/names?select=*", url);

    // Request all the names from the server
    let resp = match Request::get(&table_url)
        .header("apikey", api_key)
        .header("Authorization", &format!("Bearer {}", api_key))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            log!("Got an error response {}", err);
            return Err(format!("Failed to make request due to {}", err.to_string()));
        }
    };

    log!("Got response {:?}", resp);
    let names = match resp.json::<Vec<NameEntry>>().await {
        Ok(val) => val,
        Err(err) => {
            let err_str = format!("Failed to deserialize due to {}", err);
            log!("{}", err_str);
            return Err(err_str);
        }
    };

    log!("Fetched {} names", names.iter().len());
    Ok(names)
}
