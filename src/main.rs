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
    log!("Done Getting environment");

    let names = LocalResource::new(move || {
        async move { fetch_names(&supabase_url, &publishable_api_key).await }
    });

    view! {

        <h1> "Hewwo World" </h1>

        <Transition fallback = move || view! {<p>"Loading names..."</p>}>
            {move || {
                match names.get()
                {
                    Some(names) => {
                        match names
                        {
                            Ok(names) => {
                                names.into_iter().map(|entry| view! {<li>{entry.name}</li>}.into_any()).collect_view()
                            },
                            Err(err_msg) =>
                            {
                                vec![view! {<p>format!("Failed due to {}", err_msg)</p>}.into_any()]
                            }
                        }
                    },
                    None => vec![view!{<p>"Failed to get names... (None)"</p>}.into_any()],
                }

            }}
        </Transition>

    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct NameEntry {
    id: i32,
    name: String,
}

async fn fetch_names(url: &str, api_key: &str) -> Result<Vec<NameEntry>, String> {
    // Address and publishable api key allowable for in browser use with rls
    let table_url = format!("{}/rest/v1/names?select=*", url);
    log!("Fetching begins");

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
