use gloo_net::http::Request;
use leptos::logging::log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::{mount::mount_to_body, view, *};
use postgrest::Postgrest;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use uuid::Uuid;

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
        <p class="intro-text">
        "Help me pick my new name, or at least stop a silly goose from picking a cringe one! "
        "Feel free to suggest new names, score your favourites, dislikes and your iicks! "

        "(I am mainly trying to avoid iick names, so if possible please tell me why a name gives you the iick in the feedback form) 🪿"
        </p>
        <NameFilteringDisplayRenderer value=filtering />
        <SuggestionsRenderer value = suggestions />
                <IickReasonRenderer value = iick_manager />
                { if cfg!(debug_assertions) {
                        view! { <DebugAdminRenderer value = DebugAdminManager::new() /> }.into_any()
                    } else {
                        view! {}.into_any()
                    }
                }

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

    /// Postgrest client for creating queries
    pub client: Postgrest,
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
            client: Postgrest::new(format!("{}/rest/v1", supabase_url))
                .insert_header("apikey", publishable_api_key)
                .insert_header("Authorization", format!("Bearer {}", publishable_api_key)),
        }
    }

    fn get_or_make_uuid() -> String {
        let storage = leptos_use::use_window()
            .as_ref()
            .unwrap()
            .local_storage()
            .ok()
            .flatten();
        match storage {
            Some(storage) => {
                if let Ok(Some(id)) = storage.get_item("naming_device_id") {
                    log!("Reusing found id {}", id);
                    return id;
                }

                let new_id = Uuid::new_v4().to_string();
                match storage.set_item("naming_device_id", &new_id) {
                    Ok(_) => log!("Success making new id {}", new_id),
                    Err(e) => log!("Couldn't make new id due to {:#?}", e),
                };
                new_id.to_string()
            }
            None => {
                log!("No storage, using anon");
                "anonymous".to_string()
            }
        }
    }

    pub async fn submit_async(self, table: String, name: String, notes: String) {
        let resp = self
            .client
            .from(table)
            .insert(
                serde_json::json!({
                    "name": name,
                    "notes": notes
                })
                .to_string(),
            )
            .execute()
            .await;

        match resp {
            Ok(resp) => log!(
                "Success with {}",
                match resp.text().await {
                    Ok(e) => e,
                    Err(err) => format!("unwrap err {}", err),
                }
            ),
            Err(err) => log!("Error while inserting {}", err.to_string()),
        }
    }

    pub fn submit(self, table: String, name: String, notes: String) {
        spawn_local(self.submit_async(table, name, notes));

        let _ = web_sys::window()
            .unwrap()
            .alert_with_message("Thank yooouu");
    }

    pub async fn vote_async(self, name_id: i32, vote: Option<char>) {
        log!("Voting for name {} with value {:#?}", name_id, vote);
        let resp = self
            .client
            .from("votes")
            .upsert(
                serde_json::json!(
                    {
                        "device_uuid": self.uuid,
                        "name_id": name_id,
                        "vote_kind": match vote {
                            Some('💖') => "LOVE",
                            Some('👍') => "LIKE",
                            Some('👎') => "DISLIKE",
                            Some('🤢') => "IICK",
                            _ => "NONE",
                        }
                    }
                )
                .to_string(),
            )
            .on_conflict("device_uuid,name_id")
            .execute()
            .await;

        match resp {
            Ok(resp) => log!(
                "Success with {}",
                match resp.text().await {
                    Ok(e) => e,
                    Err(err) => format!("unwrap err {}", err),
                }
            ),
            Err(err) => log!("Error while inserting {}", err.to_string()),
        }
    }

    pub fn vote(self, name_id: i32, vote: Option<char>) {
        spawn_local(self.vote_async(name_id, vote));
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
                // Fetch the list of names
                let mut v = raw_list
                    .into_iter()
                    .map(|raw| NameEntry::from_db(raw))
                    .collect::<Vec<_>>();

                // Now we get the list of votes from this device. But we always return the
                // array of names, either with device votes or without
                Ok(Rc::new(match VotesEntryRawDb::get_data().await {
                    Ok(votes) => {
                        log!("Penis");
                        for vote in &votes {
                            v.iter_mut()
                                .filter(|f| f.id == vote.name_id)
                                .for_each(|f| match &vote.vote_kind {
                                    Some(vote) => {
                                        log!("Nyaaa");
                                        (*f).selected_vote_set.set(match vote.as_str() {
                                            "LOVE" => Some('💖'),
                                            "LIKE" => Some('👍'),
                                            "DISLIKE" => Some('👎'),
                                            "IICK" => Some('🤢'),
                                            vote => {
                                                log!("User had unknown vote {}", vote);
                                                None
                                            }
                                        });
                                    }
                                    None => {}
                                });
                        }

                        v
                    }
                    Err(_) => v,
                }))
            }
            Err(e) => Err(e),
        };

        NameManager { list }
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
        let (love_count, love_count_set) = signal(entry.vote_love.map_or(0, |v| v));
        let (like_count, like_count_set) = signal(entry.vote_like.map_or(0, |v| v));
        let (dislike_count, dislike_count_set) = signal(entry.vote_dislike.map_or(0, |v| v));
        let (iick_count, iick_count_set) = signal(entry.vote_iick.map_or(0, |v| v));
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
        log!("Which button {:#?}", which_button);

        let db = use_context::<DatabaseDetails>().expect("Failed to get the database details");

        // If user clicked the same button again, toggle it off
        if current_vote == Some(which_button) {
            match which_button {
                '💖' => self
                    .love_count_set
                    .set(self.love_count.get().saturating_sub(1)),
                '👍' => self
                    .like_count_set
                    .set(self.like_count.get().saturating_sub(1)),
                '👎' => self
                    .dislike_count_set
                    .set(self.dislike_count.get().saturating_sub(1)),
                '🤢' => self
                    .iick_count_set
                    .set(self.iick_count.get().saturating_sub(1)),
                _ => {}
            }
            self.selected_vote_set.set(None);
            db.vote(self.id, None);
            return;
        }

        // If there was a previous vote, decrement it
        if let Some(prev_vote) = current_vote {
            match prev_vote {
                '💖' => self
                    .love_count_set
                    .set(self.love_count.get().saturating_sub(1)),
                '👍' => self
                    .like_count_set
                    .set(self.like_count.get().saturating_sub(1)),
                '👎' => self
                    .dislike_count_set
                    .set(self.dislike_count.get().saturating_sub(1)),
                '🤢' => self
                    .iick_count_set
                    .set(self.iick_count.get().saturating_sub(1)),
                _ => {}
            }
        }

        // Apply new vote and handle iick special-case (focus + autofill)
        match which_button {
            '💖' => {
                self.love_count_set.set(self.love_count.get() + 1);
            }
            '👍' => {
                self.like_count_set.set(self.like_count.get() + 1);
            }
            '👎' => {
                self.dislike_count_set.set(self.dislike_count.get() + 1);
            }
            '🤢' => {
                self.iick_count_set.set(self.iick_count.get() + 1);
                if let Some(iick_mgr) = use_context::<IickManager>() {
                    log!("Triggering iick focus");
                    iick_mgr.name_write.set(self.name.clone());
                    iick_mgr
                        .focus_trigger_write
                        .set(iick_mgr.focus_trigger.get() + 1);
                }
            }
            _ => {
                log!("None");
            }
        }

        db.vote(self.id, Some(which_button));
        self.selected_vote_set.set(Some(which_button));
    }

    pub fn into_table_row(self) -> impl IntoView {
        let me = self.clone();

        let icon = self.icon;
        let name = self.name.clone();
        let notes = self.notes;
        let love_count = self.love_count;
        let like_count = self.like_count;
        let dislike_count = self.dislike_count;
        let iick_count = self.iick_count;
        let selected_vote = self.selected_vote;

        let me_love = me.clone();
        let on_love = move |_| {
            me_love.on_click('💖');
        };

        let me_like = me.clone();
        let on_like = move |_| {
            me_like.on_click('👍');
        };

        let me_dislike = me.clone();
        let on_dislike = move |_| {
            me_dislike.on_click('👎');
        };

        let me_iick = me.clone();
        let on_iick = move |_| {
            me_iick.on_click('🤢');
        };

        let selected = selected_vote.get();
        // MASSIVE OVERKILL BUT I COULDNL'T FIGURE OUT PROPER WAY
        view! {
            <tr>
                <td class="status-cell">{icon}</td>
                <td class="name-cell">{name}</td>
                <td class="rating-cell">
                    <div class="rating-content">                        

                        {if selected == Some('💖') {
                            view! {
                                <button class="selected-emoji" on:click=on_love>"💖"</button>
                                <strong>{love_count.get()}</strong>
                                }.into_any()
                            }
                        else {
                            view! {
                                <button class="unselected-emoji" on:click=on_love>"💖"</button>
                                <span>{love_count.get()}</span>
                                }.into_any()
                        }}


                        {if selected == Some('👍') {
                            view! {
                                <button class="selected-emoji" on:click=on_like>"👍"</button>
                                <strong>{like_count.get()}</strong>
                            }.into_any()
                        }
                        else {
                            view! {
                                <button class="unselected-emoji" on:click=on_like>"👍"</button>
                                <span>{like_count.get()}</span>
                            }.into_any()
                        }}


                        {if selected == Some('👎') {
                            view! {
                                <button class="selected-emoji" on:click=on_dislike>"👎"</button>
                                <strong>{dislike_count.get()}</strong>
                            }.into_any()
                        }
                        else {
                            view! {
                                <button class="unselected-emoji" on:click=on_dislike>"👎"</button>
                                <span>{dislike_count.get()}</span>
                            }.into_any()
                        }}


                        {if selected == Some('🤢') {
                            view! {
                                <button class="selected-emoji" on:click=on_iick>"🤢"</button>
                                <strong>{iick_count.get()}</strong>
                            }.into_any()
                        }
                        else {
                            view! {
                                <button class="unselected-emoji" on:click=on_iick>"🤢"</button>
                                <span>{iick_count.get()}</span>
                            }.into_any()
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

struct NameFilteringDisplay {
    pub filter_query: ReadSignal<String>,
    pub set_filter_query: WriteSignal<String>,

    pub show_rejected: ReadSignal<bool>,
    pub set_show_rejected: WriteSignal<bool>,

    pub sort_mode: ReadSignal<String>,
    pub set_sort_mode: WriteSignal<String>,

    pub show_favourites: ReadSignal<bool>,
    pub set_show_favourites: WriteSignal<bool>,
}

impl NameFilteringDisplay {
    pub fn new() -> NameFilteringDisplay {
        let (filter_query, set_filter_query) = signal(String::new());
        let (show_rejected, set_show_rejected) = signal(false);
        let (sort_mode, set_sort_mode) = signal(String::from("alpha"));
        let (show_favourites, set_show_favourites) = signal(false);

        NameFilteringDisplay {
            filter_query,
            set_filter_query,
            show_rejected,
            set_show_rejected,
            sort_mode,
            set_sort_mode,
            show_favourites,
            set_show_favourites,
        }
    }

    pub fn into_view(self: Self) -> impl IntoView {
        let filtered_names = move || {
            let q = self.filter_query.get().to_lowercase();
            let sort = self.sort_mode.get();
            let show_rej = self.show_rejected.get();
            let show_fav = self.show_favourites.get();

            let name_manager = use_context::<NameResource>().expect("Database should exist");

            match name_manager.0.get() {
                Some(manager) => match manager.list {
                    Ok(names) => {
                        let mut v = names
                            .iter()
                            .filter(|n| {
                                if n.is_rejected && (!show_rej) {
                                    return false;
                                }
                                if show_fav && !n.is_favourite {
                                    return false;
                                }
                                n.name.to_lowercase().contains(&q)
                            })
                            .cloned()
                            .collect::<Vec<_>>();

                        if sort == "score" {
                            v.sort_by(|a, b| {
                                let sa = (a.love_count.get() as i32) * 3
                                    + (a.like_count.get() as i32)
                                    - (a.dislike_count.get() as i32)
                                    - (a.iick_count.get() as i32) * 6;
                                let sb = (b.love_count.get() as i32) * 3
                                    + (b.like_count.get() as i32)
                                    - (b.dislike_count.get() as i32)
                                    - (b.iick_count.get() as i32) * 6;
                                match sb.cmp(&sa) {
                                    std::cmp::Ordering::Equal => {
                                        a.name.to_lowercase().cmp(&b.name.to_lowercase())
                                    }
                                    ord => ord,
                                }
                            });
                        } else {
                            v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                        }

                        Ok(v)
                    }
                    Err(e) => Err(e),
                },
                None => Err("Failed to fetch unwrap".to_string()),
            }
        };

        let sort_mode = self.sort_mode;
        let set_sort_mode = self.set_sort_mode;
        let show_favourites = self.show_favourites;
        let set_show_favourites = self.set_show_favourites;

        view! {
            <SleekTextInput placeholder="Search names" value=self.filter_query set_value=self.set_filter_query />
            <div>
            <table>
            <tr>
            <th> <ShowSleekBox label=" Show rejected names".to_string() value=self.show_rejected set_value=self.set_show_rejected /> </th>
            <th> <ShowSleekBox label=" Show only favourites".to_string() value=self.show_favourites set_value=self.set_show_favourites /> </th>
            </tr>
            <tr>
                <div class="sleek-checkbox">
                <label class="sort-select">
                    "    Sort: "
                    <select prop:value=sort_mode on:change=move |e| set_sort_mode.set(event_target_value(&e))>
                        <option value="alpha">"Alphabetical"</option>
                        <option value="score">"Score total"</option>
                    </select>
                </label>
            </div>
            <th>
            </th>
            </tr>
            </table>
            </div>
            <NamesList names=filtered_names />
        }
    }
}

#[component]
fn NameFilteringDisplayRenderer(value: NameFilteringDisplay) -> impl IntoView {
    value.into_view()
}

// -----------------------------------------------------------------------------------------------
// Suggestions manager
//

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

    fn into_view(self) -> impl IntoView {
        // Function to act the spawn the form submission
        let on_click = move |_| {
            let suggestion = self.suggestion_read.get();
            let notes = self.notes_read.get();
            log!("Suggestion recieved \"{}\" : \"{}\"", suggestion, notes);
            if suggestion.is_empty() {
                let _ = web_sys::window()
                    .unwrap()
                    .alert_with_message("Please enter a name to submit suggestion");
                return;
            }

            use_context::<DatabaseDetails>()
                .expect("Failed to get the database details")
                .submit("suggestions".to_string(), suggestion, notes);
            self.suggestion_write.set(String::new());
            self.notes_write.set(String::new());
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
                value=self.suggestion_read
                set_value=self.suggestion_write
            />

            <button type="submit" class="sleek-button">
                "Suggest"
            </button>
            </div>

            <SleekTextInput
                placeholder="Notes (From who? Why? etc.)"
                value=self.notes_read
                set_value=self.notes_write
            />

            </form>

        }
    }
}

#[component]
fn SuggestionsRenderer(value: SuggestionsManager) -> impl IntoView {
    value.into_view()
}

// -----------------------------------------------------------------------------------------------
// Iick manager
//

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

    fn into_view(self) -> impl IntoView {
        let reason_input_ref = NodeRef::<html::Input>::new();

        let on_submit = move |_| {
            let name = self.name_read.get();
            let reason = self.reason_read.get();

            log!("Iick reason recieved for \"{}\" : \"{}\"", name, reason);

            if name.is_empty() {
                let _ = web_sys::window()
                    .unwrap()
                    .alert_with_message("Please enter a name to submit iick");
                return;
            }

            use_context::<DatabaseDetails>()
                .expect("Failed to get the database details")
                .submit("iicks".to_string(), name, reason);

            // Clear the form after submission
            self.name_write.set(String::new());
            self.reason_write.set(String::new());
        };

        // Watch the focus trigger signal and focus the input when it changes
        // The focusing is an atomic counter, and triggers on 0 when the page loads
        Effect::new(move || {
            let v = self.focus_trigger.get();
            if v == 0i32 {
                return;
            }
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
                value=self.name_read
                set_value=self.name_write
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
                prop:value=self.reason_read
                on:input=move |e| self.reason_write.set(event_target_value(&e))
            />

            </form>

        }
    }
}

#[component]
fn IickReasonRenderer(value: IickManager) -> impl IntoView {
    value.into_view()
}

// -----------------------------------------------------------------------------------------------
// Debug admin manager
//

#[derive(Clone)]
struct DebugAdminManager {

    // Persistent api key
    pub admin_key_read: ReadSignal<String>,
    pub admin_key_write: WriteSignal<String>,

    // Subcomponents
    pub add_name: AddName,
    pub sugg_review: SuggestionsReview,
    pub iick_review: IickReview,
}

impl DebugAdminManager {
    fn new() -> Self {
        let (admin_key_read, admin_key_write) = signal(String::new());
        
        DebugAdminManager {
            add_name: AddName::new(),
            sugg_review: SuggestionsReview::new(),
            iick_review: IickReview::new(),
            admin_key_read,
            admin_key_write,
        }
    }

    fn into_view(self) -> impl IntoView {
        view! {

            <SleekPasswordInput
                placeholder="Admin key"
                value=self.admin_key_read
                set_value=self.admin_key_write
            />

            {self.add_name.into_view(self.admin_key_read)}
            {self.sugg_review.into_view(self.admin_key_read)}
            {self.iick_review.into_view(self.admin_key_read)}
        }
    }
}

#[component]
fn DebugAdminRenderer(value: DebugAdminManager) -> impl IntoView {
    view!{
        <br/>
        <br/>
        <details>
            <summary class = "intro-text">
                "Admin panel toggle"
            </summary>
            {value.into_view()}
        </details>
    }
}

// -----------------------------------------------------------------------------------------------
// Debug Add name
//
#[derive(Clone)]
struct AddName
{
    pub name_read: ReadSignal<String>,
    pub name_write: WriteSignal<String>,

    pub notes_read: ReadSignal<String>,
    pub notes_write: WriteSignal<String>,

    pub is_rejected_read: ReadSignal<bool>,
    pub is_rejected_write: WriteSignal<bool>,

    pub is_favourite_read: ReadSignal<bool>,
    pub is_favourite_write: WriteSignal<bool>,
}

impl AddName
{
    pub fn new() -> AddName
    {
        let (name_read, name_write) = signal(String::new());
        let (notes_read, notes_write) = signal(String::new());
        let (is_rejected_read, is_rejected_write) = signal(false);
        let (is_favourite_read, is_favourite_write) = signal(false);
        AddName {  
            name_read,
            name_write,
            notes_read,
            notes_write,
            is_rejected_read,
            is_rejected_write,
            is_favourite_read,
            is_favourite_write,
        }
    }

    pub fn submit(self, api_key: ReadSignal<String>)
    {
        let api_key = api_key.get();
        let name = self.name_read.get();
        let notes = self.notes_read.get();
        let is_rejected = self.is_rejected_read.get();
        let is_favourite = self.is_favourite_read.get();

            log!(
                "Admin add name \"{}\" (rejected: {}, fav: {})",
                name,
                is_rejected,
                is_favourite
            );

            if name.is_empty() {
                let _ = web_sys::window()
                    .unwrap()
                    .alert_with_message("Please enter a name to add");
                return;
            }

            let db = use_context::<DatabaseDetails>().expect("Failed to get the database details");

            // Clone signals we need inside the async block
            let admin_key_clone = api_key.clone();
            let body = serde_json::json!({
                "name": name,
                "notes": notes,
                "is_rejected": is_rejected,
                "is_favourite": is_favourite
            })
            .to_string();

            let name_write_c = self.name_write.clone();
            let notes_write_c = self.notes_write.clone();
            let is_rejected_write_c = self.is_rejected_write.clone();
            let is_favourite_write_c = self.is_favourite_write.clone();

            spawn_local(async move {
                // Use Postgrest client and apply the provided admin key for this request
                let client = Postgrest::new(format!("{}/rest/v1", db.url))
                    //.client
                    //.clone()
                    .insert_header("apikey", &admin_key_clone)
                    .insert_header("Authorization", &format!("Bearer {}", admin_key_clone));

                let resp = client.from("names").insert(body).execute().await;

                match resp {
                    Ok(r) => {
                        log!("Admin insert success: {:?}", r);
                        name_write_c.set(String::new());
                        notes_write_c.set(String::new());
                        is_rejected_write_c.set(false);
                        is_favourite_write_c.set(false);
                        let _ = web_sys::window()
                            .unwrap()
                            .alert_with_message("Inserted name as admin");
                    }
                    Err(err) => {
                        log!("Admin insert failed: {}", err);
                        let _ = web_sys::window()
                            .unwrap()
                            .alert_with_message(&format!("Failed to insert: {}", err));
                    }
                }
            });
    }

    pub fn into_view(self, api_key: ReadSignal<String>) -> impl IntoView
    {
        view!{
            <div class="debug-admin-panel">
                <h3 class ="intro-text">"Admin: Add name"</h3>
                <form on:submit = move |e| {
                        e.prevent_default();
                        self.clone().submit(api_key);
                    }>

                    <SleekTextInput
                        placeholder="Name"
                        value=self.name_read
                        set_value=self.name_write
                    />          

                    <SleekTextInput
                        placeholder="Notes"
                        value=self.notes_read
                        set_value=self.notes_write
                    />

                    <label class="sleek-checkbox">
                        <input
                            type="checkbox"
                            on:change=move |e| self.is_rejected_write.set(event_target_checked(&e))
                            prop:checked=self.is_rejected_read
                        />
                        " Is rejected"
                    </label>

                    <label class="sleek-checkbox">
                        <input
                            type="checkbox"
                            on:change=move |e| self.is_favourite_write.set(event_target_checked(&e))
                            prop:checked=self.is_favourite_read
                        />
                        " Is favourite"
                    </label>

                    <button type="submit" class="sleek-button">
                        "Add as admin"
                    </button>

                </form>
            </div>

        }
    }
}

// -----------------------------------------------------------------------------------------------
// Debug review suggestion
//

#[derive(Clone)]
struct SuggestionsReview
{

}

impl SuggestionsReview
{
    pub fn new() -> SuggestionsReview
    {
        SuggestionsReview {  }
    }

    pub fn into_view(self, api_key: ReadSignal<String>) -> impl IntoView
    {
        view!{

            <h3 class ="intro-text">"Admin: Review Suggestions"</h3>
        }
    }
}

// -----------------------------------------------------------------------------------------------
// iick review suggestion
//

#[derive(Clone)]
struct IickReview
{

}

impl IickReview
{
    pub fn new() -> IickReview
    {
        IickReview {  }
    }

    pub fn into_view(self, api_key: ReadSignal<String>) -> impl IntoView
    {
        view!{
            <h3 class ="intro-text">"Admin: Review Iicks"</h3>
        }
    }
}


// -----------------------------------------------------------------------------------------------
// Just inputs
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
fn SleekPasswordInput(
    placeholder: &'static str,
    value: ReadSignal<String>,
    set_value: WriteSignal<String>,
) -> impl IntoView {
    view! {
        <div class="search-container">
          <input
                class="sleek-input"
                type="password"
                placeholder=placeholder
                prop:value=value
                on:input=move |e| set_value.set(event_target_value(&e))
            />
        </div>
    }
}

#[component]
fn ShowSleekBox(
    label: String,
    value: ReadSignal<bool>,
    set_value: WriteSignal<bool>,
) -> impl IntoView {
    view! {
        <label class="sleek-checkbox">
            <input
                type="checkbox"
                on:change=move |e| {
                    set_value.set(event_target_checked(&e));
                }
                prop:checked=value
            />
            {label}
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


// -----------------------------------------------------------------------------------------------
// Database structs
//

/// # NameEntryRawDb
/// Raw optional values included in from the database
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct NameEntryRawDb {
    id: i32,
    name: String,
    notes: Option<String>,
    vote_love: Option<u32>,
    vote_like: Option<u32>,
    vote_dislike: Option<u32>,
    vote_iick: Option<u32>,
    is_rejected: Option<bool>,
    is_favourite: Option<bool>,
}

impl NameEntryRawDb {
    async fn get_data() -> Result<Vec<NameEntryRawDb>, String> {
        //if cfg!(debug_assertions) {
        //Self::get_mock_data().await
        //} else {
        Self::get_real_data().await
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

    /// # get_mock_data
    /// Backup provider for debug fetching of string data
    pub async fn get_mock_data() -> Result<Vec<NameEntryRawDb>, String> {
        let _db_details =
            use_context::<DatabaseDetails>().expect("Failed to get the database details");

        log!("Fetching begins");
        return Ok(vec![
        NameEntryRawDb {
            id: 1,
            name: "Lacy".to_string(),
            vote_love: Some(2),
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
            id: 4,
            name: "A".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 5,
            name: "AA".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 6,
            name: "B".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 7,
            name: "BB".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 8,
            name: "C".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 9,
            name: "CC".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 10,
            name: "D".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 11,
            name: "DD".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 12,
            name: "E".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 13,
            name: "EE".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 14,
            name: "F".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 15,
            name: "FF".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 16,
            name: "G".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 17,
            name: "GG".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 18,
            name: "H".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 19,
            name: "HH".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 20,
            name: "I".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 21,
            name: "II".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 22,
            name: "J".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 23,
            name: "JJ".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 24,
            name: "K".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 25,
            name: "KK".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 26,
            name: "L".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 27,
            name: "LL".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 28,
            name: "M".to_string(),
            ..Default::default()
        },
        NameEntryRawDb {
            id: 29,
            name: "MM".to_string(),
            ..Default::default()
        },
    ]);
    }
}

/// # VotesEntryRawDb
/// Raw votes from database
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct VotesEntryRawDb {
    id: i32,
    device_uuid: Option<String>,
    name_id: i32,
    vote_kind: Option<String>,
}

impl VotesEntryRawDb {
    pub async fn get_data() -> Result<Vec<VotesEntryRawDb>, String> {
        Self::get_real_data().await
    }

    async fn get_real_data() -> Result<Vec<VotesEntryRawDb>, String> {
        let db_details =
            use_context::<DatabaseDetails>().expect("Failed to get the database details");
        //let table_url = format!("{}/rest/v1/votes?select=*", db_details.url);

        let resp = match db_details
            .client
            .from("votes")
            .select("*")
            .eq("device_uuid", db_details.uuid)
            .execute()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                log!("Got an error response {}", err);
                return Err(format!("Failed to make request due to {}", err.to_string()));
            }
        };
        log!("Got response {:?}", resp);

        let votes = match serde_json::from_str::<Vec<VotesEntryRawDb>>(&resp.text().await.unwrap())
        {
            Ok(val) => val,
            Err(err) => {
                let err_str = format!("Failed to deserialize due to {}", err);
                log!("{}", err_str);
                return Err(err_str);
            }
        };

        log!("Fetched {} votes", votes.len());
        Ok(votes)
    }
}
