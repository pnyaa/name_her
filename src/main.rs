use gloo_net::http::Request;
use leptos::logging::log;
use leptos::prelude::*;
use leptos::{mount::mount_to_body, view, *};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

fn main() {
    mount_to_body(|| view! {<App />})
}

#[component]
fn App() -> impl IntoView {
    // Provide the database details through context so they can be fetched later
    let database = DatabaseDetails::new();
    provide_context(database);

    // Similarly, the list of names needs to be context fetchable, so we can
    // display it in the list of names and also check the suggestions
    let names_resource = LocalResource::new(move || async move { NameManager::new_async().await });
    provide_context(NameResource(names_resource.clone()));

    // Displayable features
    let filtering = NameFilteringDisplay::new();
    let suggestions = SuggestionsManager::new();


    view! {
        <TitleEntry/>
        <div class="main">

        <NameFilteringDisplayRenderer value=filtering />
        <SuggestionsRenderer value = suggestions />

        </div>

    }
}

// -----------------------------------------------------------------------------------------------
// Database context, for sharing public credentials
//

/// # DatabaseDetails
/// We use this to store the public auth for the database
#[derive(Clone, Copy)]
struct DatabaseDetails {
    pub url: &'static str,
    pub api: &'static str,
}

impl DatabaseDetails {
    fn new() -> DatabaseDetails {
        // This gets compiled into the wasm binary. They are safe to redistribute
        // but they can change over time so make them be provided as part of the env
        log!("Getting environment");
        let supabase_url = env!("SUPABASE_URL");
        let publishable_api_key = env!("SUPABASE_PUBLISHABLE_API_KEY");
        log!(
            "Done Getting environment. Server url was \"{}\"",
            supabase_url
        );

        DatabaseDetails {
            url: supabase_url,
            api: publishable_api_key,
        }
    }
}

// -----------------------------------------------------------------------------------------------
// NameResource for sharing the list of names
//

#[derive(Clone)]
struct NameResource(LocalResource<NameManager>);

#[derive(Clone)]
struct NameManager {
    pub list: Result<Rc<Vec<NameEntry>>, String>,
}

impl NameManager {
    async fn new_async() -> NameManager {

        // Get our raw database source and then extract away the optionals with default values 
        let list = match NameEntryRawDb::get_data().await {
            Ok(raw_list) => {
                
                Ok(Rc::new(raw_list.into_iter().map(|raw| NameEntry::from_db(raw)).collect::<Vec<_>>()))
                
                },
            Err(e) => Err(e),
        };

        NameManager { list }
    }
}

/// # NameEntryRawDb
/// Raw optional values included in from the database
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct NameEntryRawDb {
    id: i32,
    name: String,
    notes: Option<String>,
    love_count: Option<u32>,
    like_count: Option<u32>,
    dislike_count: Option<u32>,
    iick_count: Option<u32>,
    is_rejected: Option<bool>,
    is_favourite: Option<bool>,
}

impl NameEntryRawDb {
    async fn get_data() -> Result<Vec<NameEntryRawDb>, String> {
        if cfg!(debug_assertions) {
            get_mock_data().await
        } else {
            Self::get_real_data().await
        }
    }

    pub async fn get_real_data() -> Result<Vec<NameEntryRawDb>, String> {
        let db_details =
            use_context::<DatabaseDetails>().expect("Failed to get the database details");
        log!("Fetching begins");

        // Address and publishable api key allowable for in browser use with rls
        let table_url = format!("{}/rest/v1/names?select=*", db_details.url);

        // Request all the names from the server
        let resp = match Request::get(&table_url)
            .header("apikey", db_details.api)
            .header("Authorization", &format!("Bearer {}", db_details.api))
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
        let names = match resp.json::<Vec<NameEntryRawDb>>().await {
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
}


/// # NameEntry
/// Optionals stripped out from the database for display
#[derive(Clone)]
pub struct NameEntry {
    id: i32,
    name: String,
    notes: String,
    love_count: u32,
    like_count: u32,
    dislike_count: u32,
    iick_count: u32,
    is_rejected: bool,
    is_favourite: bool,
}

impl NameEntry
{
    pub fn from_db(entry: NameEntryRawDb) -> NameEntry
    {
        NameEntry { 
            id: entry.id, 
            name: entry.name, 
            notes: entry.notes.map_or("".to_string(), |v| v), 
            love_count: entry.love_count.map_or(0, |v|v), 
            like_count: entry.like_count.map_or(0, |v|v), 
            dislike_count: entry.dislike_count.map_or(0, |v|v), 
            iick_count: entry.iick_count.map_or(0, |v|v), 
            is_rejected: entry.is_rejected.map_or(false, |v|v ), 
            is_favourite: entry.is_favourite.map_or(false, |v|v ),  
        }
    }

}

// -----------------------------------------------------------------------------------------------
// Displaying the filtered names
//

struct NameFilteringDisplay
{
    pub filter_query: ReadSignal<String>,
    pub set_filter_query: WriteSignal<String>,

    pub show_rejected: ReadSignal<bool>,
    pub set_show_rejected: WriteSignal<bool>
}

impl NameFilteringDisplay
{
    pub fn new() -> NameFilteringDisplay
    {
        // Search bar query signals
        let (filter_query, set_filter_query) = signal(String::new());

        // Checkbox : Show rejected
        let (show_rejected, set_show_rejected) = signal(false);

        NameFilteringDisplay { filter_query, set_filter_query, show_rejected, set_show_rejected }
    }

    pub fn into_view(self: Self) -> impl IntoView
    {
        // Future searchable which responds to the search bar requests
        let filtered_names = move || {
            let q = self.filter_query.get().to_lowercase();
            log!("From the filtering search bar \"{}\"", q);

            let name_manager = use_context::<NameResource>().expect("Database should exist");

            match name_manager.0.get() {
                Some(manager) => match manager.list {
                    Ok(names) => {

                        // Extract the matching names, right now we're copying the ones that 
                        // pass the filter so that they can be displayed. 
                        // TODO: Try and remove the copy
                        let v = names
                            .iter()
                            .filter(|n| {
                                if n.is_rejected && (!self.show_rejected.get())
                                {
                                    return false;
                                }
                                n.name.to_lowercase().contains(&q)}
                        
                            )   
                            .cloned()
                            .collect::<Vec<_>>();
                        Ok(v)
                    }
                    Err(e) => Err(e),
                },
                None => Err("Failed to fetch unwrap".to_string()),
            }
        };

        view! {
            <SleekTextInput placeholder="Search names" value=self.filter_query set_value=self.set_filter_query />
            <ShowRejectedBox value=self.show_rejected set_value=self.set_show_rejected />
            <NamesList names=filtered_names />
        }
    }
}

#[component]
fn NameFilteringDisplayRenderer(value: NameFilteringDisplay) -> impl IntoView
{
    value.into_view()
}

//
//
//

#[component]
fn TitleEntry() -> impl IntoView {
    view! {
        <header class="top-bar">
            <div class="header-content">
            <img
                src = "images/headshot.jpg"
                class = "profile-circle"
            />
            <h1 class="title"> "Name this bitch 🪿"</h1>
            </div>
        </header>
    }
}

#[component]
fn SleekTextInput(
    placeholder: &'static str,
    value: ReadSignal<String>,
    set_value: WriteSignal<String>,
) -> impl IntoView {
    view! {
        <div class="search-container">
          <input
                class="sleek-input"
                type="text"
                placeholder=placeholder
                prop:value=value
                on:input=move |e| set_value.set(event_target_value(&e))
            />
        </div>

    }
}

#[component]
fn ShowRejectedBox(value: ReadSignal<bool>, set_value: WriteSignal<bool>) -> impl IntoView {
    view! {
        <label class="sleek-checkbox">
            <input
                type="checkbox"
                on:change=move |e| {
                    set_value.set(event_target_checked(&e));
                }
                prop:checked=value
            />
            " Show rejected names"
        </label>

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

struct SuggestionsManager {
    pub suggestion_read: ReadSignal<String>,
    pub suggestion_write: WriteSignal<String>,

    pub notes_read: ReadSignal<String>,
    pub notes_write: WriteSignal<String>,
}

impl SuggestionsManager {
    fn new() -> Self {
        let (sugg_read, sugg_write) = signal(String::new());
        let (notes_read, notes_write) = signal(String::new());
        SuggestionsManager {
            suggestion_read: sugg_read,
            suggestion_write: sugg_write,
            notes_read,
            notes_write,
        }
    }
}

#[component]
fn SuggestionsRenderer(value: SuggestionsManager) -> impl IntoView {
    // Function to act the spawn the form submission
    let on_click = move |_| {
        let suggestion = value.suggestion_read.get();
        let notes = value.notes_read.get();

        log!("Suggestion recieved \"{}\" : \"{}\"", suggestion, notes);
    };

    view! {
        <div>
            <label class="sleek-checkbox">
            <h2> "Suggestions?"</h2>
            </label>
        </div>

        <form on:submit = move |e| {
                e.prevent_default();
                on_click(());
            }>

        <div class="input-group">
        <SleekTextInput
            placeholder="Suggested name"
            value=value.suggestion_read
            set_value=value.suggestion_write
        />

        <button type="submit" class="sleek-button">
            "Suggest"
        </button>
        </div>

        <SleekTextInput
            placeholder="Notes (From who? Why? etc.)"
            value=value.notes_read
            set_value=value.notes_write
        />

        </form>

    }
}

//
//
//

/// # get_mock_data
/// Backup provider for debug fetching of string data
pub async fn get_mock_data() -> Result<Vec<NameEntryRawDb>, String> {
    let _db_details = use_context::<DatabaseDetails>().expect("Failed to get the database details");

    log!("Fetching begins");
    return Ok(vec![
        NameEntryRawDb {
            id: 1,
            name: "Lacy".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "Laurel".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "Lexie".to_string(),
            is_rejected: Some(true),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "A".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "AA".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "B".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "BB".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "C".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "CC".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "D".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "DD".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "E".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "EE".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "F".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "FF".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "G".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "GG".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "H".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "HH".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "I".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "II".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "J".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "JJ".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "K".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "KK".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "L".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "LL".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "M".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 2,
            name: "MM".to_string(),
            ..Default::default()
        },
    ]);
}
