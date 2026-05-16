use leptos::{mount::mount_to_body, view, *};
use leptos::prelude::ElementChild;
#[component]
fn App() -> impl IntoView {


    view! {

        <h1> "Hewwo World" </h1>

    }

}

fn main() {
    mount_to_body(|| view! {<App />})
}
