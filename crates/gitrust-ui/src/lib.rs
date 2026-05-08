use dioxus::prelude::*;

#[component]
pub fn App() -> Element {
    rsx! {
        div {
            h1 { "gitrust" }
            p { "Hello from Dioxus." }
        }
    }
}
