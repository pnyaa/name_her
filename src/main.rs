use gloo_net::http::Request;
use leptos::logging::log;
use leptos::prelude::*;
use leptos::{mount::mount_to_body, view, *};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
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
    let iick_manager = IickManager::new();
    provide_context(iick_manager.clone());


    view! {
        <TitleEntry/>
        <div class="main">

        <NameFilteringDisplayRenderer value=filtering />
        <SuggestionsRenderer value = suggestions />
        <IickReasonRenderer value = iick_manager />

        </div>

    }
}

// -----------------------------------------------------------------------------------------------
// Database context, for sharing public credentials
//

/// # DatabaseDetails
/// We use this to store the public auth for the database
#[derive(Clone)]
struct DatabaseDetails {
    pub url: &'static str,
    pub api: &'static str,

    /// We use local storage to create a uuid, it doesn't contain personal info
    /// but just gives an id a "good enough" identifier so people won't 
    /// accidentally vote twice
    pub uuid: String,
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
            uuid: Self::get_or_make_uuid(),
        }
    }

    fn get_or_make_uuid() -> String 
    {
        let storage = leptos_use::use_window().as_ref().unwrap().local_storage().ok().flatten();
        match storage
        {
            Some(storage) => {

                if let Ok(Some(id)) = storage.get_item("naming_device_id"){
                    log!("Reusing found id {}", id);
                    return id;
                }
                
                let new_id = Uuid::new_v4().to_string();
                match storage.set_item("naming_device_id", &new_id)
                {
                    Ok(_) => log!("Success making new id {}", new_id),
                    Err(e) => log!("Couldn't make new id due to {:#?}", e),
                };
                new_id.to_string()
            },
            None => {
                log!("No storage, using anon");
                "anonymous".to_string()
            },
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
        //if cfg!(debug_assertions) {
            get_mock_data().await
        //} else {
        //    Self::get_real_data().await
        //}
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
    icon: char,
    name: String,
    notes: String,
    love_count: ReadSignal<u32>,
    love_count_set: WriteSignal<u32>,
    like_count: ReadSignal<u32>,
    like_count_set: WriteSignal<u32>,
    dislike_count: ReadSignal<u32>,
    dislike_count_set: WriteSignal<u32>,
    iick_count: ReadSignal<u32>,
    iick_count_set: WriteSignal<u32>,
    selected_vote: ReadSignal<Option<char>>,
    selected_vote_set: WriteSignal<Option<char>>,
    is_rejected: bool,
    is_favourite: bool,
}

impl NameEntry {
    pub fn from_db(entry: NameEntryRawDb) -> NameEntry {
        let (love_count, love_count_set) = signal(entry.love_count.map_or(0, |v| v));
        let (like_count, like_count_set) = signal(entry.like_count.map_or(0, |v| v));
        let (dislike_count, dislike_count_set) = signal(entry.dislike_count.map_or(0, |v| v));
        let (iick_count, iick_count_set) = signal(entry.iick_count.map_or(0, |v| v));
        let (selected_vote, selected_vote_set) = signal(None::<char>);

        let mut n = NameEntry {
            id: entry.id,
            name: entry.name,
            notes: entry.notes.map_or("".to_string(), |v| v),
            love_count,
            love_count_set,
            like_count,
            like_count_set,
            dislike_count,
            dislike_count_set,
            iick_count,
            iick_count_set,
            selected_vote,
            selected_vote_set,
            is_rejected: entry.is_rejected.map_or(false, |v| v),
            is_favourite: entry.is_favourite.map_or(false, |v| v),
            icon: ' ',
        };

        n.icon = match n.is_favourite {
            true => '⭐',
            false => match n.is_rejected {
                true => '❌',
                false => '\u{2001}',
            },
        };

        n
    }

    pub fn on_click(&self, which_button: char) {
        let current_vote = self.selected_vote.get();
        if current_vote == Some(which_button) {
            return;
        }

        if let Some(prev_vote) = current_vote {
            match prev_vote {
                '💖' => self.love_count_set.set(self.love_count.get().saturating_sub(1)),
                '👍' => self.like_count_set.set(self.like_count.get().saturating_sub(1)),
                '👎' => self.dislike_count_set.set(self.dislike_count.get().saturating_sub(1)),
                '🤢' => self.iick_count_set.set(self.iick_count.get().saturating_sub(1)),
                _ => {}
            }
        }

        match which_button {
            '💖' => self.love_count_set.set(self.love_count.get() + 1),
            '👍' => self.like_count_set.set(self.like_count.get() + 1),
            '👎' => self.dislike_count_set.set(self.dislike_count.get() + 1),
            '🤢' => self.iick_count_set.set(self.iick_count.get() + 1),
            _ => {}
        }

        self.selected_vote_set.set(Some(which_button));
    }

    pub fn into_table_row(self) -> impl IntoView {
        let icon = self.icon;
        let name = self.name.clone();
        let name_for_iick = self.name;
        let notes = self.notes;
        let love_count = self.love_count;
        let like_count = self.like_count;
        let dislike_count = self.dislike_count;
        let iick_count = self.iick_count;
        let selected_vote = self.selected_vote;
        let selected_vote_set = self.selected_vote_set;
        let love_count_set = self.love_count_set;
        let like_count_set = self.like_count_set;
        let dislike_count_set = self.dislike_count_set;
        let iick_count_set = self.iick_count_set;

        let on_love = move |_| {
            let current = selected_vote.get();
            if current == Some('💖') {
                love_count_set.set(love_count.get().saturating_sub(1));
                selected_vote_set.set(None);
                return;
            }
            if current == Some('👍') {
                like_count_set.set(like_count.get().saturating_sub(1));
            } else if current == Some('👎') {
                dislike_count_set.set(dislike_count.get().saturating_sub(1));
            } else if current == Some('🤢') {
                iick_count_set.set(iick_count.get().saturating_sub(1));
            }
            love_count_set.set(love_count.get() + 1);
            selected_vote_set.set(Some('💖'));
        };
        let on_like = move |_| {
            let current = selected_vote.get();
            if current == Some('👍') {
                like_count_set.set(like_count.get().saturating_sub(1));
                selected_vote_set.set(None);
                return;
            }
            if current == Some('💖') {
                love_count_set.set(love_count.get().saturating_sub(1));
            } else if current == Some('👎') {
                dislike_count_set.set(dislike_count.get().saturating_sub(1));
            } else if current == Some('🤢') {
                iick_count_set.set(iick_count.get().saturating_sub(1));
            }
            like_count_set.set(like_count.get() + 1);
            selected_vote_set.set(Some('👍'));
        };
        let on_dislike = move |_| {
            let current = selected_vote.get();
            if current == Some('👎') {
                dislike_count_set.set(dislike_count.get().saturating_sub(1));
                selected_vote_set.set(None);
                return;
            }
            if current == Some('💖') {
                love_count_set.set(love_count.get().saturating_sub(1));
            } else if current == Some('👍') {
                like_count_set.set(like_count.get().saturating_sub(1));
            } else if current == Some('🤢') {
                iick_count_set.set(iick_count.get().saturating_sub(1));
            }
            dislike_count_set.set(dislike_count.get() + 1);
            selected_vote_set.set(Some('👎'));
        };
        let on_iick = move |_| {
            let current = selected_vote.get();
            if current == Some('🤢') {
                iick_count_set.set(iick_count.get().saturating_sub(1));
                selected_vote_set.set(None);
                return;
            }
            if current == Some('💖') {
                love_count_set.set(love_count.get().saturating_sub(1));
            } else if current == Some('👍') {
                like_count_set.set(like_count.get().saturating_sub(1));
            } else if current == Some('👎') {
                dislike_count_set.set(dislike_count.get().saturating_sub(1));
            }
            iick_count_set.set(iick_count.get() + 1);
            selected_vote_set.set(Some('🤢'));
            
            // Auto-fill the iick reason box with the name and trigger focus
            if let Some(iick_mgr) = use_context::<IickManager>() {
                log!("Triggering iick focus");
                iick_mgr.name_write.set(name_for_iick.clone());
                iick_mgr.focus_trigger_write.set(iick_mgr.focus_trigger.get() + 1);
            }
        };

        let selected = selected_vote.get();
        view! {
            <tr>
                <td class="status-cell">{icon}</td>
                <td class="name-cell">{name}</td>
                <td class="rating-cell">
                    <div class="rating-content">
                        <button on:click=on_love>"💖"</button>
                        {if selected == Some('💖') {
                            view! { <strong>{love_count.get()}</strong> }.into_any()
                        } else {
                            view! { <span>{love_count.get()}</span> }.into_any()
                        }}
                        <button on:click=on_like>"👍"</button>
                        {if selected == Some('👍') {
                            view! { <strong>{like_count.get()}</strong> }.into_any()
                        } else {
                            view! { <span>{like_count.get()}</span> }.into_any()
                        }}
                        <button on:click=on_dislike>"👎"</button>
                        {if selected == Some('👎') {
                            view! { <strong>{dislike_count.get()}</strong> }.into_any()
                        } else {
                            view! { <span>{dislike_count.get()}</span> }.into_any()
                        }}
                        <button on:click=on_iick>"🤢"</button>
                        {if selected == Some('🤢') {
                            view! { <strong>{iick_count.get()}</strong> }.into_any()
                        } else {
                            view! { <span>{iick_count.get()}</span> }.into_any()
                        }}
                    </div>
                </td>
                <td class="notes-cell">{notes}</td>
            </tr>
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
                            view! {
                                <table class="name-table">
                                <tr>
                                    <th class="status-cell">Status</th>
                                    <th class="name-cell">Name</th>
                                    <th class="rating-cell">Rating</th>
                                    <th class="notes-cell">Notes</th>
                                </tr>
                                
                                {names.into_iter().map(|entry| entry.into_table_row().into_any()).collect_view()}
                                
                                </table>
                            }.into_any()
                                },
                                Err(err_msg) =>
                                {
                                    view! {<p>{format!("Failed due to {}", err_msg)}</p>}.into_any()
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

#[derive(Clone)]
struct IickManager {
    pub name_read: ReadSignal<String>,
    pub name_write: WriteSignal<String>,

    pub reason_read: ReadSignal<String>,
    pub reason_write: WriteSignal<String>,
    
    pub focus_trigger: ReadSignal<i32>,
    pub focus_trigger_write: WriteSignal<i32>,
}

impl IickManager {
    fn new() -> Self {
        let (name_read, name_write) = signal(String::new());
        let (reason_read, reason_write) = signal(String::new());
        let (focus_trigger, focus_trigger_write) = signal(0);
        IickManager {
            name_read,
            name_write,
            reason_read,
            reason_write,
            focus_trigger,
            focus_trigger_write,
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

#[component]
fn IickReasonRenderer(value: IickManager) -> impl IntoView {
    let reason_input_ref = NodeRef::<html::Input>::new();
    
    let on_submit = move |_| {
        let name = value.name_read.get();
        let reason = value.reason_read.get();

        log!("Iick reason recieved for \"{}\" : \"{}\"", name, reason);
        
        // Clear the form after submission
        value.name_write.set(String::new());
        value.reason_write.set(String::new());
    };
    
    // Watch the focus trigger signal and focus the input when it changes
    // The focusing is an atomic counter, and triggers on 0 when the page loads
    Effect::new(move || {
        let v = value.focus_trigger.get();
        if v == 0i32 {return;}
        log!("Focusing with value {}", v);
        if let Some(input) = reason_input_ref.get() {
            let _ = input.focus();
        }
    });

    view! {
        <div>
            <label class="sleek-checkbox">
            <h2> "Got the iick?"</h2>
            </label>
        </div>

        <form on:submit = move |e| {
                e.prevent_default();
                on_submit(());
            }>

        <div class="input-group">
        <SleekTextInput
            placeholder="Name"
            value=value.name_read
            set_value=value.name_write
        />

        <button type="submit" class="sleek-button">
            "Submit"
        </button>
        </div>

        <input
            node_ref=reason_input_ref
            type="text"
            class="sleek-input"
            placeholder="Why does it give the iick?"
            prop:value=value.reason_read
            on:input=move |e| value.reason_write.set(event_target_value(&e))
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
            is_favourite: Some(true),
            notes: Some("Currently her favourite .................. ........... ......... ............ .............. ...... ............... .".to_string()),
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
            id: 3,
            name: "Lina".to_string(),
            is_rejected: Some(true),
            notes: Some("There's another transgender girl called Lina who is in graphics programming, so that feels pretty taken".to_string()),
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
